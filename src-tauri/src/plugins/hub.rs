//! The host side of the protocol: who is connected, and what each message means. The main
//! service of a plugin and each open property inspector are separate connections; the hub routes
//! between them and hands what concerns keys and their settings to the engine.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::thread;

use log::{info, warn};
use serde_json::Value;

use super::desktop::Desktop;
use super::images;
use super::protocol::{self, Instance, ack, text};
use super::registry::{Registry, is_plugin_id};
use crate::render::ImageData;

pub type ConnId = u64;

/// What the engine has to do about a plugin.
#[derive(Debug, Clone)]
pub enum PluginInput {
    MainConnected {
        plugin: String,
    },
    InspectorConnected {
        conn: ConnId,
        instance: Instance,
    },
    Visual {
        actionid: String,
        change: VisualChange,
    },
    /// Save the settings of an instance and pass them on as `forward` to the other side.
    SaveSettings {
        actionid: String,
        settings: Value,
        from: ConnId,
        forward: &'static str,
    },
    GetSettings {
        actionid: String,
        conn: ConnId,
    },
    Alert {
        actionid: String,
    },
    /// Settings shared by every key of a plugin; they live in the configuration.
    SaveGlobalSettings {
        plugin: String,
        settings: Value,
    },
    /// Answer `message` on `conn` with the global settings of `plugin`.
    GetGlobalSettings {
        plugin: String,
        conn: ConnId,
        message: Value,
    },
}

