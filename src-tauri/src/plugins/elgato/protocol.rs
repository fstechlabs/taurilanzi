//! Elgato's Stream Deck protocol, which Mirabox's Stream Dock plugins speak too: `event`s about a
//! `context` (an instance of an action) on a `device`. The host keeps one shape inside, Ulanzi's
//! (see `protocol`); this turns what the engine sends into the events of this protocol, and the
//! commands of these plugins into Ulanzi's, so the hub serves both.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::plugins::manifest::{Manifest, this_platform};
use crate::plugins::registry::Plugin;
use crate::plugins::ulanzi::protocol::{device_of, size_of, text};
/// What plugins made for Stream Dock are told the host is: a Stream Dock recent enough for every
/// plugin of its store, whose SDKs take a version 5 or later for another app of Mirabox's.
pub const STREAM_DOCK_VERSION: &str = "3.10.191.0421";
/// What plugins made for the Stream Deck are told, unless they ask for a later one: the first
/// whose behaviour Elgato's current SDK requires, with resources.
const STREAM_DECK_VERSION: (u32, u32) = (7, 1);
/// The size of a key whose event does not say, as the D200 draws its keys.
const KEY_SIZE: (u32, u32) = (196, 196);
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

/// A dock as these plugins are told of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDock {
    pub id: String,
    pub name: String,
    pub columns: u8,
    pub rows: u8,
    /// The Stream Deck with the same keypad, by Elgato's device types.
    pub kind: u8,
}

/// What the host keeps of each instance a main service was shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub action: String,
    pub key: String,
    pub device: String,
    pub size: (u32, u32),
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

    pub fn known(&self, context: &str) -> bool {
        self.states.contains_key(context)
    }
}

/// `-info`, and the fourth argument of the pages: the host, the docks and the plugin.
pub fn info(plugin: &Plugin, language: &str, docks: &[PluginDock]) -> Value {
    let stream_dock = plugin.manifest.made_for_stream_dock();
    json!({
        "application": {
            "font": "",
            "language": plugin_language(&plugin.dir, language),
            "platform": this_platform(),
            "platformVersion": sysinfo::System::os_version().unwrap_or_default(),
            "version": host_version(&plugin.manifest),
        },
        "colors": {
            "buttonMouseOverBackgroundColor": "#464646FF",
            "buttonPressedBackgroundColor": "#303030FF",
            "buttonPressedBorderColor": "#646464FF",
            "buttonPressedTextColor": "#969696FF",
            "highlightColor": "#0078FFFF",
        },
        // Keys are drawn at 196 pixels: Stream Deck plugins pick their sharper images by this.
        "devicePixelRatio": if stream_dock { 1 } else { 2 },
        "devices": docks.iter().map(|dock| json!({
            "id": dock.id,
            "name": dock.name,
            "size": { "columns": dock.columns, "rows": dock.rows },
            "type": dock.kind,
        })).collect::<Vec<_>>(),
        "plugin": { "uuid": plugin.manifest.uuid, "version": plugin.manifest.version },
    })
}

/// The command line of a main service, after its program and script, in the order every SDK
/// reads it.
pub fn arguments(port: u16, plugin: &Plugin, language: &str, docks: &[PluginDock]) -> Vec<String> {
    vec![
        "-port".into(),
        port.to_string(),
        "-pluginUUID".into(),
        plugin.manifest.uuid.clone(),
        "-registerEvent".into(),
        REGISTER_PLUGIN.into(),
        "-info".into(),
        info(plugin, language, docks).to_string(),
    ]
}

