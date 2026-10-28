//! Keys of plugins: telling each plugin which of its keys exist and are on the dock, passing on
//! their presses, drawing what the plugin sends and keeping the settings it saves.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use log::warn;
use serde_json::{Value, json};
use uuid::Uuid;

use super::commands::KeySlot;
use super::runtime::Engine;
use super::snapshot::EngineEvent;
use crate::config::{Action, Key};
use crate::plugins::{
    Instance, PluginHost, PluginInput, Registry, VisualChange, event, image_file, key_fields,
    key_name, settings_object, with_settings,
};
use crate::render::{Color, DEFAULT_FAMILY, Fit, ImageData, Layer, Outline, VerticalPosition};

const TITLE_SIZE: f32 = 26.0;

/// What a plugin made a key look like. Runtime state, like the path.
#[derive(Debug, Clone, Default)]
struct Visual {
    state: Option<u32>,
    /// For now, over the state's image.
    image: Option<ImageData>,
    /// Replacing the manifest's, by state.
    state_images: HashMap<u32, ImageData>,
    title: Option<String>,
}

pub struct PluginKeys {
    host: Option<PluginHost>,
    registry: Arc<RwLock<Registry>>,
    /// Every placement of a plugin action in the configuration, by its id.
    known: HashMap<String, Instance>,
    /// Those on the page the dock shows, which their plugins know are active.
    visible: HashMap<String, Instance>,
    visuals: HashMap<String, Visual>,
    /// Settings a plugin saved for a key the editor has not saved yet: its inspector opens as the
    /// action is added, before the key reaches the engine.
    pending: HashMap<String, Value>,
    /// Images of the manifests, read once.
    images: Mutex<HashMap<std::path::PathBuf, ImageData>>,
}

impl PluginKeys {
    pub fn new(host: Option<PluginHost>, registry: Arc<RwLock<Registry>>) -> Self {
        Self {
            host,
            registry,
            known: HashMap::new(),
            visible: HashMap::new(),
            visuals: HashMap::new(),
            pending: HashMap::new(),
            images: Mutex::default(),
        }
    }

    pub fn host(&self) -> Option<&PluginHost> {
        self.host.as_ref()
    }
}

/// The plugin action of a key, which may only be in its press list.
pub fn plugin_action(key: &Key) -> Option<(&str, Uuid, &Value)> {
    key.actions.iter().find_map(|action| match action {
        Action::Plugin {
            action,
            id,
            settings,
        } => Some((action.as_str(), *id, settings)),
        _ => None,
    })
}

fn instance(action: &str, index: u8, id: Uuid) -> Instance {
    Instance {
        action: action.to_owned(),
        key: key_name(index),
        actionid: id.to_string(),
    }
}

impl Engine {
    /// Brings the plugins up to date with the keys: `clear` for placements that are gone, `add`
    /// and `setactive` for those now on the dock, `setactive(false)` for those that left it.
    pub(super) fn sync_plugins(&mut self) {
        if self.plugin_keys.host.is_none() {
            return;
        }
        let mut all = HashMap::new();
        for page in self.config.profiles.iter().flat_map(|p| &p.pages) {
            for (&index, key) in &page.keys {
                if let Some((action, id, _)) = plugin_action(key) {
                    all.insert(id.to_string(), instance(action, index, id));
                }
            }
        }
        let showing = !self.paused && self.screen.showing_keys();
        let mut visible = HashMap::new();
        let mut settings = HashMap::new();
        if showing && let Some(page) = self.current_page() {
            for (&index, key) in &page.keys {
                if let Some((action, id, values)) = plugin_action(key) {
                    visible.insert(id.to_string(), instance(action, index, id));
                    settings.insert(id.to_string(), settings_object(values));
                }
            }
        }

        let removed: Vec<Instance> = self
            .plugin_keys
            .known
            .iter()
            .filter(|(id, old)| all.get(*id) != Some(old))
            .map(|(_, old)| old.clone())
            .collect();
        let hidden: Vec<Instance> = self
            .plugin_keys
            .visible
            .iter()
            .filter(|(id, old)| visible.get(*id) != Some(old) && all.get(*id) == Some(old))
            .map(|(_, old)| old.clone())
            .collect();
        let shown: Vec<Instance> = visible
            .iter()
            .filter(|(id, now)| self.plugin_keys.visible.get(*id) != Some(now))
            .map(|(_, now)| now.clone())
            .collect();

        let host = self.plugin_keys.host.clone().expect("checked above");
        let mut cleared: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for gone in &removed {
            cleared
                .entry(host.plugin_id(&gone.action))
                .or_default()
                .push(json!({ "uuid": gone.action, "key": gone.key, "actionid": gone.actionid }));
            if !all.contains_key(&gone.actionid) {
                self.plugin_keys.visuals.remove(&gone.actionid);
            }
        }
        for (plugin, list) in cleared {
            host.to_main(
                &plugin,
                &json!({ "cmd": "clear", "uuid": plugin, "key": "", "actionid": "", "param": list }),
            );
        }
        for gone in &hidden {
            host.to_main(
                &host.plugin_id(&gone.action),
                &event("setactive", gone, json!({ "active": false })),
            );
        }
        for now in &shown {
            let values = settings.get(&now.actionid).cloned().unwrap_or_default();
            announce(&host, now, &values);
        }
        self.plugin_keys.known = all;
        self.plugin_keys.visible = visible;
    }

