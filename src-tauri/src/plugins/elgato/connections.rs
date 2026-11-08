//! The hub's side of the plugins that speak the Stream Deck protocol: how they register, the
//! commands only this protocol has, and what they hear of the dock, the computer and links.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::PoisonError;

use log::{info, warn};
use serde_json::Value;

use super::protocol as streamdeck;
use super::qt_svg;
use crate::plugins::hub::{ConnId, Hub, Peer, PluginHost, PluginInput, VisualChange};
use crate::plugins::images;
use crate::plugins::manifest::Protocol;
use crate::plugins::ulanzi::protocol::{self, Instance, text};

impl PluginHost {
    /// Sends to the main services that speak the Stream Deck protocol, of one plugin or of all.
    pub fn to_stream_deck(&self, plugin: Option<&str>, message: &Value) {
        let hub = self.hub();
        for (&id, connection) in &hub.connections {
            if connection.protocol == Protocol::StreamDeck
                && matches!(&connection.peer, Peer::Main { plugin: p } if plugin.is_none_or(|w| w == p))
            {
                hub.send(id, message.clone());
            }
        }
    }

    /// The dock came or went: Stream Deck plugins hear of it.
    pub fn set_dock(&self, connected: bool) {
        let mut hub = self.hub();
        if hub.dock == connected {
            return;
        }
        hub.dock = connected;
        let event = if connected {
            streamdeck::device_connected()
        } else {
            streamdeck::device_disconnected()
        };
        for (&id, connection) in &hub.connections {
            if connection.protocol == Protocol::StreamDeck
                && matches!(connection.peer, Peer::Main { .. })
            {
                hub.send(id, event.clone());
            }
        }
    }

    /// The apps running now: Stream Deck plugins that watch one hear it started or quit.
    pub fn apps_running(&self, now: HashSet<String>) {
        let mut hub = self.hub();
        let started: Vec<String> = now.difference(&hub.apps).cloned().collect();
        let quit: Vec<String> = hub.apps.difference(&now).cloned().collect();
        hub.apps = now;
        for (apps, event) in [
            (started, "applicationDidLaunch"),
            (quit, "applicationDidTerminate"),
        ] {
            for app in apps {
                hub.tell_app(None, &app, event);
            }
        }
    }

    /// A link the Stream Deck would open: `streamdeck://plugins/message/<plugin>/<rest>` reaches
    /// that plugin as `/<rest>`, now or once it connects.
    pub fn deep_link(&self, url: &str) {
        let Some(rest) = url
            .strip_prefix("streamdeck://plugins/message/")
            .filter(|r| !r.is_empty())
        else {
            warn!(
                "the app does not open links like {}",
                url.split('?').next().unwrap_or(url)
            );
            return;
        };
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (plugin, path) = rest.split_at(end);
        let path = if path.starts_with('/') {
            path.to_owned()
        } else {
            format!("/{path}")
        };
        let mut hub = self.hub();
        let ids = hub.main_ids(plugin);
        if ids.is_empty() {
            hub.links.entry(plugin.to_owned()).or_default().push(path);
            return;
        }
        for id in ids {
            hub.send(id, deep_link_event(&path));
        }
    }

    /// Every Stream Deck plugin hears the computer woke up.
    pub fn system_woke(&self) {
        let hub = self.hub();
        let event = serde_json::json!({ "event": "systemDidWakeUp" });
        for (&id, connection) in &hub.connections {
            if connection.protocol == Protocol::StreamDeck
                && matches!(connection.peer, Peer::Main { .. })
            {
                hub.send(id, event.clone());
            }
        }
    }

    /// The state an instance of a Stream Deck plugin is in.
    pub fn state_of(&self, actionid: &str) -> u32 {
        self.known_state(actionid).unwrap_or_default()
    }

    /// The state of an instance, if it has been in one other than its first.
    pub fn known_state(&self, actionid: &str) -> Option<u32> {
        let hub = self.hub();
        let contexts = hub.contexts.borrow();
        contexts
            .values()
            .find_map(|c| c.known(actionid).then(|| c.state(actionid)))
            .or_else(|| hub.saved_states.borrow().get(actionid).copied())
    }
}

impl Hub {
    fn main_ids(&self, plugin: &str) -> Vec<ConnId> {
        self.connections
            .iter()
            .filter(|(_, c)| {
                c.protocol == Protocol::StreamDeck
                    && matches!(&c.peer, Peer::Main { plugin: p } if p == plugin)
            })
            .map(|(&id, _)| id)
            .collect()
    }