/// The version of the host a plugin is told, by the app it was made for.
pub fn host_version(manifest: &Manifest) -> String {
    if manifest.made_for_stream_dock() {
        return STREAM_DOCK_VERSION.to_owned();
    }
    let mut asked = manifest.software.minimum_version.trim().split('.');
    let mut part = || asked.next().and_then(|p| p.parse::<u32>().ok());
    let wanted = (part().unwrap_or_default(), part().unwrap_or_default());
    let (major, minor) = wanted.max(STREAM_DECK_VERSION);
    format!("{major}.{minor}.0.0")
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

/// Told to a main service as it registers, for each dock connected, and as one connects.
pub fn device_connected(dock: &PluginDock) -> Value {
    json!({
        "event": "deviceDidConnect",
        "device": dock.id,
        "deviceInfo": {
            "name": dock.name,
            "type": dock.kind,
            "size": { "columns": dock.columns, "rows": dock.rows },
        },
    })
}

pub fn device_disconnected(id: &str) -> Value {
    json!({ "event": "deviceDidDisconnect", "device": id })
}

fn coordinates(key: &str) -> Value {
    let (column, row) = key.split_once('_').unwrap_or_default();
    json!({
        "column": column.parse::<u32>().unwrap_or_default(),
        "row": row.parse::<u32>().unwrap_or_default(),
    })
}

fn about(event: &str, action: &str, context: &str, device: &str, payload: Value) -> Value {
    let mut message = json!({
        "event": event,
        "action": action,
        "context": context,
        "device": device,
    });
    if !payload.is_null() {
        message["payload"] = payload;
    }
    message
}

/// What every event of a key carries, with the size it is drawn at.
fn key_payload(
    settings: Value,
    resources: Value,
    key: &str,
    (width, height): (u32, u32),
    state: u32,
) -> Value {
    json!({
        "settings": settings,
        "coordinates": coordinates(key),
        "controller": "Keypad",
        "state": state,
        "isInMultiAction": false,
        "resources": resources,
        "width": width,
        "height": height,
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
    let shown = contexts.shown.get(&context);
    let device = device_of(message)
        .or_else(|| shown.map(|s| s.device.clone()))
        .unwrap_or_default();
    let size = size_of(message)
        .or_else(|| shown.map(|s| s.size))
        .unwrap_or(KEY_SIZE);
    let device = device.as_str();
    let settings = || {
        message
            .get("settings")
            .or_else(|| message.get("param"))
            .filter(|s| s.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}))
    };
    let resources = || {
        message
            .get("resources")
            .filter(|r| r.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}))
    };
    // A request of the plugin is answered with its id, which tells the answer from a change.
    let answer = |mut event: Value| {
        if let Some(id) = message.get("id").filter(|id| !id.is_null()) {
            event["id"] = id.clone();
        }
        event
    };
    match (text(message, "cmd").as_str(), side) {
        ("add", Side::Main) => {
            contexts.shown.insert(
                context.clone(),
                Shown {
                    action: action.clone(),
                    key: key.clone(),
                    device: device.to_owned(),
                    size,
                },
            );
            let state = contexts.state(&context);
            vec![about(
                "willAppear",
                &action,
                &context,
                device,
                key_payload(settings(), resources(), &key, size, state),
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
                device,
                key_payload(settings(), resources(), &key, size, state),
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
                        device,
                        key_payload(settings(), resources(), &key, size, state),
                    )
                })
                .collect()
        }
        ("paramfromapp", Side::Main) => vec![about(
            "propertyInspectorDidAppear",
            &action,
            &context,
            device,
            Value::Null,
        )],
        // Stream Dock's inspectors wait for their settings and never ask for them.
        ("paramfromapp" | "didReceiveSettings", _) => {
            let state = contexts.state(&context);
            vec![answer(about(
                "didReceiveSettings",
                &action,
                &context,
                device,
                key_payload(settings(), resources(), &key, size, state),
            ))]
        }
        ("didReceiveResources", _) => {
            let state = contexts.state(&context);
            vec![answer(about(
                "didReceiveResources",
                &action,
                &context,
                device,
                key_payload(settings(), resources(), &key, size, state),
            ))]
        }
        ("didReceiveGlobalSettings", _) => vec![answer(json!({
            "event": "didReceiveGlobalSettings",
            "payload": { "settings": settings() },
        }))],
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
        &gone.device,
        key_payload(settings, json!({}), &gone.key, gone.size, state),
    )]
}