    /// The physical press and release of a key go to its plugin as they happen.
    pub(super) fn plugin_key(&self, index: u8, pressed: bool) {
        let Some(host) = &self.plugin_keys.host else {
            return;
        };
        let Some(key) = self.current_page().and_then(|p| p.keys.get(&index)) else {
            return;
        };
        let Some((action, id, settings)) = plugin_action(key) else {
            return;
        };
        let cmd = if pressed { "keydown" } else { "keyup" };
        host.to_main(
            &host.plugin_id(action),
            &event(
                cmd,
                &instance(action, index, id),
                key_fields(&settings_object(settings)),
            ),
        );
    }

    pub(super) fn on_plugin(&mut self, input: PluginInput) {
        let Some(host) = self.plugin_keys.host.clone() else {
            return;
        };
        match input {
            PluginInput::MainConnected { plugin } => {
                let mine: Vec<Instance> = self
                    .plugin_keys
                    .visible
                    .values()
                    .filter(|i| host.plugin_id(&i.action) == plugin)
                    .cloned()
                    .collect();
                for instance in mine {
                    let settings = self.plugin_settings(&instance.actionid).unwrap_or_default();
                    announce(&host, &instance, &settings);
                }
                self.changed = true;
            }
            PluginInput::InspectorConnected { conn, instance } => {
                let settings = self.plugin_settings(&instance.actionid).unwrap_or_default();
                let fields = key_fields(&settings);
                host.to_conn(conn, &event("add", &instance, fields.clone()));
                host.to_conn(conn, &event("paramfromapp", &instance, fields.clone()));
                host.to_main(
                    &host.plugin_id(&instance.action),
                    &event("paramfromapp", &instance, fields),
                );
            }
            PluginInput::GetSettings { actionid, conn } => {
                if let Some((instance, settings)) = self.plugin_instance(&actionid) {
                    host.to_conn(
                        conn,
                        &with_settings(
                            &event("didReceiveSettings", &instance, json!({})),
                            "didReceiveSettings",
                            settings,
                        ),
                    );
                }
            }
            PluginInput::SaveSettings {
                actionid,
                settings,
                from,
                forward,
            } => {
                let Some((instance, _)) = self.plugin_instance(&actionid) else {
                    self.plugin_keys.pending.insert(actionid, settings);
                    return;
                };
                self.store_plugin_settings(&actionid, settings.clone());
                host.to_instance_except(
                    from,
                    &host.plugin_id(&instance.action),
                    &actionid,
                    &with_settings(&event(forward, &instance, json!({})), forward, settings),
                );
            }
            PluginInput::Visual { actionid, change } => self.plugin_visual(&actionid, change),
            PluginInput::Alert { actionid } => {
                if let Some(slot) = self.plugin_slot(&actionid) {
                    self.on_key_result(slot, false);
                }
            }
            PluginInput::SaveGlobalSettings { plugin, settings } => {
                if self.config.plugin_settings.get(&plugin) != Some(&settings) {
                    self.config.plugin_settings.insert(plugin, settings);
                    if let Err(err) = self.save() {
                        warn!("could not save the settings of a plugin: {err}");
                    }
                }
            }
            PluginInput::GetGlobalSettings {
                plugin,
                conn,
                message,
            } => {
                let settings = settings_object(
                    self.config
                        .plugin_settings
                        .get(&plugin)
                        .unwrap_or(&Value::Null),
                );
                host.to_conn(
                    conn,
                    &with_settings(&message, "didReceiveGlobalSettings", settings),
                );
            }
        }
    }

