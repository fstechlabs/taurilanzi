//! Elgato's Stream Deck protocol, which Mirabox's Stream Dock plugins speak too: `event`s about a
//! `context` (an instance of an action) on a `device`. The host keeps one shape inside, Ulanzi's
//! (see `protocol`); this turns what the engine sends into the events of this protocol, and the
//! commands of these plugins into Ulanzi's, so the hub serves both.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::protocol::text;
use super::registry::Plugin;
use crate::driver::d200::COLUMNS;

/// The dock as plugins see it. One fixed id: the app drives one D200.
pub const DEVICE_ID: &str = "TAURILANZI-D200";
const DEVICE_NAME: &str = "Ulanzi D200";
const ROWS: u8 = 3;
/// The type of a Stream Deck of five columns and three rows, which the D200 is laid out as.
const DEVICE_TYPE: u8 = 0;
/// What the plugins are told the host is: a Stream Dock recent enough for every plugin of its
/// store, whose SDKs take a version 5 or later for another app of Mirabox's.
pub const HOST_VERSION: &str = "3.10.191.0421";
/// Keys are drawn at this size; the wide key is as wide as two and their gap.
const KEY_SIZE: u32 = 196;
const WIDE_KEY_WIDTH: u32 = 452;
const WIDE_KEY: &str = "3_2";
/// A run this soon after a key went down on the dock is that same press, already told.
const SAME_PRESS: Duration = Duration::from_secs(2);
pub const REGISTER_PLUGIN: &str = "registerPlugin";
pub const REGISTER_INSPECTOR: &str = "registerPropertyInspector";

/// Which side of a plugin a connection is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Main,
    Inspector,
}

/// What the host keeps of each instance a main service was shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub action: String,
    pub key: String,
}

/// The instances of one plugin on the dock and the state of each, which these plugins are told
/// with every press.
#[derive(Debug, Default)]
pub struct Contexts {
    shown: HashMap<String, Shown>,
    states: HashMap<String, u32>,
    pressed: HashMap<String, Instant>,
}

impl Contexts {
    pub fn state(&self, context: &str) -> u32 {
        self.states.get(context).copied().unwrap_or_default()
    }

    pub fn set_state(&mut self, context: &str, state: u32) {
        self.states.insert(context.to_owned(), state);
    }

    /// The state an instance starts in, the first time it is seen.
    pub fn start(&mut self, context: &str, state: u32) {
        self.states.entry(context.to_owned()).or_insert(state);
    }

    pub fn shown(&self, context: &str) -> Option<&Shown> {
        self.shown.get(context)
    }
}

/// `-info`, and the fourth argument of the pages: the host, the dock and the plugin.
pub fn info(plugin: &Plugin, language: &str) -> Value {
    json!({
        "application": {
            "font": "",
            "language": plugin_language(&plugin.dir, language),
            "platform": platform(),
            "platformVersion": sysinfo::System::os_version().unwrap_or_default(),
            "version": HOST_VERSION,
        },
        "colors": {
            "buttonMouseOverBackgroundColor": "#464646FF",
            "buttonPressedBackgroundColor": "#303030FF",
            "buttonPressedBorderColor": "#646464FF",
            "buttonPressedTextColor": "#969696FF",
            "highlightColor": "#0078FFFF",
        },
        "devicePixelRatio": 1,
        "devices": [{
            "id": DEVICE_ID,
            "name": DEVICE_NAME,
            "size": { "columns": COLUMNS, "rows": ROWS },
            "type": DEVICE_TYPE,
        }],
        "plugin": { "uuid": plugin.manifest.uuid, "version": plugin.manifest.version },
    })
}

/// The command line of a main service, after its program and script, in the order every SDK
/// reads it.
pub fn arguments(port: u16, plugin: &Plugin, language: &str) -> Vec<String> {
    vec![
        "-port".into(),
        port.to_string(),
        "-pluginUUID".into(),
        plugin.manifest.uuid.clone(),
        "-registerEvent".into(),
        REGISTER_PLUGIN.into(),
        "-info".into(),
        info(plugin, language).to_string(),
    ]
}

