//! Ulanzi's plugin protocol: JSON objects with a `cmd`, addressed by the action `uuid`, the `key`
//! as `column_row` and the instance `actionid`. Events go out without `code`: the SDKs drop any
//! message that has one unless it is marked as a request.

use serde_json::{Map, Value, json};

use crate::driver::d200::COLUMNS;

/// Where an instance of an action is, as the plugin knows it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Instance {
    /// The action id, five segments or more.
    pub action: String,
    pub key: String,
    pub actionid: String,
}

/// Key 7 of the D200 is the third column of the second row: `2_1`.
pub fn key_name(index: u8) -> String {
    format!("{}_{}", index % COLUMNS, index / COLUMNS)
}

pub fn text(message: &Value, field: &str) -> String {
    match message.get(field) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// An event for one instance, as Ulanzi Studio sends it.
pub fn event(cmd: &str, instance: &Instance, extra: Value) -> Value {
    let mut message = Map::new();
    message.insert("cmd".into(), cmd.into());
    message.insert("uuid".into(), instance.action.clone().into());
    message.insert("key".into(), instance.key.clone().into());
    message.insert("actionid".into(), instance.actionid.clone().into());
    if let Value::Object(fields) = extra {
        message.extend(fields);
    }
    Value::Object(message)
}

/// The fields Ulanzi Studio adds to the events of a key.
pub fn key_fields(param: &Value) -> Value {
    json!({
        "param": param,
        "controller": "Keypad",
        "device": "D200",
        "isInMultiAction": false,
        "multiActionState": -1,
    })
}

/// What the host answers to every command; the SDKs ignore it, as it has a `code`.
pub fn ack(cmd: &str) -> Value {
    json!({ "cmd": cmd, "cmdType": "NOTIFY", "code": 0, "param": null })
}

/// The answer to a file or folder dialog: a request, so the SDKs do process it.
pub fn dialog_answer(kind: &str, filter: &str, path: &str) -> Value {
    json!({
        "cmd": "selectdialog",
        "cmdType": "REQUEST",
        "code": 0,
        "type": kind,
        "filter": filter,
        "path": path,
    })
}

/// Ulanzi Studio puts settings in `settings`, but plugins written against other versions read
/// `param` or `payload`: all three carry them.
pub fn with_settings(message: &Value, cmd: &str, settings: Value) -> Value {
    let mut answer = message.clone();
    answer["cmd"] = cmd.into();
    answer["param"] = settings.clone();
    answer["payload"] = settings.clone();
    answer["settings"] = settings;
    answer
}

/// Settings are always an object, even when a plugin never saved any.
pub fn settings_object(value: &Value) -> Value {
    if value.is_object() {
        value.clone()
    } else {
        Value::Object(Map::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance() -> Instance {
        Instance {
            action: "com.a.b.c.act".into(),
            key: "2_1".into(),
            actionid: "id-1".into(),
        }
    }

    #[test]
    fn keys_are_named_by_column_and_row() {
        assert_eq!(key_name(0), "0_0");
        assert_eq!(key_name(4), "4_0");
        assert_eq!(key_name(7), "2_1");
        assert_eq!(key_name(13), "3_2");
    }

    #[test]
    fn events_address_an_instance_without_a_code() {
        let message = event("keydown", &instance(), key_fields(&json!({ "n": 1 })));
        assert_eq!(
            message,
            json!({
                "cmd": "keydown",
                "uuid": "com.a.b.c.act",
                "key": "2_1",
                "actionid": "id-1",
                "param": { "n": 1 },
                "controller": "Keypad",
                "device": "D200",
                "isInMultiAction": false,
                "multiActionState": -1,
            })
        );
        assert!(message.get("code").is_none());
        assert_eq!(
            event("setactive", &instance(), Value::Null),
            json!({ "cmd": "setactive", "uuid": "com.a.b.c.act", "key": "2_1", "actionid": "id-1" })
        );
    }

    #[test]
    fn acks_are_notifications_and_dialog_answers_requests() {
        assert_eq!(
            ack("setTitle"),
            json!({ "cmd": "setTitle", "cmdType": "NOTIFY", "code": 0, "param": null })
        );
        let answer = dialog_answer("file", "png", "/tmp/a.png");
        assert_eq!(answer["cmdType"], "REQUEST");
        assert_eq!(answer["cmd"], "selectdialog");
        assert_eq!(answer["code"], 0);
        assert_eq!(answer["path"], "/tmp/a.png");
        assert_eq!(answer["type"], "file");
        assert_eq!(answer["filter"], "png");
    }

    #[test]
    fn settings_go_in_settings_param_and_payload() {
        let message = json!({ "cmd": "getSettings", "uuid": "com.a.b.c.act", "actionid": "id-1" });
        let answer = with_settings(&message, "didReceiveSettings", json!({ "a": 1 }));
        assert_eq!(answer["cmd"], "didReceiveSettings");
        assert_eq!(answer["uuid"], "com.a.b.c.act");
        assert_eq!(answer["actionid"], "id-1");
        for field in ["settings", "param", "payload"] {
            assert_eq!(answer[field], json!({ "a": 1 }), "{field}");
        }
    }

    #[test]
    fn settings_are_always_an_object() {
        assert_eq!(settings_object(&json!({ "a": 1 })), json!({ "a": 1 }));
        assert_eq!(settings_object(&Value::Null), json!({}));
        assert_eq!(settings_object(&json!([1, 2])), json!({}));
        assert_eq!(settings_object(&json!("text")), json!({}));
    }

    #[test]
    fn text_reads_strings_and_numbers() {
        let message = json!({ "a": "x", "b": 3, "c": true });
        assert_eq!(text(&message, "a"), "x");
        assert_eq!(text(&message, "b"), "3");
        assert_eq!(text(&message, "c"), "");
        assert_eq!(text(&message, "missing"), "");
    }
}