#[derive(Debug, Clone)]
pub enum VisualChange {
    /// A state of the manifest, optionally with a text over it.
    State {
        state: u32,
        title: Option<String>,
    },
    /// An image for now, over the state's.
    Image {
        image: ImageData,
        title: Option<String>,
    },
    /// Images that replace the manifest's, by state.
    StateImages(Vec<(u32, ImageData)>),
    /// Back to the manifest's images.
    ClearImages,
    Title(Option<String>),
    SetState(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Peer {
    /// Connected, not yet said who it is.
    Unknown,
    Main {
        plugin: String,
    },
    Inspector {
        plugin: String,
        instance: Instance,
    },
}

struct Connection {
    peer: Peer,
    out: Sender<String>,
}

pub struct Hub {
    connections: HashMap<ConnId, Connection>,
    next: ConnId,
    registry: Arc<RwLock<Registry>>,
    notify: Box<dyn Fn(PluginInput) + Send>,
    desktop: Arc<dyn Desktop>,
    logs: PathBuf,
    port: u16,
}

/// Cheap to clone; how the server, the launcher and the engine reach the hub.
#[derive(Clone)]
pub struct PluginHost(Arc<Mutex<Hub>>);

pub struct HubSetup {
    pub registry: Arc<RwLock<Registry>>,
    pub notify: Box<dyn Fn(PluginInput) + Send>,
    pub desktop: Arc<dyn Desktop>,
    pub logs: PathBuf,
}

impl PluginHost {
    pub fn new(setup: HubSetup) -> Self {
        Self(Arc::new(Mutex::new(Hub {
            connections: HashMap::new(),
            next: 1,
            registry: setup.registry,
            notify: setup.notify,
            desktop: setup.desktop,
            logs: setup.logs,
            port: 0,
        })))
    }

    fn hub(&self) -> std::sync::MutexGuard<'_, Hub> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set_port(&self, port: u16) {
        self.hub().port = port;
    }

    pub fn port(&self) -> u16 {
        self.hub().port
    }

    pub fn registry(&self) -> Arc<RwLock<Registry>> {
        Arc::clone(&self.hub().registry)
    }

    pub(super) fn open(&self, out: Sender<String>) -> ConnId {
        let mut hub = self.hub();
        let id = hub.next;
        hub.next += 1;
        hub.connections.insert(
            id,
            Connection {
                peer: Peer::Unknown,
                out,
            },
        );
        id
    }

    pub(super) fn close(&self, conn: ConnId) {
        if let Some(connection) = self.hub().connections.remove(&conn)
            && let Peer::Main { plugin } = connection.peer
        {
            info!("plugin {plugin} disconnected");
        }
    }

    pub(super) fn receive(&self, conn: ConnId, text: &str) {
        let Ok(message) = serde_json::from_str::<Value>(text) else {
            warn!("plugin sent something that is not JSON");
            return;
        };
        // A file dialog waits for the user, so it runs apart and without holding the hub.
        if protocol::text(&message, "cmd") == "selectdialog" {
            let host = self.clone();
            thread::spawn(move || host.select(conn, &message));
        } else {
            self.handle(conn, message);
        }
    }

    fn select(&self, conn: ConnId, message: &Value) {
        let desktop = {
            let hub = self.hub();
            hub.send(conn, ack("selectdialog"));
            Arc::clone(&hub.desktop)
        };
        let kind = text(message, "type");
        let filter = text(message, "filter");
        if let Some(path) = desktop.select(&kind, &filter) {
            let mut answer = protocol::dialog_answer(&kind, &filter, &path);
            for field in ["uuid", "key", "actionid"] {
                answer[field] = message.get(field).cloned().unwrap_or_default();
            }
            self.hub().send(conn, answer);
        }
    }

    fn handle(&self, conn: ConnId, message: Value) {
        let cmd = text(&message, "cmd");
        let mut hub = self.hub();
        if cmd == "connected" {
            hub.connected(conn, &message);
            return;
        }
        // The main service echoes every event back with `code: 0`.
        if message.get("code").is_some() {
            return;
        }
        let Some(peer) = hub.connections.get(&conn).map(|c| c.peer.clone()) else {
            return;
        };
        let plugin = match &peer {
            Peer::Main { plugin } | Peer::Inspector { plugin, .. } => plugin.clone(),
            Peer::Unknown => hub.plugin_id(&text(&message, "uuid")),
        };
        hub.send(conn, ack(&cmd));
        hub.command(conn, &peer, &plugin, &cmd, &message);
    }

    /// The plugin an id belongs to: itself, or the installed plugin that owns the action.
    pub fn plugin_id(&self, uuid: &str) -> String {
        self.hub().plugin_id(uuid)
    }

    /// Sends to the main service of a plugin, if it is connected.
    pub fn to_main(&self, plugin: &str, message: &Value) -> bool {
        let hub = self.hub();
        let ids: Vec<ConnId> = hub
            .connections
            .iter()
            .filter(|(_, c)| matches!(&c.peer, Peer::Main { plugin: p } if p == plugin))
            .map(|(&id, _)| id)
            .collect();
        for id in &ids {
            hub.send(*id, message.clone());
        }
        !ids.is_empty()
    }

    /// To the main service and the inspectors of an instance, but not to the one that spoke.
    pub fn to_instance_except(&self, from: ConnId, plugin: &str, actionid: &str, message: &Value) {
        let hub = self.hub();
        for (&id, connection) in &hub.connections {
            let target = match &connection.peer {
                Peer::Main { plugin: p } => p == plugin,
                Peer::Inspector { instance, .. } => instance.actionid == actionid,
                Peer::Unknown => false,
            };
            if target && id != from {
                hub.send(id, message.clone());
            }
        }
    }

    pub fn to_conn(&self, conn: ConnId, message: &Value) {
        self.hub().send(conn, message.clone());
    }

    /// Main services connected now, so the UI can tell which plugins run.
    pub fn connected_plugins(&self) -> Vec<String> {
        self.hub()
            .connections
            .values()
            .filter_map(|c| match &c.peer {
                Peer::Main { plugin } => Some(plugin.clone()),
                _ => None,
            })
            .collect()
    }

    /// Closes the connections of a plugin, before it is stopped or removed.
    pub fn drop_plugin(&self, plugin: &str) {
        self.hub().connections.retain(|_, c| match &c.peer {
            Peer::Main { plugin: p } | Peer::Inspector { plugin: p, .. } => p != plugin,
            Peer::Unknown => true,
        });
    }
}

impl Hub {
    fn send(&self, conn: ConnId, message: Value) {
        if let Some(connection) = self.connections.get(&conn) {
            let _ = connection.out.send(message.to_string());
        }
    }