    /// Tells the main services that watch an app, or one of them, that it started or quit, by
    /// the name their manifest gives it.
    fn tell_app(&self, only: Option<ConnId>, app: &str, event: &str) {
        let registry = self.registry.read().unwrap_or_else(PoisonError::into_inner);
        for (&id, connection) in &self.connections {
            let Peer::Main { plugin } = &connection.peer else {
                continue;
            };
            if connection.protocol != Protocol::StreamDeck || only.is_some_and(|o| o != id) {
                continue;
            }
            let Some(named) = registry.plugin(plugin).and_then(|p| {
                p.manifest
                    .applications_to_monitor
                    .here()
                    .iter()
                    .find(|name| name.eq_ignore_ascii_case(app))
                    .cloned()
            }) else {
                continue;
            };
            self.send(
                id,
                serde_json::json!({ "event": event, "payload": { "application": named } }),
            );
        }
    }

    pub(in crate::plugins) fn remember_state(&self, context: &str, state: u32) {
        let mut saved = self.saved_states.borrow_mut();
        if saved.insert(context.to_owned(), state) == Some(state) {
            return;
        }
        if let Ok(text) = serde_json::to_string(&*saved)
            && let Err(err) = fs::write(&self.states_file, text)
        {
            warn!("could not keep the states of plugin keys: {err}");
        }
    }

    /// How many states an action has and which one it starts in, and whether the host switches
    /// between them by itself.
    pub(in crate::plugins) fn states_of(&self, action: &str) -> ((u32, u32), bool) {
        let registry = self.registry.read().unwrap_or_else(PoisonError::into_inner);
        registry.action(action).map_or(((1, 0), false), |(_, a)| {
            (
                (a.states.len().max(1) as u32, a.state),
                !a.disable_automatic_states,
            )
        })
    }

    /// A message of a Stream Deck plugin: its registration, what only this protocol has, or a
    /// command both share, which goes on as Ulanzi's.
    pub(in crate::plugins) fn stream_deck(&mut self, conn: ConnId, message: &Value) {
        let event = text(message, "event");
        if event == streamdeck::REGISTER_PLUGIN || event == streamdeck::REGISTER_INSPECTOR {
            self.register(conn, &event, message);
            return;
        }
        let Some(peer) = self.connections.get(&conn).map(|c| c.peer.clone()) else {
            return;
        };
        let plugin = match &peer {
            Peer::Main { plugin } | Peer::Inspector { plugin, .. } => plugin.clone(),
            Peer::Unknown if event == "logMessage" => {
                let said = message.pointer("/payload/message").map(Value::to_string);
                info!(
                    "a plugin logged before registering: {}",
                    said.unwrap_or_default()
                );
                return;
            }
            Peer::Unknown => {
                warn!("a client sent {event} before saying who it is");
                return;
            }
        };
        let context = text(message, "context");
        let payload = message.get("payload").cloned().unwrap_or(Value::Null);
        let state = payload
            .get("state")
            .and_then(Value::as_u64)
            .map(|s| s as u32);
        match event.as_str() {
            "setImage" => self.stream_deck_image(&plugin, &context, &payload),
            // A title left out goes back to the manifest's; an empty one shows none.
            "setTitle" => self.visual(
                &context,
                VisualChange::Title {
                    state,
                    title: payload
                        .get("title")
                        .filter(|t| !t.is_null())
                        .map(|t| t.as_str().map_or_else(|| t.to_string(), str::to_owned)),
                },
            ),
            "setResources" => (self.notify)(PluginInput::SaveResources {
                actionid: context,
                resources: protocol::settings_object(&payload),
                from: conn,
            }),
            "getResources" => (self.notify)(PluginInput::GetResources {
                actionid: context,
                conn,
                id: message.get("id").cloned(),
            }),
            "setState" => {
                if let Some(state) = payload.get("state").and_then(Value::as_u64) {
                    let state = state as u32;
                    self.contexts
                        .borrow_mut()
                        .entry(plugin)
                        .or_default()
                        .set_state(&context, state);
                    self.remember_state(&context, state);
                    self.visual(&context, VisualChange::SetState(state));
                }
            }
            "showOk" => (self.notify)(PluginInput::Result {
                actionid: context,
                ok: true,
            }),
            "sendToPlugin" => self.to_mains(&plugin, message),
            "sendToPropertyInspector" => {
                let mut forwarded = message.clone();
                if text(&forwarded, "action").is_empty()
                    && let Some(shown) = self
                        .contexts
                        .borrow()
                        .get(&plugin)
                        .and_then(|c| c.shown(&context).cloned())
                {
                    forwarded["action"] = shown.action.into();
                }
                self.to_inspectors_except(&context, None, &forwarded);
            }
            _ => match streamdeck::incoming(message) {
                Some(command) => {
                    let cmd = text(&command, "cmd");
                    self.command(conn, &peer, &plugin, &cmd, &command);
                }
                // Dials, profiles and what only Mirabox's own devices have.
                None => log::debug!("plugin {plugin} sent {event}, which the app does not do"),
            },
        }
    }

