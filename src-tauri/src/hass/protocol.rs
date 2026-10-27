//! Home Assistant's WebSocket API: its JSON messages, one per frame.

use serde::Deserialize;
use serde_json::{Value, json};

pub mod kind {
    pub const AUTH_REQUIRED: &str = "auth_required";
    pub const AUTH_OK: &str = "auth_ok";
    pub const AUTH_INVALID: &str = "auth_invalid";
    pub const RESULT: &str = "result";
    pub const EVENT: &str = "event";
    pub const PONG: &str = "pong";
}

/// Any message Home Assistant sends.
#[derive(Debug, Deserialize)]
pub struct Incoming {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default)]
    pub result: Value,
    #[serde(default)]
    pub error: Option<ErrorBody>,
    #[serde(default)]
    pub event: Value,
    #[serde(default)]
    pub ha_version: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ErrorBody {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
}

pub fn auth(token: &str) -> String {
    json!({ "type": "auth", "access_token": token }).to_string()
}

/// A command: its `fields` sit next to `id` and `type`.
pub fn command(id: u64, kind: &str, fields: Value) -> String {
    let mut message = match fields {
        Value::Object(map) => map,
        _ => Default::default(),
    };
    message.insert("id".into(), id.into());
    message.insert("type".into(), kind.into());
    Value::Object(message).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_carry_their_fields() {
        let message: Value = serde_json::from_str(&command(
            7,
            "call_service",
            json!({ "domain": "light", "service": "toggle" }),
        ))
        .unwrap();
        assert_eq!(
            message,
            json!({ "id": 7, "type": "call_service", "domain": "light", "service": "toggle" })
        );
        let bare: Value = serde_json::from_str(&command(8, "get_states", Value::Null)).unwrap();
        assert_eq!(bare, json!({ "id": 8, "type": "get_states" }));
        let auth: Value = serde_json::from_str(&auth("abc")).unwrap();
        assert_eq!(auth, json!({ "type": "auth", "access_token": "abc" }));
    }

    #[test]
    fn reads_results_errors_and_events() {
        let ok: Incoming =
            serde_json::from_str(r#"{"id":3,"type":"result","success":true,"result":null}"#)
                .unwrap();
        assert_eq!((ok.id, ok.success), (Some(3), Some(true)));
        let failed: Incoming = serde_json::from_str(
            r#"{"id":4,"type":"result","success":false,"error":{"code":"not_found","message":"Service light.nope not found."}}"#,
        )
        .unwrap();
        let error = failed.error.unwrap();
        assert_eq!(error.code, "not_found");
        assert_eq!(error.message, "Service light.nope not found.");
        let event: Incoming =
            serde_json::from_str(r#"{"id":5,"type":"event","event":{"r":["light.a"]}}"#).unwrap();
        assert_eq!(event.kind, kind::EVENT);
        assert_eq!(event.event, json!({ "r": ["light.a"] }));
        let hello: Incoming =
            serde_json::from_str(r#"{"type":"auth_required","ha_version":"2026.10.4"}"#).unwrap();
        assert_eq!(hello.ha_version.as_deref(), Some("2026.10.4"));
    }
}