    fn to_inspectors_except(&self, actionid: &str, except: Option<ConnId>, message: &Value) {
        for (&id, connection) in &self.connections {
            if Some(id) != except
                && matches!(&connection.peer, Peer::Inspector { instance, .. } if instance.actionid == actionid)
            {
                self.send(id, message.clone());
            }
        }
    }

    fn to_plugin_except(&self, plugin: &str, except: ConnId, message: &Value) {
        for (&id, connection) in &self.connections {
            let mine = match &connection.peer {
                Peer::Main { plugin: p } | Peer::Inspector { plugin: p, .. } => p == plugin,
                Peer::Unknown => false,
            };
            if mine && id != except {
                self.send(id, message.clone());
            }
        }
    }

    fn plugin_id(&self, uuid: &str) -> String {
        self.registry
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .plugin_id(uuid)
    }

    /// The id of an installed plugin is its main service; an id under one is an inspector of an
    /// instance. Otherwise, as Ulanzi does: four segments for a main service, more for an inspector.
    fn connected(&mut self, conn: ConnId, message: &Value) {
        let uuid = text(message, "uuid");
        let main = {
            let registry = self.registry.read().unwrap_or_else(PoisonError::into_inner);
            match registry.owner(&uuid) {
                Some(plugin) => plugin.manifest.uuid == uuid,
                None => is_plugin_id(&uuid),
            }
        };
        let peer = if main {
            info!("plugin {uuid} connected");
            Peer::Main {
                plugin: uuid.clone(),
            }
        } else {
            Peer::Inspector {
                plugin: self.plugin_id(&uuid),
                instance: Instance {
                    action: uuid.clone(),
                    key: text(message, "key"),
                    actionid: text(message, "actionid"),
                },
            }
        };
        let Some(connection) = self.connections.get_mut(&conn) else {
            return;
        };
        connection.peer = peer.clone();
        match peer {
            Peer::Main { plugin } => (self.notify)(PluginInput::MainConnected { plugin }),
            Peer::Inspector { instance, .. } => {
                (self.notify)(PluginInput::InspectorConnected { conn, instance });
            }
            Peer::Unknown => {}
        }
    }

