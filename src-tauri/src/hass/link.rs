//! One open connection to Home Assistant's WebSocket API: signing in with the token, commands
//! and their results, and the events of subscriptions.

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::time::{Duration, Instant};

use serde_json::Value;
use tungstenite::handshake::HandshakeError;
use tungstenite::{Message, WebSocket};

use super::address::Address;
use super::error::HassError;
use super::protocol::{self, Incoming, kind};
use super::status::HassFailure;
use super::stream::{Stream, is_certificate_error};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(8);
/// How long a read waits, so the worker can serve requests while no message arrives.
const POLL: Duration = Duration::from_millis(50);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// For answers that can be large: every state, a day of history.
pub const SLOW_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Link {
    socket: WebSocket<Stream>,
    next_id: u64,
    /// Events that arrived while waiting for a result: subscription id and event.
    events: VecDeque<(u64, Value)>,
    lost: bool,
}

pub type OpenError = (HassFailure, String);

impl Link {
    /// Connects and signs in, returning Home Assistant's version.
    pub fn open(
        address: &Address,
        insecure: bool,
        token: impl FnOnce() -> Option<String>,
    ) -> Result<(Self, String), OpenError> {
        let unreachable = |detail: String| (HassFailure::Unreachable, detail);
        let stream = Stream::open(address, insecure).map_err(unreachable)?;
        let timeouts = stream
            .socket()
            .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
            .and_then(|()| stream.socket().set_write_timeout(Some(HANDSHAKE_TIMEOUT)))
            .and_then(|()| stream.socket().set_nodelay(true));
        timeouts.map_err(|err| unreachable(err.to_string()))?;
        let (socket, _) =
            tungstenite::client(address.websocket(), stream).map_err(|err| match err {
                HandshakeError::Failure(tungstenite::Error::Io(io))
                    if is_certificate_error(&io) =>
                {
                    (HassFailure::Certificate, io.to_string())
                }
                HandshakeError::Failure(tungstenite::Error::Io(io)) => unreachable(io.to_string()),
                HandshakeError::Failure(other) => (
                    HassFailure::NotHomeAssistant,
                    format!("no WebSocket API there: {other}"),
                ),
                HandshakeError::Interrupted(_) => unreachable("timed out".into()),
            })?;
        let mut link = Self {
            socket,
            next_id: 0,
            events: VecDeque::new(),
            lost: false,
        };

        let hello = link.handshake_message()?;
        if hello.kind != kind::AUTH_REQUIRED {
            return Err((
                HassFailure::NotHomeAssistant,
                format!("unexpected first message: {}", hello.kind),
            ));
        }
        let Some(token) = token().filter(|t| !t.trim().is_empty()) else {
            return Err((HassFailure::TokenRequired, "no token saved".into()));
        };
        link.send(protocol::auth(token.trim()))
            .map_err(|err| (HassFailure::Other, err.to_string()))?;
        let answer = link.handshake_message()?;
        match answer.kind.as_str() {
            kind::AUTH_OK => {}
            kind::AUTH_INVALID => {
                return Err((
                    HassFailure::WrongToken,
                    answer.message.unwrap_or_else(|| "invalid token".into()),
                ));
            }
            other => {
                return Err((
                    HassFailure::Other,
                    format!("unexpected answer to the token: {other}"),
                ));
            }
        }
        let version = answer.ha_version.or(hello.ha_version).unwrap_or_default();
        link.socket
            .get_ref()
            .socket()
            .set_read_timeout(Some(POLL))
            .map_err(|err| unreachable(err.to_string()))?;
        Ok((link, version))
    }

    pub fn call(&mut self, kind: &str, fields: Value) -> Result<Value, HassError> {
        self.call_within(kind, fields, REQUEST_TIMEOUT)
    }