    fn plugin_visual(&mut self, actionid: &str, change: VisualChange) {
        let visual = self
            .plugin_keys
            .visuals
            .entry(actionid.to_owned())
            .or_default();
        match change {
            VisualChange::State { state, title } => {
                visual.state = Some(state);
                visual.image = None;
                visual.title = title;
            }
            VisualChange::Image { image, title } => {
                visual.image = Some(image);
                visual.title = title;
            }
            VisualChange::StateImages(images) => {
                visual.state_images = images.into_iter().collect();
                visual.image = None;
            }
            VisualChange::ClearImages => {
                visual.state_images.clear();
                visual.image = None;
            }
            VisualChange::Title(title) => visual.title = title,
            VisualChange::SetState(state) => {
                visual.state = Some(state);
                visual.image = None;
            }
        }
        if let Some(slot) = self.plugin_slot(actionid) {
            self.redraw_plugin_key(slot);
        }
    }

    fn redraw_plugin_key(&mut self, slot: KeySlot) {
        self.emit(EngineEvent::KeyImage {
            page: slot.page,
            index: slot.index,
        });
        if self.paused
            || !self.screen.showing_keys()
            || self.current_page().map(|p| p.id) != Some(slot.page)
        {
            return;
        }
        let Some(design) = self.design_for(slot.index) else {
            return;
        };
        match self.draw(slot.index, &design) {
            Ok(png) => self.dock.set_keys(BTreeMap::from([(slot.index, png)])),
            Err(err) => warn!("key {} could not be drawn: {err}", slot.index),
        }
    }

    /// The image and title a plugin key shows now: what the plugin sent, or the image of its
    /// state in the manifest.
    pub(super) fn plugin_layers(&self, key: &Key) -> Option<(Option<Layer>, Option<Layer>)> {
        let (action, id, _) = plugin_action(key)?;
        let visual = self
            .plugin_keys
            .visuals
            .get(&id.to_string())
            .cloned()
            .unwrap_or_default();
        let registry = self
            .plugin_keys
            .registry
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let found = registry.action(action);
        let state = visual
            .state
            .unwrap_or_else(|| found.map_or(0, |(_, a)| a.state));
        let image = visual
            .image
            .clone()
            .or_else(|| visual.state_images.get(&state).cloned())
            .or_else(|| {
                let (plugin, manifest) = found?;
                let path = plugin.file(manifest.state_image(state)?)?;
                let mut cache = self
                    .plugin_keys
                    .images
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                if let Some(cached) = cache.get(&path) {
                    return Some(cached.clone());
                }
                let image = image_file(&path)?;
                cache.insert(path, image.clone());
                Some(image)
            });
        let image = image.map(|image| Layer::Image {
            image,
            fit: Fit::Contain,
            inset: 0.0,
            tint: None,
        });
        let title = visual.title.map(|content| Layer::Text {
            content,
            font_family: DEFAULT_FAMILY.into(),
            font_weight: 700,
            size: TITLE_SIZE,
            color: Color::WHITE,
            position: VerticalPosition::Bottom,
            outline: Some(Outline {
                color: Color::BLACK,
                width: 4.0,
            }),
        });
        Some((image, title))
    }

    /// Where a placement is, in the active profile.
    fn plugin_slot(&self, actionid: &str) -> Option<KeySlot> {
        let profile = self.config.active_profile()?;
        profile.pages.iter().find_map(|page| {
            page.keys.iter().find_map(|(&index, key)| {
                plugin_action(key)
                    .filter(|(_, id, _)| id.to_string() == actionid)
                    .map(|_| KeySlot {
                        page: page.id,
                        index,
                    })
            })
        })
    }

    fn plugin_instance(&self, actionid: &str) -> Option<(Instance, Value)> {
        self.config
            .profiles
            .iter()
            .flat_map(|p| &p.pages)
            .find_map(|page| {
                page.keys.iter().find_map(|(&index, key)| {
                    let (action, id, settings) = plugin_action(key)?;
                    (id.to_string() == actionid)
                        .then(|| (instance(action, index, id), settings_object(settings)))
                })
            })
    }

    fn plugin_settings(&self, actionid: &str) -> Option<Value> {
        self.plugin_instance(actionid).map(|(_, settings)| settings)
    }