    fn command(&mut self, conn: ConnId, peer: &Peer, plugin: &str, cmd: &str, message: &Value) {
        let actionid = text(message, "actionid");
        match cmd {
            "state" => {
                let list = message
                    .pointer("/param/statelist")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for entry in list {
                    self.state_entry(plugin, &entry);
                }
            }
            "setState" => {
                let param = message.get("param").unwrap_or(message);
                if let Some(state) = param.get("state").and_then(Value::as_u64) {
                    self.visual(
                        &text(param, "actionid"),
                        VisualChange::SetState(state as u32),
                    );
                }
            }
            "setTitle" => {
                let param = message.get("param").unwrap_or(message);
                let title = text(param, "text");
                self.visual(
                    &text(param, "actionid"),
                    VisualChange::Title(Some(title).filter(|t| !t.is_empty())),
                );
            }
            "setImage" => {
                let param = message.get("param").unwrap_or(message);
                self.set_image(plugin, param);
            }
            "setSettings" | "paramfromplugin" => {
                let field = if cmd == "setSettings" {
                    "settings"
                } else {
                    "param"
                };
                let settings =
                    protocol::settings_object(message.get(field).unwrap_or(&Value::Null));
                let forward = if cmd == "setSettings" {
                    "didReceiveSettings"
                } else {
                    "paramfromplugin"
                };
                (self.notify)(PluginInput::SaveSettings {
                    actionid,
                    settings,
                    from: conn,
                    forward,
                });
            }
            "getSettings" => (self.notify)(PluginInput::GetSettings { actionid, conn }),
            "setGlobalSettings" => {
                let settings =
                    protocol::settings_object(message.get("settings").unwrap_or(&Value::Null));
                let answer =
                    protocol::with_settings(message, "didReceiveGlobalSettings", settings.clone());
                self.to_plugin_except(plugin, conn, &answer);
                (self.notify)(PluginInput::SaveGlobalSettings {
                    plugin: plugin.to_owned(),
                    settings,
                });
            }
            "getGlobalSettings" => (self.notify)(PluginInput::GetGlobalSettings {
                plugin: plugin.to_owned(),
                conn,
                message: message.clone(),
            }),
            "sendToPlugin" => {
                let mut forwarded = message.clone();
                if forwarded.get("uuid").is_none_or(|u| u.as_str() == Some("")) {
                    forwarded["uuid"] = plugin.into();
                }
                for (&id, connection) in &self.connections {
                    if matches!(&connection.peer, Peer::Main { plugin: p } if p == plugin) {
                        self.send(id, forwarded.clone());
                    }
                }
            }
            "sendToPropertyInspector" => self.to_inspectors_except(&actionid, None, message),
            "showAlert" => (self.notify)(PluginInput::Alert { actionid }),
            "toast" => self.desktop.toast(plugin, &text(message, "msg")),
            "logMessage" => self.log(plugin, &text(message, "level"), &text(message, "message")),
            "openurl" => self.open_url(plugin, message),
            "openview" => {
                if let Some(url) =
                    self.plugin_url(plugin, &text(message, "url"), message.get("param"))
                {
                    let size = |field: &str, default: f64| {
                        message
                            .get(field)
                            .and_then(Value::as_f64)
                            .unwrap_or(default)
                    };
                    self.desktop
                        .open_view(&url, size("width", 200.0), size("height", 200.0));
                }
            }
            "hotkey" => {
                let keys = text(message, "keylist");
                match super::hotkey::parse(&keys) {
                    Some(combo) => {
                        if let Err(err) = crate::actions::press_hotkey(&combo) {
                            warn!("plugin {plugin} hotkey {keys}: {err}");
                        }
                    }
                    None => warn!("plugin {plugin} sent a hotkey it cannot read: {keys}"),
                }
            }
            _ => {
                if matches!(peer, Peer::Unknown) {
                    warn!("a client sent {cmd} before saying who it is");
                }
            }
        }
    }

    fn visual(&self, actionid: &str, change: VisualChange) {
        if !actionid.is_empty() {
            (self.notify)(PluginInput::Visual {
                actionid: actionid.to_owned(),
                change,
            });
        }
    }

    /// One entry of a `state` command: type 0 a state of the manifest, 1 a base64 image, 2 an
    /// image file, 3 and 4 the same for GIFs.
    fn state_entry(&self, plugin: &str, entry: &Value) {
        let actionid = text(entry, "actionid");
        let title = (entry
            .get("showtext")
            .and_then(Value::as_bool)
            .unwrap_or(false))
        .then(|| text(entry, "textData"))
        .filter(|t| !t.is_empty());
        let kind = entry.get("type").and_then(Value::as_u64).unwrap_or(0);
        let image = match kind {
            1 => images::from_base64(&text(entry, "data")),
            2 => self.file_image(plugin, &text(entry, "path")),
            3 => images::from_base64(&text(entry, "gifdata")),
            4 => self.file_image(plugin, &text(entry, "gifpath")),
            _ => None,
        };
        let change = match (kind, image) {
            (0, _) => VisualChange::State {
                state: entry.get("state").and_then(Value::as_u64).unwrap_or(0) as u32,
                title,
            },
            (_, Some(image)) => VisualChange::Image { image, title },
            (_, None) => {
                warn!("plugin {plugin} sent an image that could not be read");
                return;
            }
        };
        self.visual(&actionid, change);
    }