fn platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    }
}

/// Plugins load `<language>.json` by the name they are told, and show `undefined` when it is
/// missing: a language they ship, English otherwise.
fn plugin_language(dir: &Path, language: &str) -> String {
    let full = language.replace('-', "_");
    let short = full.split('_').next().unwrap_or_default().to_owned();
    [full, short]
        .into_iter()
        .find(|name| !name.is_empty() && dir.join(format!("{name}.json")).is_file())
        .unwrap_or_else(|| "en".into())
}

/// Told to a main service as it registers.
pub fn device_connected() -> Value {
    json!({
        "event": "deviceDidConnect",
        "device": DEVICE_ID,
        "deviceInfo": {
            "name": DEVICE_NAME,
            "type": DEVICE_TYPE,
            "size": { "columns": COLUMNS, "rows": ROWS },
        },
    })
}

fn coordinates(key: &str) -> Value {
    let (column, row) = key.split_once('_').unwrap_or_default();
    json!({
        "column": column.parse::<u32>().unwrap_or_default(),
        "row": row.parse::<u32>().unwrap_or_default(),
    })
}

fn about(event: &str, action: &str, context: &str, payload: Value) -> Value {
    let mut message = json!({
        "event": event,
        "action": action,
        "context": context,
        "device": DEVICE_ID,
    });
    if !payload.is_null() {
        message["payload"] = payload;
    }
    message
}

/// What every event of a key carries, with the size it is drawn at.
fn key_payload(settings: Value, key: &str, state: u32) -> Value {
    let width = if key == WIDE_KEY {
        WIDE_KEY_WIDTH
    } else {
        KEY_SIZE
    };
    json!({
        "settings": settings,
        "coordinates": coordinates(key),
        "controller": "Keypad",
        "state": state,
        "isInMultiAction": false,
        "width": width,
        "height": KEY_SIZE,
    })
}

/// The events of this protocol for a message the engine sends in Ulanzi's shape to one side of
/// the plugin. Messages already in this protocol pass as they are; those with no counterpart are
/// dropped.
pub fn outgoing(message: &Value, side: Side, contexts: &mut Contexts) -> Vec<Value> {
    if message.get("event").is_some() {
        return vec![message.clone()];
    }
    if message.get("code").is_some() {
        return Vec::new();
    }
    let action = text(message, "uuid");
    let key = text(message, "key");
    let context = text(message, "actionid");
    let settings = || {
        message
            .get("settings")
            .or_else(|| message.get("param"))
            .filter(|s| s.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}))
    };
    match (text(message, "cmd").as_str(), side) {
        ("add", Side::Main) => {
            contexts.shown.insert(
                context.clone(),
                Shown {
                    action: action.clone(),
                    key: key.clone(),
                },
            );
            let state = contexts.state(&context);
            vec![about(
                "willAppear",
                &action,
                &context,
                key_payload(settings(), &key, state),
            )]
        }
        ("setactive", Side::Main) if message.get("active") == Some(&Value::Bool(false)) => {
            disappear(contexts, &context, settings())
        }
        ("clear", Side::Main) => message
            .get("param")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|gone| disappear(contexts, &text(gone, "actionid"), json!({})))
            .collect(),
        (cmd @ ("keydown" | "keyup"), Side::Main) => {
            let event = if cmd == "keydown" {
                contexts.pressed.insert(context.clone(), Instant::now());
                "keyDown"
            } else {
                "keyUp"
            };
            let state = contexts.state(&context);
            vec![about(
                event,
                &action,
                &context,
                key_payload(settings(), &key, state),
            )]
        }
        // These plugins only hear keys go down and up: a press that did not come from the dock,
        // or one the dock kept to tell gestures apart, is both.
        ("run", Side::Main) => {
            if contexts
                .pressed
                .get(&context)
                .is_some_and(|at| at.elapsed() < SAME_PRESS)
            {
                return Vec::new();
            }
            let state = contexts.state(&context);
            ["keyDown", "keyUp"]
                .into_iter()
                .map(|event| {
                    about(
                        event,
                        &action,
                        &context,
                        key_payload(settings(), &key, state),
                    )
                })
                .collect()
        }
        ("paramfromapp", Side::Main) => vec![about(
            "propertyInspectorDidAppear",
            &action,
            &context,
            Value::Null,
        )],
        // Stream Dock's inspectors wait for their settings and never ask for them.
        ("paramfromapp" | "didReceiveSettings", _) => {
            let state = contexts.state(&context);
            vec![about(
                "didReceiveSettings",
                &action,
                &context,
                key_payload(settings(), &key, state),
            )]
        }
        ("didReceiveGlobalSettings", _) => vec![json!({
            "event": "didReceiveGlobalSettings",
            "payload": { "settings": settings() },
        })],
        _ => Vec::new(),
    }
}