    /// Settings belong to the plugin: saved as they come, outside undo.
    fn store_plugin_settings(&mut self, actionid: &str, settings: Value) {
        let mut found = false;
        for key in self
            .config
            .profiles
            .iter_mut()
            .flat_map(|p| &mut p.pages)
            .flat_map(|page| page.keys.values_mut())
        {
            for action in &mut key.actions {
                if let Action::Plugin {
                    id,
                    settings: stored,
                    ..
                } = action
                    && id.to_string() == actionid
                {
                    *stored = settings.clone();
                    found = true;
                }
            }
        }
        if found && let Err(err) = self.save() {
            warn!("could not save the settings of a plugin key: {err}");
        }
    }

    /// The settings of placements already saved win over those in an edit of the key, which
    /// may have been loaded before the plugin changed them.
    pub(super) fn keep_plugin_settings(&self, key: &mut Key) {
        let current: HashMap<Uuid, Value> = self
            .config
            .profiles
            .iter()
            .flat_map(|p| &p.pages)
            .flat_map(|page| page.keys.values())
            .flat_map(|key| &key.actions)
            .filter_map(|action| match action {
                Action::Plugin { id, settings, .. } => Some((*id, settings.clone())),
                _ => None,
            })
            .collect();
        for action in &mut key.actions {
            if let Action::Plugin { id, settings, .. } = action {
                if let Some(saved) = current.get(id) {
                    *settings = saved.clone();
                } else if let Some(early) = self.plugin_keys.pending.get(&id.to_string()) {
                    *settings = early.clone();
                }
            }
        }
    }

    /// The placements of the plugins in `pages` get ids of their own, as copies.
    pub(super) fn renew_plugin_ids<'a>(keys: impl Iterator<Item = &'a mut Key>) {
        for key in keys {
            for action in &mut key.actions {
                if let Action::Plugin { id, .. } = action {
                    *id = Uuid::new_v4();
                }
            }
        }
    }

    /// Placement ids already used, so a key arriving with one of them is a copy.
    pub(super) fn used_plugin_ids(&self) -> HashSet<Uuid> {
        self.config
            .profiles
            .iter()
            .flat_map(|p| &p.pages)
            .flat_map(|page| page.keys.values())
            .flat_map(|key| &key.actions)
            .filter_map(|action| match action {
                Action::Plugin { id, .. } => Some(*id),
                _ => None,
            })
            .collect()
    }
}

