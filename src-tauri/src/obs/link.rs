//! One open connection to obs-websocket: identifying, requests and their answers, and events.

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use serde_json::Value;
use tungstenite::{Message, WebSocket};

use super::error::ObsError;
use super::protocol::{self, Envelope, Event, Hello, RequestResponse, close, op};
use super::status::ObsFailure;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a read waits, so the worker can serve requests while no message arrives.
const POLL: Duration = Duration::from_millis(50);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Versions {
    pub obs: String,
    pub websocket: String,
}

pub struct Link {
    socket: WebSocket<TcpStream>,
    next_id: u64,
    /// Events that arrived while waiting for the answer to a request.
    events: VecDeque<Event>,
    lost: bool,
}

pub type OpenError = (ObsFailure, String);

impl Link {
    /// Connects and identifies. The password is only asked for if OBS wants one.
    pub fn open(
        host: &str,
        port: u16,
        password: impl FnOnce() -> Option<String>,
    ) -> Result<(Self, Versions), OpenError> {
        let unreachable = |detail: String| (ObsFailure::Unreachable, detail);
        let stream = connect(host, port).map_err(unreachable)?;
        let timeouts = stream
            .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
            .and_then(|()| stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT)))
            .and_then(|()| stream.set_nodelay(true));
        timeouts.map_err(|err| unreachable(err.to_string()))?;
        let host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        let (socket, _) = tungstenite::client(format!("ws://{host}:{port}"), stream)
            .map_err(|err| unreachable(format!("not a WebSocket server: {err}")))?;
        let mut link = Self {
            socket,
            next_id: 0,
            events: VecDeque::new(),
            lost: false,
        };

        let other = |detail: String| (ObsFailure::Other, detail);
        let hello: Hello = link.handshake_message(op::HELLO).and_then(|d| {
            serde_json::from_value(d).map_err(|err| (ObsFailure::Other, err.to_string()))
        })?;
        if hello.rpc_version < protocol::RPC_VERSION {
            return Err((
                ObsFailure::Incompatible,
                format!("RPC version {}", hello.rpc_version),
            ));
        }
        let authentication = match &hello.authentication {
            Some(challenge) => match password() {
                Some(password) => Some(protocol::authentication(&password, challenge)),
                None => {
                    return Err((
                        ObsFailure::PasswordRequired,
                        "OBS asks for a password".into(),
                    ));
                }
            },
            None => None,
        };
        link.send(protocol::identify(authentication))
            .map_err(|err| other(err.to_string()))?;
        link.handshake_message(op::IDENTIFIED)?;

        link.socket
            .get_ref()
            .set_read_timeout(Some(POLL))
            .map_err(|err| unreachable(err.to_string()))?;
        let version = link
            .call("GetVersion", Value::Null)
            .map_err(|err| other(err.to_string()))?;
        let field = |name: &str| version[name].as_str().unwrap_or_default().to_string();
        let versions = Versions {
            obs: field("obsVersion"),
            websocket: field("obsWebSocketVersion"),
        };
        Ok((link, versions))
    }

    /// Sends a request and waits for its answer, keeping the events that arrive meanwhile.
    pub fn call(&mut self, request_type: &str, data: Value) -> Result<Value, ObsError> {
        self.next_id += 1;
        let id = self.next_id.to_string();
        self.send(protocol::request(&id, request_type, data))?;
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        while Instant::now() < deadline {
            let Some(envelope) = self.receive()? else {
                continue;
            };
            match envelope.op {
                op::EVENT => self.keep_event(envelope.d),
                op::REQUEST_RESPONSE => {
                    let response: RequestResponse = serde_json::from_value(envelope.d)
                        .map_err(|err| ObsError::Protocol(err.to_string()))?;
                    if response.request_id != id {
                        continue;
                    }
                    let status = response.request_status;
                    return if status.result {
                        Ok(response.response_data)
                    } else {
                        Err(ObsError::Rejected {
                            code: status.code,
                            message: status
                                .comment
                                .unwrap_or_else(|| format!("code {}", status.code)),
                        })
                    };
                }
                _ => {}
            }
        }
        Err(ObsError::Timeout)
    }

    /// The next event, waiting a moment for one.
    pub fn next_event(&mut self) -> Result<Option<Event>, ObsError> {
        if let Some(event) = self.events.pop_front() {
            return Ok(Some(event));
        }
        match self.receive()? {
            Some(envelope) if envelope.op == op::EVENT => {
                Ok(serde_json::from_value(envelope.d).ok())
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

    fn keep_event(&mut self, d: Value) {
        if let Ok(event) = serde_json::from_value(d) {
            self.events.push_back(event);
        }
    }

    fn send(&mut self, text: String) -> Result<(), ObsError> {
        self.socket.send(Message::text(text)).map_err(|_| {
            self.lost = true;
            ObsError::Lost
        })
    }

    /// One message, or `None` if none arrived within the poll interval.
    fn receive(&mut self) -> Result<Option<Envelope>, ObsError> {
        self.read().map_err(|_| ObsError::Lost)
    }

    /// Like `receive`, with OBS's close code when it closed the connection.
    fn read(&mut self) -> Result<Option<Envelope>, Option<u16>> {
        match self.socket.read() {
            Ok(Message::Text(text)) => Ok(serde_json::from_str(text.as_str()).ok()),
            Ok(Message::Close(frame)) => {
                self.lost = true;
                Err(frame.map(|frame| u16::from(frame.code)))
            }
            Ok(_) => Ok(None),
            Err(tungstenite::Error::Io(err))
                if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                Ok(None)
            }
            Err(_) => {
                self.lost = true;
                Err(None)
            }
        }
    }

    /// Waits for a message of the identification, telling apart why OBS refused it.
    fn handshake_message(&mut self, expected: u8) -> Result<Value, OpenError> {
        let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
        while Instant::now() < deadline {
            match self.read() {
                Ok(Some(envelope)) if envelope.op == expected => return Ok(envelope.d),
                Ok(_) => {}
                Err(Some(close::AUTHENTICATION_FAILED)) => {
                    return Err((ObsFailure::WrongPassword, "wrong password".into()));
                }
                Err(Some(close::UNSUPPORTED_RPC_VERSION)) => {
                    return Err((ObsFailure::Incompatible, "unsupported RPC version".into()));
                }
                Err(code) => {
                    let detail = code.map_or("connection closed".into(), |code| {
                        format!("connection closed with code {code}")
                    });
                    return Err((ObsFailure::Other, detail));
                }
            }
        }
        Err((ObsFailure::Other, "OBS did not answer in time".into()))
    }
}

fn connect(host: &str, port: u16) -> Result<TcpStream, String> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|err| format!("{host}: {err}"))?;
    let mut last = format!("{host} has no address");
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(err) => last = format!("{address}: {err}"),
        }
    }
    Err(last)
}