fn disappear(contexts: &mut Contexts, context: &str, settings: Value) -> Vec<Value> {
    let Some(gone) = contexts.shown.remove(context) else {
        return Vec::new();
    };
    let state = contexts.state(context);
    vec![about(
        "willDisappear",
        &gone.action,
        context,
        key_payload(settings, &gone.key, state),
    )]
}

/// Told to the main service when an inspector of one of its instances closes.
pub fn inspector_closed(action: &str, context: &str) -> Value {
    about(
        "propertyInspectorDidDisappear",
        action,
        context,
        Value::Null,
    )
}

/// A command of these plugins in Ulanzi's shape, for those both protocols have.
pub fn incoming(message: &Value) -> Option<Value> {
    let context = text(message, "context");
    let payload = message.get("payload").cloned().unwrap_or(Value::Null);
    let command = match text(message, "event").as_str() {
        "setSettings" => json!({
            "cmd": "setSettings",
            "actionid": context,
            "settings": payload,
        }),
        "getSettings" => json!({ "cmd": "getSettings", "actionid": context }),
        "setGlobalSettings" => json!({ "cmd": "setGlobalSettings", "settings": payload }),
        "getGlobalSettings" => json!({ "cmd": "getGlobalSettings" }),
        "setTitle" => json!({
            "cmd": "setTitle",
            "param": { "actionid": context, "text": text(&payload, "title") },
        }),
        "showAlert" => json!({ "cmd": "showAlert", "actionid": context }),
        "openUrl" => json!({ "cmd": "openurl", "url": text(&payload, "url") }),
        "logMessage" => json!({
            "cmd": "logMessage",
            "level": "info",
            "message": text(&payload, "message"),
        }),
        _ => return None,
    };
    Some(command)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::manifest::{Manifest, Protocol, Runtime};
    use super::super::protocol::{Instance, event, key_fields, with_settings};
    use super::*;

    const ACTION: &str = "com.a.b.act";

    fn instance(key: &str) -> Instance {
        Instance {
            action: ACTION.into(),
            key: key.into(),
            actionid: "ctx".into(),
        }
    }

    fn plugin(dir: &Path) -> Plugin {
        let manifest: Manifest =
            serde_json::from_value(json!({ "UUID": "com.a.b", "Version": "1.2" })).unwrap();
        Plugin {
            manifest,
            dir: dir.to_path_buf(),
            folder: "com.a.b.sdPlugin".into(),
            protocol: Protocol::StreamDeck,
            runtime: Runtime::None,
        }
    }

    #[test]
    fn tells_the_plugin_the_host_the_dock_and_a_language_it_has() {
        let dir = tempfile::tempdir().unwrap();
        let plugin = plugin(dir.path());
        let info = info(&plugin, "es-ES");
        assert_eq!(info["application"]["language"], "en", "none shipped");
        assert_eq!(info["application"]["version"], HOST_VERSION);
        assert_eq!(
            info["plugin"],
            json!({ "uuid": "com.a.b", "version": "1.2" })
        );
        assert_eq!(
            info["devices"][0]["size"],
            json!({ "columns": 5, "rows": 3 })
        );
        assert_eq!(info["devices"][0]["id"], DEVICE_ID);
        fs::write(dir.path().join("es.json"), "{}").unwrap();
        assert_eq!(info_language(&plugin, "es-ES"), "es");
        fs::write(dir.path().join("zh_CN.json"), "{}").unwrap();
        assert_eq!(info_language(&plugin, "zh-CN"), "zh_CN");

        let args = arguments(3906, &plugin, "en");
        assert_eq!(
            args[..7],
            [
                "-port",
                "3906",
                "-pluginUUID",
                "com.a.b",
                "-registerEvent",
                "registerPlugin",
                "-info"
            ]
        );
        let info: Value = serde_json::from_str(&args[7]).unwrap();
        assert_eq!(info["plugin"]["uuid"], "com.a.b");
    }

    fn info_language(plugin: &Plugin, language: &str) -> String {
        info(plugin, language)["application"]["language"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn keys_appear_are_pressed_and_disappear() {
        let mut contexts = Contexts::default();
        let settings = json!({ "n": 1 });
        let add = event("add", &instance("2_1"), key_fields(&settings));
        let appear = outgoing(&add, Side::Main, &mut contexts);
        assert_eq!(
            appear,
            vec![json!({
                "event": "willAppear",
                "action": ACTION,
                "context": "ctx",
                "device": DEVICE_ID,
                "payload": {
                    "settings": { "n": 1 },
                    "coordinates": { "column": 2, "row": 1 },
                    "controller": "Keypad",
                    "state": 0,
                    "isInMultiAction": false,
                    "width": 196,
                    "height": 196,
                },
            })]
        );
        assert!(outgoing(&add, Side::Inspector, &mut contexts).is_empty());
        let active = event("setactive", &instance("2_1"), json!({ "active": true }));
        assert!(outgoing(&active, Side::Main, &mut contexts).is_empty());

        contexts.set_state("ctx", 1);
        let down = outgoing(
            &event("keydown", &instance("2_1"), key_fields(&settings)),
            Side::Main,
            &mut contexts,
        );
        assert_eq!(down[0]["event"], "keyDown");
        assert_eq!(down[0]["payload"]["state"], 1);
        assert_eq!(down[0]["payload"]["settings"], settings);
        let up = outgoing(
            &event("keyup", &instance("2_1"), key_fields(&settings)),
            Side::Main,
            &mut contexts,
        );
        assert_eq!(up[0]["event"], "keyUp");
        assert!(
            outgoing(
                &event("run", &instance("2_1"), key_fields(&settings)),
                Side::Main,
                &mut contexts
            )
            .is_empty(),
            "the presses already went"
        );

        let hidden = event("setactive", &instance("2_1"), json!({ "active": false }));
        let gone = outgoing(&hidden, Side::Main, &mut contexts);
        assert_eq!(gone[0]["event"], "willDisappear");
        assert_eq!(
            gone[0]["payload"]["coordinates"],
            json!({ "column": 2, "row": 1 })
        );
        assert!(
            outgoing(&hidden, Side::Main, &mut contexts).is_empty(),
            "once"
        );

        outgoing(
            &event("add", &instance("3_2"), key_fields(&settings)),
            Side::Main,
            &mut contexts,
        );
        let clear = json!({
            "cmd": "clear",
            "uuid": "com.a.b",
            "param": [{ "uuid": ACTION, "key": "3_2", "actionid": "ctx" }, { "actionid": "other" }],
        });
        let cleared = outgoing(&clear, Side::Main, &mut contexts);
        assert_eq!(cleared.len(), 1);
        assert_eq!(cleared[0]["event"], "willDisappear");
        assert_eq!(cleared[0]["payload"]["width"], 452, "the wide key");
    }

    #[test]
    fn a_press_that_did_not_come_from_the_dock_goes_down_and_up() {
        let mut contexts = Contexts::default();
        let run = event("run", &instance("0_0"), key_fields(&json!({})));
        let events: Vec<Value> = outgoing(&run, Side::Main, &mut contexts)
            .into_iter()
            .map(|e| e["event"].clone())
            .collect();
        assert_eq!(events, [json!("keyDown"), json!("keyUp")]);
        assert!(outgoing(&run, Side::Inspector, &mut contexts).is_empty());
    }

    #[test]
    fn inspectors_get_their_settings_and_the_plugin_learns_they_opened() {
        let mut contexts = Contexts::default();
        let opened = event(
            "paramfromapp",
            &instance("0_0"),
            key_fields(&json!({ "a": 1 })),
        );
        let to_inspector = outgoing(&opened, Side::Inspector, &mut contexts);
        assert_eq!(to_inspector[0]["event"], "didReceiveSettings");
        assert_eq!(to_inspector[0]["payload"]["settings"], json!({ "a": 1 }));
        let to_main = outgoing(&opened, Side::Main, &mut contexts);
        assert_eq!(
            to_main,
            vec![json!({
                "event": "propertyInspectorDidAppear",
                "action": ACTION,
                "context": "ctx",
                "device": DEVICE_ID,
            })]
        );

        let saved = with_settings(
            &event("didReceiveSettings", &instance("0_0"), json!({})),
            "didReceiveSettings",
            json!({ "b": 2 }),
        );
        assert_eq!(
            outgoing(&saved, Side::Main, &mut contexts)[0]["payload"]["settings"],
            json!({ "b": 2 })
        );
        let global = with_settings(
            &json!({ "cmd": "getGlobalSettings" }),
            "didReceiveGlobalSettings",
            json!({ "token": "t" }),
        );
        assert_eq!(
            outgoing(&global, Side::Inspector, &mut contexts),
            vec![
                json!({ "event": "didReceiveGlobalSettings", "payload": { "settings": { "token": "t" } } })
            ]
        );
        assert!(
            outgoing(&json!({ "cmd": "x", "code": 0 }), Side::Main, &mut contexts).is_empty(),
            "no acks"
        );
        let raw = json!({ "event": "sendToPlugin", "payload": {} });
        assert_eq!(outgoing(&raw, Side::Main, &mut contexts), vec![raw]);
    }

    #[test]
    fn commands_become_the_hosts_own() {
        let command = |message: Value| incoming(&message).unwrap();
        assert_eq!(
            command(json!({ "event": "setSettings", "context": "c", "payload": { "a": 1 } })),
            json!({ "cmd": "setSettings", "actionid": "c", "settings": { "a": 1 } })
        );
        assert_eq!(
            command(
                json!({ "event": "setTitle", "context": "c", "payload": { "title": "Hi", "target": 0 } })
            ),
            json!({ "cmd": "setTitle", "param": { "actionid": "c", "text": "Hi" } })
        );
        assert_eq!(
            command(json!({ "event": "setGlobalSettings", "context": "p", "payload": { "k": 1 } })),
            json!({ "cmd": "setGlobalSettings", "settings": { "k": 1 } })
        );
        assert_eq!(
            command(json!({ "event": "openUrl", "payload": { "url": "https://a.b" } })),
            json!({ "cmd": "openurl", "url": "https://a.b" })
        );
        assert_eq!(
            command(json!({ "event": "getSettings", "context": "c" }))["cmd"],
            "getSettings"
        );
        assert!(incoming(&json!({ "event": "setFeedback", "context": "c" })).is_none());
    }
}