    /// Sends a command and waits for its result, keeping the events that arrive meanwhile.
    pub fn call_within(
        &mut self,
        kind: &str,
        fields: Value,
        timeout: Duration,
    ) -> Result<Value, HassError> {
        let id = self.send_command(kind, fields)?;
        self.result(id, timeout)
    }

    /// Starts a subscription; its events arrive under the id returned.
    pub fn subscribe(&mut self, kind: &str, fields: Value) -> Result<u64, HassError> {
        let id = self.send_command(kind, fields)?;
        self.result(id, REQUEST_TIMEOUT)?;
        Ok(id)
    }

    pub fn unsubscribe(&mut self, subscription: u64) -> Result<(), HassError> {
        self.events.retain(|(id, _)| *id != subscription);
        let fields = serde_json::json!({ "subscription": subscription });
        self.call("unsubscribe_events", fields).map(|_| ())
    }

    /// Whether Home Assistant still answers.
    pub fn ping(&mut self) -> Result<(), HassError> {
        self.call("ping", Value::Null).map(|_| ())
    }

    /// The next event of a subscription, waiting a moment for one.
    pub fn next_event(&mut self) -> Result<Option<(u64, Value)>, HassError> {
        if let Some(event) = self.events.pop_front() {
            return Ok(Some(event));
        }
        match self.receive()? {
            Some(message) if message.kind == kind::EVENT => {
                Ok(message.id.map(|id| (id, message.event)))
            }
            _ => Ok(None),
        }
    }

    pub fn is_lost(&self) -> bool {
        self.lost
    }

    pub fn close(mut self) {
        let _ = self.socket.close(None);
        let _ = self.socket.flush();
    }

    fn send_command(&mut self, kind: &str, fields: Value) -> Result<u64, HassError> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(protocol::command(id, kind, fields))?;
        Ok(id)
    }

    fn result(&mut self, id: u64, timeout: Duration) -> Result<Value, HassError> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let Some(message) = self.receive()? else {
                continue;
            };
            match (message.kind.as_str(), message.id) {
                (kind::EVENT, Some(subscription)) => {
                    self.events.push_back((subscription, message.event));
                }
                (kind::PONG, Some(answered)) if answered == id => return Ok(Value::Null),
                (kind::RESULT, Some(answered)) if answered == id => {
                    return if message.success == Some(true) {
                        Ok(message.result)
                    } else {
                        let error = message.error;
                        Err(HassError::Rejected {
                            code: error.as_ref().map(|e| e.code.clone()).unwrap_or_default(),
                            message: error
                                .map(|e| e.message)
                                .filter(|m| !m.is_empty())
                                .unwrap_or_else(|| "unknown error".into()),
                        })
                    };
                }
                _ => {}
            }
        }
        Err(HassError::Timeout)
    }

    fn send(&mut self, text: String) -> Result<(), HassError> {
        self.socket.send(Message::text(text)).map_err(|_| {
            self.lost = true;
            HassError::Lost
        })
    }

    /// One message, or `None` if none arrived within the poll interval.
    fn receive(&mut self) -> Result<Option<Incoming>, HassError> {
        match self.socket.read() {
            Ok(Message::Text(text)) => Ok(serde_json::from_str(text.as_str()).ok()),
            Ok(Message::Close(_)) => {
                self.lost = true;
                Err(HassError::Lost)
            }
            Ok(_) => Ok(None),
            Err(tungstenite::Error::Io(err))
                if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                Ok(None)
            }
            Err(_) => {
                self.lost = true;
                Err(HassError::Lost)
            }
        }
    }

    fn handshake_message(&mut self) -> Result<Incoming, OpenError> {
        let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
        while Instant::now() < deadline {
            match self.receive() {
                Ok(Some(message)) => return Ok(message),
                Ok(None) => {}
                Err(_) => return Err((HassFailure::Other, "connection closed".into())),
            }
        }
        Err((
            HassFailure::NotHomeAssistant,
            "no answer to the sign in".into(),
        ))
    }
}