    fn set_image(&self, plugin: &str, param: &Value) {
        let actionid = text(param, "actionid");
        if param.get("clear").and_then(Value::as_bool) == Some(true) {
            self.visual(&actionid, VisualChange::ClearImages);
            return;
        }
        let icons = param
            .get("icons")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let read = |icon: &Value| match text(icon, "source").as_str() {
            "base64" => images::from_base64(&text(icon, "base64")),
            _ => self.file_image(plugin, &text(icon, "path")),
        };
        if param.get("isDefault").and_then(Value::as_bool) == Some(true) {
            let images = icons
                .iter()
                .filter_map(|icon| {
                    let state = icon.get("state").and_then(Value::as_u64).unwrap_or(0) as u32;
                    read(icon).map(|image| (state, image))
                })
                .collect();
            self.visual(&actionid, VisualChange::StateImages(images));
        } else if let Some(image) = icons.first().and_then(read) {
            self.visual(&actionid, VisualChange::Image { image, title: None });
        }
    }

    /// Absolute paths are files the plugin wrote; others are inside its folder.
    fn file_image(&self, plugin: &str, path: &str) -> Option<ImageData> {
        let path = path.trim();
        let file =
            if std::path::Path::new(path).is_absolute() && std::path::Path::new(path).is_file() {
                PathBuf::from(path)
            } else {
                let registry = self.registry.read().unwrap_or_else(PoisonError::into_inner);
                registry.plugin(plugin)?.file(path)?
            };
        images::from_file(&file)
    }

    /// A page of the plugin, served by the host, with the query the plugin asked for.
    fn plugin_url(&self, plugin: &str, path: &str, param: Option<&Value>) -> Option<String> {
        let registry = self.registry.read().unwrap_or_else(PoisonError::into_inner);
        let found = registry.plugin(plugin)?;
        found.file(path)?;
        let relative = path.trim().trim_start_matches("./").trim_start_matches('/');
        let mut url = format!(
            "http://127.0.0.1:{}/plugins/{}/{}",
            self.port,
            urlencode(&found.folder),
            relative
                .split('/')
                .map(urlencode)
                .collect::<Vec<_>>()
                .join("/")
        );
        if let Some(query) = param.and_then(query_string) {
            url.push('?');
            url.push_str(&query);
        }
        Some(url)
    }

    fn open_url(&self, plugin: &str, message: &Value) {
        let url = text(message, "url");
        let local = message
            .get("local")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || !(url.starts_with("http://") || url.starts_with("https://"));
        let full = if local {
            self.plugin_url(plugin, &url, message.get("param"))
        } else {
            let query = message.get("param").and_then(query_string);
            Some(match query {
                Some(q) if url.contains('?') => format!("{url}&{q}"),
                Some(q) => format!("{url}?{q}"),
                None => url,
            })
        };
        match full {
            Some(url) => {
                if let Err(err) = crate::actions::open_web(&url) {
                    warn!("plugin {plugin} could not open {url}: {err}");
                }
            }
            None => warn!("plugin {plugin} asked to open a page it does not have"),
        }
    }

    /// One file per plugin, named after its full id.
    fn log(&self, plugin: &str, level: &str, message: &str) {
        let letter = match level {
            "debug" => 'D',
            "warn" => 'W',
            "error" => 'E',
            _ => 'I',
        };
        if fs::create_dir_all(&self.logs).is_err() {
            return;
        }
        let line = format!(
            "[{}] [{plugin}][{letter}] {message}\n",
            jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S%.3f")
        );
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.logs.join(format!("{plugin}.log")))
        {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

fn query_string(param: &Value) -> Option<String> {
    let object = param.as_object()?;
    let pairs: Vec<String> = object
        .iter()
        .map(|(k, v)| {
            let value = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            format!("{}={}", urlencode(k), urlencode(&value))
        })
        .collect();
    (!pairs.is_empty()).then(|| pairs.join("&"))
}

pub fn urlencode(text: &str) -> String {
    percent_encoding::utf8_percent_encode(text, percent_encoding::NON_ALPHANUMERIC)
        .to_string()
        .replace("%2E", ".")
        .replace("%2D", "-")
        .replace("%5F", "_")
        .replace("%7E", "~")
}