/// What Ulanzi Studio sends when a key of a plugin shows up: `add` with its settings, then
/// `setactive`.
fn announce(host: &PluginHost, instance: &Instance, settings: &Value) {
    let plugin = host.plugin_id(&instance.action);
    host.to_main(&plugin, &event("add", instance, key_fields(settings)));
    host.to_main(
        &plugin,
        &event("setactive", instance, json!({ "active": true })),
    );
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::net::TcpStream;
    use std::path::Path;
    use std::sync::mpsc::{self, Receiver};
    use std::time::{Duration, Instant};

    use tempfile::TempDir;
    use tokio::sync::broadcast;
    use tungstenite::{Message, WebSocket};

    use super::*;
    use crate::actions::{Context, SoundPlayer};
    use crate::config::{Appearance, Config, DeviceSettings, Page, Position, Profile, Store};
    use crate::driver::d200::D200;
    use crate::engine::handle::Activity;
    use crate::engine::runtime::Links;
    use crate::plugins::{Desktop, Plugins, PluginsSetup};
    use crate::render::{Background, ImageFormat, Renderer};
    use crate::secrets::Secrets;

    const PLUGIN: &str = "com.taurilanzi.e2e.counter";
    const ACTION: &str = "com.taurilanzi.e2e.counter.count";
    const WAIT: Duration = Duration::from_secs(5);

    type Socket = WebSocket<TcpStream>;

    struct Quiet;

    impl Desktop for Quiet {
        fn toast(&self, _plugin: &str, _message: &str) {}
        fn open_view(&self, _url: &str, _width: f64, _height: f64) {}
        fn select(&self, _kind: &str, _filter: &str) -> Option<String> {
            None
        }
        fn run_page(&self, _plugin: &str, _url: &str) {}
        fn stop_page(&self, _plugin: &str) {}
    }

    struct Setup {
        engine: Engine,
        port: u16,
        inputs: Receiver<PluginInput>,
        home: Uuid,
        other: Uuid,
        _plugins: Plugins,
        _dir: TempDir,
    }

    fn fixture() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/e2e/fixtures/com.taurilanzi.e2e.ulanziPlugin")
    }

    /// The test plugin without its main service, so nothing is started.
    fn package(plugins: &Path) {
        let dir = plugins.join("com.taurilanzi.e2e.ulanziPlugin");
        fs::create_dir_all(dir.join("images")).unwrap();
        let mut manifest: Value =
            serde_json::from_slice(&fs::read(fixture().join("manifest.json")).unwrap()).unwrap();
        manifest.as_object_mut().unwrap().remove("CodePath");
        fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        for image in ["icon.svg", "on.svg", "off.svg"] {
            fs::copy(
                fixture().join("images").join(image),
                dir.join("images").join(image),
            )
            .unwrap();
        }
    }

    fn page(name: &str) -> Page {
        Page {
            id: Uuid::new_v4(),
            name: name.into(),
            keys: BTreeMap::new(),
            position: Position::default(),
        }
    }

    fn key(actions: Vec<Action>) -> Key {
        Key {
            appearance: Appearance {
                background: Background::Solid {
                    color: Color::BLACK,
                },
                layers: Vec::new(),
            },
            actions,
            double_press: Vec::new(),
            long_press: Vec::new(),
            second_state: None,
        }
    }

    fn plugin_key(id: Uuid, settings: Value) -> Key {
        key(vec![Action::Plugin {
            action: ACTION.into(),
            id,
            settings,
        }])
    }

    /// A profile with a key of the plugin on each of its two pages: 7 at home, 0 on the other.
    fn setup(home_key: Uuid, other_key: Uuid) -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        package(&data.join("plugins"));
        let (tx, inputs) = mpsc::channel();
        let plugins = Plugins::start(PluginsSetup {
            data_dir: data,
            resources: None,
            language: "es".into(),
            notify: Box::new(move |input| {
                let _ = tx.send(input);
            }),
            desktop: Arc::new(Quiet),
        })
        .unwrap();
        let host = plugins.host();
        let port = host.port();

        let mut home = page("Inicio");
        home.keys.insert(7, plugin_key(home_key, json!({ "n": 3 })));
        let mut other = page("Otra");
        other.keys.insert(0, plugin_key(other_key, Value::Null));
        let (home_id, other_id) = (home.id, other.id);
        let profile = Profile {
            id: Uuid::new_v4(),
            name: "Perfil".into(),
            home_page: home.id,
            pages: vec![home, other],
            apps: Vec::new(),
        };
        let store = Store::open(dir.path().join("store")).unwrap();
        let config = Config {
            schema: 2,
            revision: 0,
            device: DeviceSettings {
                brightness: 50,
                sleep_minutes: 0,
            },
            preferences: Default::default(),
            active_profile: profile.id,
            profiles: vec![profile],
            plugin_settings: Default::default(),
            integrations: Vec::new(),
            local_profiles: Default::default(),
            plugins: Default::default(),
            declined_plugins: Default::default(),
        };
        let (dock, _) = D200::detached();
        let (events, _) = broadcast::channel(16);
        let context = Context {
            assets: store.assets.clone(),
            sounds: SoundPlayer::start(),
            obs: Default::default(),
            hass: Default::default(),
            secrets: Secrets::new(dir.path(), "com.taurilanzi.test"),
        };
        let links = Links {
            events,
            activity: Activity::new(),
            inputs: mpsc::channel().0,
            plugins: PluginKeys::new(Some(host), plugins.registry()),
        };
        let engine = Engine::new(store, config, Renderer::new(), dock, links, context);
        Setup {
            engine,
            port,
            inputs,
            home: home_id,
            other: other_id,
            _plugins: plugins,
            _dir: dir,
        }
    }

    impl Setup {
        fn main(&self) -> Socket {
            let mut socket = connect(self.port);
            send(&mut socket, json!({ "cmd": "connected", "uuid": PLUGIN }));
            self.wait(|input| matches!(input, PluginInput::MainConnected { .. }));
            socket
        }

        fn inspector(&self, actionid: Uuid) -> (Socket, PluginInput) {
            let mut socket = connect(self.port);
            send(
                &mut socket,
                json!({ "cmd": "connected", "uuid": ACTION, "key": "2_1", "actionid": actionid.to_string() }),
            );
            let input = self.wait(|input| matches!(input, PluginInput::InspectorConnected { .. }));
            (socket, input)
        }

        fn wait(&self, wanted: impl Fn(&PluginInput) -> bool) -> PluginInput {
            let input = self.inputs.recv_timeout(WAIT).unwrap();
            assert!(wanted(&input), "{input:?}");
            input
        }

        fn settings(&self, page: Uuid, index: u8) -> Value {
            let key = &self.engine.config.profiles[0].page(page).unwrap().keys[&index];
            plugin_action(key).unwrap().2.clone()
        }
    }

    fn connect(port: u16) -> Socket {
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (socket, _) =
            tungstenite::client::client(format!("ws://127.0.0.1:{port}/"), stream).unwrap();
        socket
            .get_ref()
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        socket
    }

    fn send(socket: &mut Socket, message: Value) {
        socket.send(Message::text(message.to_string())).unwrap();
    }

    fn next(socket: &mut Socket) -> Value {
        let deadline = Instant::now() + WAIT;
        loop {
            match socket.read() {
                Ok(Message::Text(text)) => return serde_json::from_str(text.as_str()).unwrap(),
                Ok(_) => {}
                Err(tungstenite::Error::Io(err))
                    if matches!(
                        err.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    assert!(Instant::now() < deadline, "nothing arrived");
                }
                Err(err) => panic!("{err}"),
            }
        }
    }

    /// What reached the connection so far: everything the host sent before the ack of a probe.
    fn drain(socket: &mut Socket) -> Vec<Value> {
        let ack = json!({ "cmd": "probe", "cmdType": "NOTIFY", "code": 0, "param": null });
        send(socket, json!({ "cmd": "probe" }));
        let mut before = Vec::new();
        loop {
            let got = next(socket);
            if got == ack {
                return before;
            }
            before.push(got);
        }
    }

    fn at(index: u8, id: Uuid) -> Instance {
        instance(ACTION, index, id)
    }

    fn added(index: u8, id: Uuid, settings: Value) -> Vec<Value> {
        vec![
            event("add", &at(index, id), key_fields(&settings)),
            event("setactive", &at(index, id), json!({ "active": true })),
        ]
    }

    fn left(index: u8, id: Uuid) -> Value {
        event("setactive", &at(index, id), json!({ "active": false }))
    }

    #[test]
    fn plugins_learn_which_of_their_keys_are_on_the_dock() {
        let (x, z) = (Uuid::new_v4(), Uuid::new_v4());
        let mut setup = setup(x, z);
        let mut main = setup.main();

        setup.engine.sync_plugins();
        assert_eq!(drain(&mut main), added(7, x, json!({ "n": 3 })));
        setup.engine.sync_plugins();
        assert!(drain(&mut main).is_empty(), "only changes are sent");

        let other = setup.other;
        setup
            .engine
            .navigate(|nav, profile| nav.open(profile, other));
        setup.engine.sync_plugins();
        let mut expected = vec![left(7, x)];
        expected.extend(added(0, z, json!({})));
        assert_eq!(drain(&mut main), expected);

        setup.engine.config.profiles[0]
            .page_mut(other)
            .unwrap()
            .keys
            .remove(&0);
        setup.engine.sync_plugins();
        assert_eq!(
            drain(&mut main),
            vec![json!({
                "cmd": "clear",
                "uuid": PLUGIN,
                "key": "",
                "actionid": "",
                "param": [{ "uuid": ACTION, "key": "0_0", "actionid": z.to_string() }],
            })]
        );

        let home = setup.home;
        setup
            .engine
            .navigate(|nav, profile| nav.open(profile, home));
        setup.engine.sync_plugins();
        assert_eq!(drain(&mut main), added(7, x, json!({ "n": 3 })));
        setup.engine.paused = true;
        setup.engine.sync_plugins();
        assert_eq!(drain(&mut main), vec![left(7, x)]);
        setup.engine.paused = false;
        setup.engine.sync_plugins();
        assert_eq!(drain(&mut main), added(7, x, json!({ "n": 3 })));

        setup.engine.on_plugin(PluginInput::MainConnected {
            plugin: PLUGIN.into(),
        });
        assert_eq!(
            drain(&mut main),
            added(7, x, json!({ "n": 3 })),
            "a restarted plugin hears again"
        );
    }

    #[test]
    fn presses_and_runs_reach_the_plugin() {
        let (x, z) = (Uuid::new_v4(), Uuid::new_v4());
        let setup = setup(x, z);
        let mut main = setup.main();
        setup.engine.plugin_key(7, true);
        setup.engine.plugin_key(7, false);
        setup.engine.plugin_key(0, true);
        let fields = key_fields(&json!({ "n": 3 }));
        assert_eq!(
            drain(&mut main),
            vec![
                event("keydown", &at(7, x), fields.clone()),
                event("keyup", &at(7, x), fields),
            ]
        );

        let pressed = setup.engine.config.profiles[0]
            .page(setup.home)
            .unwrap()
            .keys[&7]
            .clone();
        let slot = KeySlot {
            page: setup.home,
            index: 7,
        };
        setup.engine.run_actions(pressed.actions, slot);
        assert_eq!(
            next(&mut main),
            event("run", &at(7, x), key_fields(&json!({ "n": 3 })))
        );
    }

    #[test]
    fn settings_a_plugin_saves_are_stored_and_passed_on() {
        let (x, z) = (Uuid::new_v4(), Uuid::new_v4());
        let mut setup = setup(x, z);
        let mut main = setup.main();
        let (mut inspector, connected) = setup.inspector(x);
        let PluginInput::InspectorConnected { conn, .. } = connected.clone() else {
            unreachable!();
        };

        setup.engine.on_plugin(connected);
        let fields = key_fields(&json!({ "n": 3 }));
        assert_eq!(
            drain(&mut inspector),
            vec![
                event("add", &at(7, x), fields.clone()),
                event("paramfromapp", &at(7, x), fields.clone()),
            ]
        );
        assert_eq!(
            drain(&mut main),
            vec![event("paramfromapp", &at(7, x), fields)]
        );

        setup.engine.on_plugin(PluginInput::SaveSettings {
            actionid: x.to_string(),
            settings: json!({ "n": 5 }),
            from: conn,
            forward: "didReceiveSettings",
        });
        assert_eq!(setup.settings(setup.home, 7), json!({ "n": 5 }));
        let saved = with_settings(
            &event("didReceiveSettings", &at(7, x), json!({})),
            "didReceiveSettings",
            json!({ "n": 5 }),
        );
        assert_eq!(drain(&mut main), vec![saved.clone()]);
        assert!(
            drain(&mut inspector).is_empty(),
            "not back to who saved them"
        );

        setup.engine.on_plugin(PluginInput::GetSettings {
            actionid: x.to_string(),
            conn,
        });
        assert_eq!(drain(&mut inspector), vec![saved]);

        let reloaded = setup.engine.store.load().unwrap().unwrap();
        let key = &reloaded.profiles[0].page(setup.home).unwrap().keys[&7];
        assert_eq!(plugin_action(key).unwrap().2, &json!({ "n": 5 }));
    }

    #[test]
    fn global_settings_live_in_the_configuration() {
        let (x, z) = (Uuid::new_v4(), Uuid::new_v4());
        let mut setup = setup(x, z);
        let mut main = setup.main();
        let (mut inspector, _) = setup.inspector(x);
        let get = json!({ "cmd": "getGlobalSettings", "uuid": PLUGIN });
        let ask = |setup: &mut Setup, main: &mut Socket| {
            send(main, get.clone());
            let input = setup.wait(|input| matches!(input, PluginInput::GetGlobalSettings { .. }));
            setup.engine.on_plugin(input);
            drain(main).pop()
        };

        assert_eq!(
            ask(&mut setup, &mut main),
            Some(with_settings(&get, "didReceiveGlobalSettings", json!({})))
        );

        let token = json!({ "token": "t" });
        send(
            &mut inspector,
            json!({ "cmd": "setGlobalSettings", "uuid": ACTION, "actionid": x.to_string(), "settings": token }),
        );
        let input = setup.wait(|input| matches!(input, PluginInput::SaveGlobalSettings { .. }));
        setup.engine.on_plugin(input);
        assert_eq!(setup.engine.config.plugin_settings[PLUGIN], token);
        let reloaded = setup.engine.store.load().unwrap().unwrap();
        assert_eq!(reloaded.plugin_settings[PLUGIN], token);

        assert_eq!(
            ask(&mut setup, &mut main),
            Some(with_settings(&get, "didReceiveGlobalSettings", token))
        );
    }

    #[test]
    fn an_edit_keeps_the_settings_the_plugin_saved() {
        let (x, z) = (Uuid::new_v4(), Uuid::new_v4());
        let mut setup = setup(x, z);
        let early = Uuid::new_v4();
        setup.engine.on_plugin(PluginInput::SaveSettings {
            actionid: early.to_string(),
            settings: json!({ "early": true }),
            from: 0,
            forward: "didReceiveSettings",
        });

        let mut edited = plugin_key(x, json!({}));
        setup.engine.keep_plugin_settings(&mut edited);
        assert_eq!(plugin_action(&edited).unwrap().2, &json!({ "n": 3 }));
        let mut added = plugin_key(early, json!({}));
        setup.engine.keep_plugin_settings(&mut added);
        assert_eq!(plugin_action(&added).unwrap().2, &json!({ "early": true }));
        let mut unknown = plugin_key(Uuid::new_v4(), json!({ "own": 1 }));
        setup.engine.keep_plugin_settings(&mut unknown);
        assert_eq!(plugin_action(&unknown).unwrap().2, &json!({ "own": 1 }));
    }

    #[test]
    fn copies_of_pages_and_profiles_get_their_own_placements() {
        let (x, z) = (Uuid::new_v4(), Uuid::new_v4());
        let mut setup = setup(x, z);
        let home = setup.home;
        let copy = setup.engine.duplicate_page(home).unwrap();
        let (action, id, settings) = {
            let key = &setup.engine.config.profiles[0].page(copy).unwrap().keys[&7];
            let (action, id, settings) = plugin_action(key).unwrap();
            (action.to_owned(), id, settings.clone())
        };
        assert_eq!(action, ACTION);
        assert_ne!(id, x);
        assert_eq!(settings, json!({ "n": 3 }));
        assert_eq!(
            plugin_action(&setup.engine.config.profiles[0].page(home).unwrap().keys[&7])
                .unwrap()
                .1,
            x
        );

        let original = setup.engine.config.profiles[0].id;
        let before = setup.engine.used_plugin_ids();
        assert_eq!(before, HashSet::from([x, z, id]));
        let copied = setup.engine.duplicate_profile(original).unwrap();
        let profile = setup
            .engine
            .config
            .profiles
            .iter()
            .find(|p| p.id == copied)
            .unwrap();
        let ids: HashSet<Uuid> = profile
            .pages
            .iter()
            .flat_map(|p| p.keys.values())
            .filter_map(|k| plugin_action(k).map(|(_, id, _)| id))
            .collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.is_disjoint(&before));
        assert_eq!(setup.engine.used_plugin_ids().len(), 6);
    }

    #[test]
    fn keys_show_what_the_plugin_draws_over_the_manifest() {
        let (x, z) = (Uuid::new_v4(), Uuid::new_v4());
        let mut setup = setup(x, z);
        let placed = setup.engine.config.profiles[0]
            .page(setup.home)
            .unwrap()
            .keys[&7]
            .clone();
        let image_of = |layers: Option<(Option<Layer>, Option<Layer>)>| match layers {
            Some((Some(Layer::Image { image, .. }), title)) => (image, title),
            other => panic!("{other:?}"),
        };
        let svg = |name: &str| fs::read(fixture().join("images").join(name)).unwrap();

        let (image, title) = image_of(setup.engine.plugin_layers(&placed));
        assert_eq!(*image.bytes, *svg("off.svg"));
        assert!(title.is_none());

        setup.engine.on_plugin(PluginInput::Visual {
            actionid: x.to_string(),
            change: VisualChange::State {
                state: 1,
                title: Some("Hi".into()),
            },
        });
        let (image, title) = image_of(setup.engine.plugin_layers(&placed));
        assert_eq!(*image.bytes, *svg("on.svg"));
        assert!(matches!(title, Some(Layer::Text { content, .. }) if content == "Hi"));

        let png = ImageData {
            format: ImageFormat::Png,
            bytes: Arc::from(&b"\x89PNG"[..]),
        };
        setup.engine.on_plugin(PluginInput::Visual {
            actionid: x.to_string(),
            change: VisualChange::Image {
                image: png.clone(),
                title: None,
            },
        });
        let (image, title) = image_of(setup.engine.plugin_layers(&placed));
        assert_eq!(image, png);
        assert!(title.is_none());

        setup.engine.on_plugin(PluginInput::Visual {
            actionid: x.to_string(),
            change: VisualChange::SetState(0),
        });
        let (image, _) = image_of(setup.engine.plugin_layers(&placed));
        assert_eq!(*image.bytes, *svg("off.svg"));
        assert!(
            setup
                .engine
                .plugin_layers(&key(vec![Action::Back]))
                .is_none()
        );
    }
}