/// Told to the main service when an inspector of one of its instances closes.
pub fn inspector_closed(action: &str, context: &str, device: &str) -> Value {
    about(
        "propertyInspectorDidDisappear",
        action,
        context,
        device,
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
        "getSettings" => {
            json!({ "cmd": "getSettings", "actionid": context, "id": message.get("id") })
        }
        "setGlobalSettings" => json!({ "cmd": "setGlobalSettings", "settings": payload }),
        "getGlobalSettings" => json!({ "cmd": "getGlobalSettings", "id": message.get("id") }),
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

    use super::*;
    use crate::plugins::manifest::{Protocol, Runtime};
    use crate::plugins::ulanzi::protocol::{Instance, KeyDock, event, key_fields, with_settings};

    const ACTION: &str = "com.a.b.act";
    const DEVICE_ID: &str = "dock-1";

    fn instance(key: &str) -> Instance {
        Instance {
            action: ACTION.into(),
            key: key.into(),
            actionid: "ctx".into(),
            device: DEVICE_ID.into(),
        }
    }

    /// The fields of a key of a D200, the wide one if `wide`.
    fn fields(settings: &Value, wide: bool) -> Value {
        let dock = KeyDock {
            model: "D200".into(),
            width: if wide { 452 } else { 196 },
            height: 196,
        };
        key_fields(settings, &dock)
    }

    fn docks() -> Vec<PluginDock> {
        vec![PluginDock {
            id: DEVICE_ID.into(),
            name: "Ulanzi D200".into(),
            columns: 5,
            rows: 3,
            kind: 0,
        }]
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
        let info = info(&plugin, "es-ES", &docks());
        assert_eq!(info["application"]["language"], "en", "none shipped");
        assert_eq!(info["application"]["version"], STREAM_DOCK_VERSION);
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

        let args = arguments(3906, &plugin, "en", &docks());
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
        info(plugin, language, &docks())["application"]["language"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn keys_appear_are_pressed_and_disappear() {
        let mut contexts = Contexts::default();
        let settings = json!({ "n": 1 });
        let add = event("add", &instance("2_1"), fields(&settings, false));
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
                    "resources": {},
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
            &event("keydown", &instance("2_1"), fields(&settings, false)),
            Side::Main,
            &mut contexts,
        );
        assert_eq!(down[0]["event"], "keyDown");
        assert_eq!(down[0]["payload"]["state"], 1);
        assert_eq!(down[0]["payload"]["settings"], settings);
        let up = outgoing(
            &event("keyup", &instance("2_1"), fields(&settings, false)),
            Side::Main,
            &mut contexts,
        );
        assert_eq!(up[0]["event"], "keyUp");
        assert!(
            outgoing(
                &event("run", &instance("2_1"), fields(&settings, false)),
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
            &event("add", &instance("3_2"), fields(&settings, true)),
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
        let run = event("run", &instance("0_0"), fields(&json!({}), false));
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
            fields(&json!({ "a": 1 }), false),
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
            command(json!({ "event": "setGlobalSettings", "context": "p", "payload": { "k": 1 } })),
            json!({ "cmd": "setGlobalSettings", "settings": { "k": 1 } })
        );
        assert_eq!(
            command(json!({ "event": "openUrl", "payload": { "url": "https://a.b" } })),
            json!({ "cmd": "openurl", "url": "https://a.b" })
        );
        assert_eq!(
            command(json!({ "event": "getSettings", "context": "c", "id": "r1" })),
            json!({ "cmd": "getSettings", "actionid": "c", "id": "r1" })
        );
        assert_eq!(
            command(json!({ "event": "getGlobalSettings", "context": "p", "id": "r2" })),
            json!({ "cmd": "getGlobalSettings", "id": "r2" })
        );
        assert!(incoming(&json!({ "event": "setFeedback", "context": "c" })).is_none());
    }

    #[test]
    fn each_plugin_is_told_the_host_it_was_made_for() {
        let made = |manifest: Value| {
            let manifest: Manifest = serde_json::from_value(manifest).unwrap();
            (manifest.made_for_stream_dock(), host_version(&manifest))
        };
        let dock = (true, STREAM_DOCK_VERSION.to_owned());
        assert_eq!(made(json!({ "APIVersion": "1.0", "SDKVersion": 2 })), dock);
        assert_eq!(
            made(json!({ "SDKVersion": 1, "Software": { "MinimumVersion": "2.9" } })),
            dock
        );
        assert_eq!(made(json!({ "SDKVersion": 1 })), dock);
        assert_eq!(
            made(json!({ "SDKVersion": 2, "Software": { "MinimumVersion": "5.0" } })),
            (false, "7.1.0.0".to_owned())
        );
        assert_eq!(
            made(json!({ "SDKVersion": 3, "Software": { "MinimumVersion": "7.4" } })),
            (false, "7.4.0.0".to_owned()),
            "a later one when the plugin asks for it"
        );
        assert_eq!(
            made(json!({ "SDKVersion": 2 })),
            (false, "7.1.0.0".to_owned())
        );

        let dir = tempfile::tempdir().unwrap();
        let mut elgato = plugin(dir.path());
        elgato.manifest.sdk_version = json!(2);
        assert_eq!(info(&elgato, "en", &docks())["devicePixelRatio"], 2);
        assert_eq!(
            info(&plugin(dir.path()), "en", &docks())["devicePixelRatio"],
            1
        );
    }
}