    /// The plugin learns of the dock as it registers; an inspector is known by the instance its
    /// page was opened for, which the page adds to its registration.
    fn register(&mut self, conn: ConnId, event: &str, message: &Value) {
        let uuid = text(message, "uuid");
        let peer = if event == streamdeck::REGISTER_PLUGIN {
            let plugin = self.plugin_id(&uuid);
            info!("plugin {plugin} connected");
            Peer::Main { plugin }
        } else {
            let hint = message.get("taurilanzi").cloned().unwrap_or(Value::Null);
            let shown = self
                .contexts
                .borrow()
                .values()
                .find_map(|c| c.shown(&uuid).cloned());
            let action = Some(text(&hint, "action"))
                .filter(|a| !a.is_empty())
                .or_else(|| shown.as_ref().map(|s| s.action.clone()))
                .unwrap_or_default();
            let key = Some(text(&hint, "key"))
                .filter(|k| !k.is_empty())
                .or_else(|| shown.map(|s| s.key))
                .unwrap_or_default();
            Peer::Inspector {
                plugin: self.plugin_id(&action),
                instance: Instance {
                    action,
                    key,
                    actionid: uuid,
                },
            }
        };
        let Some(connection) = self.connections.get_mut(&conn) else {
            return;
        };
        connection.peer = peer.clone();
        connection.protocol = Protocol::StreamDeck;
        match peer {
            Peer::Main { plugin } => {
                if self.dock {
                    self.send(conn, streamdeck::device_connected());
                }
                for app in self.apps.clone() {
                    self.tell_app(Some(conn), &app, "applicationDidLaunch");
                }
                for path in self.links.remove(&plugin).unwrap_or_default() {
                    self.send(conn, deep_link_event(&path));
                }
                (self.notify)(PluginInput::MainConnected { plugin });
            }
            Peer::Inspector { instance, .. } => {
                let settings = message
                    .pointer("/taurilanzi/settings")
                    .filter(|s| s.is_object())
                    .cloned();
                (self.notify)(PluginInput::InspectorConnected {
                    conn,
                    instance,
                    settings,
                });
            }
            Peer::Unknown => {}
        }
    }

    /// An image for one state or for all; none goes back to the manifest's. The image is a data
    /// URI, an SVG as it is, or a file of the plugin, with or without its extension.
    fn stream_deck_image(&self, plugin: &str, context: &str, payload: &Value) {
        let image = text(payload, "image");
        let state = payload
            .get("state")
            .and_then(Value::as_u64)
            .map(|s| s as u32);
        let trimmed = image.trim();
        if trimmed.is_empty() {
            let change = state.map_or(VisualChange::ClearImages, VisualChange::ClearStateImage);
            self.visual(context, change);
            return;
        }
        let read = if trimmed.starts_with("data:") {
            images::from_base64(trimmed)
        } else if trimmed.starts_with('<') {
            images::from_bytes(trimmed.as_bytes().to_vec())
        } else {
            self.file_image(plugin, trimmed)
                .or_else(|| images::from_base64(trimmed))
        };
        let Some(image) = read.map(qt_svg::as_qt_draws) else {
            warn!("plugin {plugin} sent an image that could not be read");
            return;
        };
        let change = match state {
            Some(state) => VisualChange::StateImage { state, image },
            None => VisualChange::EveryState(image),
        };
        self.visual(context, change);
    }
}

fn deep_link_event(path: &str) -> Value {
    serde_json::json!({ "event": "didReceiveDeepLink", "payload": { "url": path } })
}

/// Next to the logs, in the data folder.
pub(in crate::plugins) fn states_file(logs: &std::path::Path) -> PathBuf {
    logs.parent().unwrap_or(logs).join("plugin-states.json")
}

pub(in crate::plugins) fn read_states(file: &std::path::Path) -> HashMap<String, u32> {
    fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}
