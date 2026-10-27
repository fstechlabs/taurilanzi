//! obs-websocket 5: its JSON messages, and how a client proves it knows the password.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const RPC_VERSION: u32 = 1;
/// Every category except the ones sent many times a second.
const EVENT_SUBSCRIPTIONS: u32 = (1 << 11) - 1;

pub mod op {
    pub const HELLO: u8 = 0;
    pub const IDENTIFY: u8 = 1;
    pub const IDENTIFIED: u8 = 2;
    pub const EVENT: u8 = 5;
    pub const REQUEST: u8 = 6;
    pub const REQUEST_RESPONSE: u8 = 7;
}

/// Why OBS closed the connection, from its close codes.
pub mod close {
    pub const AUTHENTICATION_FAILED: u16 = 4009;
    pub const UNSUPPORTED_RPC_VERSION: u16 = 4010;
}

/// Request status codes the client treats as "already done".
pub mod status {
    pub const OUTPUT_RUNNING: u16 = 500;
    pub const OUTPUT_NOT_RUNNING: u16 = 501;
    pub const OUTPUT_PAUSED: u16 = 502;
    pub const OUTPUT_NOT_PAUSED: u16 = 503;
    pub const STUDIO_MODE_ACTIVE: u16 = 504;
    pub const STUDIO_MODE_NOT_ACTIVE: u16 = 505;
}

#[derive(Debug, Deserialize)]
pub struct Envelope {
    pub op: u8,
    #[serde(default)]
    pub d: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub rpc_version: u32,
    pub authentication: Option<Challenge>,
}

#[derive(Debug, Deserialize)]
pub struct Challenge {
    pub challenge: String,
    pub salt: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub event_type: String,
    #[serde(default)]
    pub event_data: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestResponse {
    pub request_id: String,
    pub request_status: RequestStatus,
    #[serde(default)]
    pub response_data: Value,
}

#[derive(Debug, Deserialize)]
pub struct RequestStatus {
    pub result: bool,
    pub code: u16,
    #[serde(default)]
    pub comment: Option<String>,
}

/// `base64(sha256(base64(sha256(password + salt)) + challenge))`.
pub fn authentication(password: &str, challenge: &Challenge) -> String {
    let secret = BASE64.encode(Sha256::digest(format!("{password}{}", challenge.salt)));
    BASE64.encode(Sha256::digest(format!("{secret}{}", challenge.challenge)))
}

pub fn identify(authentication: Option<String>) -> String {
    let mut d = json!({
        "rpcVersion": RPC_VERSION,
        "eventSubscriptions": EVENT_SUBSCRIPTIONS,
    });
    if let Some(auth) = authentication {
        d["authentication"] = Value::String(auth);
    }
    json!({ "op": op::IDENTIFY, "d": d }).to_string()
}

pub fn request(id: &str, request_type: &str, data: Value) -> String {
    let mut d = json!({ "requestType": request_type, "requestId": id });
    if !data.is_null() {
        d["requestData"] = data;
    }
    json!({ "op": op::REQUEST, "d": d }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authentication_matches_the_spec() {
        let challenge = Challenge {
            challenge: "+IxH4CnCiqpX1rM9scsNynZzbOe4KhDeYcTNS3PDaeY=".into(),
            salt: "lM1GncleQOaCu9lT1yeUZhFYnqhsLLP1G5lAGo3ixaI=".into(),
        };
        assert_eq!(
            authentication("supersecretpassword", &challenge),
            "1Ct943GAT+6YQUUX47Ia/ncufilbe6+oD6lY+5kaCu4="
        );
    }

    #[test]
    fn identifies_with_or_without_a_password() {
        let with: Value = serde_json::from_str(&identify(Some("abc".into()))).unwrap();
        assert_eq!(
            with,
            json!({ "op": 1, "d": {
                "rpcVersion": 1,
                "eventSubscriptions": 2047,
                "authentication": "abc",
            }})
        );
        let without: Value = serde_json::from_str(&identify(None)).unwrap();
        assert!(without["d"].get("authentication").is_none());
    }

    #[test]
    fn requests_omit_empty_data() {
        let bare: Value = serde_json::from_str(&request("7", "GetVersion", Value::Null)).unwrap();
        assert_eq!(
            bare,
            json!({ "op": 6, "d": { "requestType": "GetVersion", "requestId": "7" } })
        );
        let data: Value = serde_json::from_str(&request(
            "8",
            "SetCurrentProgramScene",
            json!({ "sceneName": "A" }),
        ))
        .unwrap();
        assert_eq!(data["d"]["requestData"], json!({ "sceneName": "A" }));
    }

    #[test]
    fn reads_obs_messages() {
        let envelope: Envelope = serde_json::from_str(
            r#"{"op":0,"d":{"obsWebSocketVersion":"5.5.0","rpcVersion":1,
                "authentication":{"challenge":"c","salt":"s"}}}"#,
        )
        .unwrap();
        assert_eq!(envelope.op, op::HELLO);
        let hello: Hello = serde_json::from_value(envelope.d).unwrap();
        assert_eq!(hello.rpc_version, 1);
        let challenge = hello.authentication.unwrap();
        assert_eq!(
            (challenge.challenge.as_str(), challenge.salt.as_str()),
            ("c", "s")
        );

        let open: Hello = serde_json::from_str(r#"{"rpcVersion":1}"#).unwrap();
        assert!(open.authentication.is_none());

        let response: RequestResponse = serde_json::from_str(
            r#"{"requestType":"StartRecord","requestId":"3",
                "requestStatus":{"result":false,"code":500,"comment":"already"}}"#,
        )
        .unwrap();
        assert_eq!(response.request_id, "3");
        assert!(!response.request_status.result);
        assert_eq!(response.request_status.code, status::OUTPUT_RUNNING);
        assert_eq!(response.request_status.comment.as_deref(), Some("already"));
        assert!(response.response_data.is_null());

        let event: Event =
            serde_json::from_str(r#"{"eventType":"ExitStarted","eventIntent":64}"#).unwrap();
        assert_eq!(event.event_type, "ExitStarted");
        assert!(event.event_data.is_null());

        let bare: Envelope = serde_json::from_str(r#"{"op":2}"#).unwrap();
        assert_eq!((bare.op, bare.d), (op::IDENTIFIED, Value::Null));
    }
}
