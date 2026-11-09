//! Keys of plugins: telling each plugin which of its keys exist and are on the dock, passing on
//! their presses, drawing what the plugin sends and keeping the settings it saves.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

use log::warn;
use serde_json::{Value, json};
use uuid::Uuid;

use super::commands::KeySlot;
use super::runtime::Engine;
use super::snapshot::EngineEvent;
use crate::config::{Action, Key, LayerConfig};
use crate::driver::d200::KEY_SIZE;
use crate::plugins::{
    Instance, PluginHost, PluginInput, Protocol, Registry, StateManifest, VisualChange,
    as_qt_draws, event, image_file, key_fields, key_name, settings_object, with_settings,
};
use crate::render::{Color, DEFAULT_FAMILY, Fit, ImageData, Layer, Outline, VerticalPosition};

const TITLE_SIZE: f32 = 26.0;
/// The Stream Deck sizes titles in points of a key of 72, 12 unless the manifest gives a number.
const ELGATO_KEY: f32 = 72.0;
const ELGATO_SIZE: f64 = 12.0;
/// The fonts the Stream Deck offers for titles. It names only these, and leaves the default one
/// empty: SDKs that read the name into a list of them refuse the whole message otherwise.
const TITLE_FONTS: [&str; 15] = [
    "Arial",
    "Arial Black",
    "Comic Sans MS",
    "Courier",
    "Courier New",
    "Georgia",
    "Impact",
    "Microsoft Sans Serif",
    "Symbol",
    "Tahoma",
    "Times New Roman",
    "Trebuchet MS",
    "Verdana",
    "Webdings",
    "Wingdings",
];
const TITLE_STYLES: [&str; 4] = ["Regular", "Bold", "Italic", "Bold Italic"];

fn told_as(known: &[&'static str], name: &str) -> &'static str {
    known
        .iter()
        .find(|k| k.eq_ignore_ascii_case(name.trim()))
        .copied()
        .unwrap_or("")
}
/// Stream Dock draws titles on keys of 112 pixels, in points two more than their `FontSize`: 10
/// when the manifest gives none, 12 when it gives one it cannot read. Points of Windows, where
/// its plugins are made: 4/3 of a pixel.
const STREAM_DOCK_KEY: f32 = 112.0;
const STREAM_DOCK_SIZE: f64 = 10.0;
const STREAM_DOCK_UNREADABLE_SIZE: f64 = 12.0;
const STREAM_DOCK_POINT: f32 = 4.0 / 3.0;
/// Settings plugins save as they go are written at most this often: some save several times a
/// second.
const SETTINGS_SAVE_DELAY: Duration = Duration::from_secs(2);

/// What a plugin made a key look like. Runtime state, like the path.
#[derive(Debug, Clone, Default)]
struct Visual {
    state: Option<u32>,
    /// For now, over the state's image.
    image: Option<ImageData>,
    /// Replacing the manifest's, by state.
    state_images: HashMap<u32, ImageData>,
    /// For every state; none is the manifest's.
    title: Option<String>,
    /// For one state, over `title`.
    state_titles: HashMap<u32, Option<String>>,
}

impl Visual {
    fn title_of(&self, state: u32) -> Option<String> {
        self.state_titles
            .get(&state)
            .cloned()
            .unwrap_or_else(|| self.title.clone())
    }
}

pub struct PluginKeys {
    host: Option<PluginHost>,
    registry: Arc<RwLock<Registry>>,
    /// Every placement of a plugin action in the configuration, by its id.
    known: HashMap<String, Instance>,
    /// Those on the page the dock shows, and on the one the editor shows, which their plugins
    /// know are active.
    visible: HashMap<String, Instance>,
    /// The page open in the editor: its keys draw for their previews, even off the dock.
    edited: Option<Uuid>,
    visuals: HashMap<String, Visual>,
    /// Settings a plugin saved for a key the editor has not saved yet: its inspector opens as the
    /// action is added, before the key reaches the engine.
    pending: HashMap<String, Value>,
    /// Images of the manifests, read once.
    images: Mutex<HashMap<std::path::PathBuf, ImageData>>,
    /// Since when settings a plugin saved wait to be written.
    unsaved: Option<Instant>,
    /// The title and its look each Stream Deck instance on view was last told, by instance.
    titles_told: HashMap<String, Value>,
}

impl PluginKeys {
    pub fn new(host: Option<PluginHost>, registry: Arc<RwLock<Registry>>) -> Self {
        Self {
            host,
            registry,
            known: HashMap::new(),
            visible: HashMap::new(),
            edited: None,
            visuals: HashMap::new(),
            pending: HashMap::new(),
            images: Mutex::default(),
            unsaved: None,
            titles_told: HashMap::new(),
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
            ..
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
        let on_dock = self.current_page().filter(|_| showing);
        let edited = self
            .plugin_keys
            .edited
            .and_then(|id| self.config.active_profile()?.page(id));
        for page in on_dock.into_iter().chain(edited) {
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
            announce(&host, now, &values, self.plugin_resources(&now.actionid));
        }
        self.plugin_keys.known = all;
        self.plugin_keys.visible = visible;
        self.tell_titles(&host);
    }

    /// Stream Deck plugins hear the title of each key on view and how it looks, as it appears and
    /// whenever the user changes it.
    fn tell_titles(&mut self, host: &PluginHost) {
        let mut told = HashMap::new();
        for instance in self.plugin_keys.visible.values() {
            let Some(event) = self.title_event(host, instance) else {
                continue;
            };
            if self.plugin_keys.titles_told.get(&instance.actionid) != Some(&event) {
                host.to_stream_deck(Some(&host.plugin_id(&instance.action)), &event);
            }
            told.insert(instance.actionid.clone(), event);
        }
        self.plugin_keys.titles_told = told;
    }

    /// `titleParametersDidChange` for an instance of a Stream Deck plugin: the user's text when
    /// the key has one, else the plugin's or the manifest's title, in the look it is drawn in.
    fn title_event(&self, host: &PluginHost, instance: &Instance) -> Option<Value> {
        let key = self
            .config
            .profiles
            .iter()
            .flat_map(|p| &p.pages)
            .find_map(|page| {
                page.keys.values().find(|key| {
                    plugin_action(key).is_some_and(|(_, id, _)| id.to_string() == instance.actionid)
                })
            })?;
        let (_, _, settings) = plugin_action(key)?;
        let kept = host.state_of(&instance.actionid);
        let registry = self
            .plugin_keys
            .registry
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let (plugin, action) = registry.action(&instance.action)?;
        if plugin.protocol != Protocol::StreamDeck {
            return None;
        }
        let visual = self.plugin_keys.visuals.get(&instance.actionid);
        let state = visual.and_then(|v| v.state).unwrap_or(kept);
        let look = action
            .states
            .get(state as usize)
            .or_else(|| action.states.first());
        let users = key.appearance.layers.iter().find_map(|layer| match layer {
            LayerConfig::Text {
                content,
                font_family,
                font_weight,
                size,
                color,
                position,
                ..
            } if !content.trim().is_empty() => {
                Some((content, font_family, *font_weight, *size, *color, *position))
            }
            _ => None,
        });
        let alignment = |position: VerticalPosition| match position {
            VerticalPosition::Top => "top",
            VerticalPosition::Middle => "middle",
            VerticalPosition::Bottom => "bottom",
        };
        let (title, parameters) = match users.filter(|_| action.user_title_enabled) {
            Some((content, family, weight, size, color, position)) => (
                content.clone(),
                json!({
                    "fontFamily": told_as(&TITLE_FONTS, family),
                    "fontSize": (size * ELGATO_KEY / KEY_SIZE.0 as f32).round() as u32,
                    "fontStyle": if weight >= 700 { "Bold" } else { "Regular" },
                    "fontUnderline": false,
                    "showTitle": true,
                    "titleAlignment": alignment(position),
                    "titleColor": color.hex(),
                }),
            ),
            None => {
                let drawn = stream_deck_title(
                    visual.and_then(|v| v.title_of(state)),
                    look,
                    plugin.manifest.made_for_stream_dock(),
                );
                let asked = look.map(|l| &l.font_size).and_then(|size| {
                    size.as_f64()
                        .or_else(|| size.as_str().and_then(|s| s.trim().parse().ok()))
                });
                let (content, family, color, position) = match drawn {
                    Some(Layer::Text {
                        content,
                        font_family,
                        color,
                        position,
                        ..
                    }) => (content, font_family, color, position),
                    _ => (
                        String::new(),
                        DEFAULT_FAMILY.to_owned(),
                        Color::WHITE,
                        VerticalPosition::Bottom,
                    ),
                };
                let style = look
                    .map(|l| told_as(&TITLE_STYLES, &l.font_style))
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Regular");
                (
                    content,
                    json!({
                        "fontFamily": told_as(
                            &TITLE_FONTS,
                            look.map(|l| l.font_family.trim())
                                .filter(|f| !f.is_empty())
                                .unwrap_or(family.as_str()),
                        ),
                        "fontSize": asked.unwrap_or(ELGATO_SIZE).round() as u32,
                        "fontStyle": style,
                        "fontUnderline": false,
                        "showTitle": look.and_then(|l| l.show_title).unwrap_or(true),
                        "titleAlignment": alignment(position),
                        "titleColor": color.hex(),
                    }),
                )
            }
        };
        let (column, row) = instance.key.split_once('_').unwrap_or_default();
        Some(json!({
            "event": "titleParametersDidChange",
            "action": instance.action,
            "context": instance.actionid,
            "device": crate::plugins::DEVICE_ID,
            "payload": {
                "controller": "Keypad",
                "coordinates": {
                    "column": column.parse::<u32>().unwrap_or_default(),
                    "row": row.parse::<u32>().unwrap_or_default(),
                },
                "resources": self.plugin_resources(&instance.actionid),
                "settings": settings_object(settings),
                "state": state,
                "title": title,
                "titleParameters": parameters,
            },
        }))
    }

    /// The editor shows another page, or none.
    pub(super) fn edit_page(&mut self, id: Option<Uuid>) {
        if self.plugin_keys.edited != id {
            self.plugin_keys.edited = id;
            self.sync_plugins();
        }
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
        // Stream Deck plugins act as their key goes down and up; when the key also tells gestures
        // apart, they hear the press once it is told, from its run.
        let gestures = !key.double_press.is_empty() || !key.long_press.is_empty();
        if gestures && host.stream_deck(action) {
            return;
        }
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
                    announce(
                        &host,
                        &instance,
                        &settings,
                        self.plugin_resources(&instance.actionid),
                    );
                }
                self.changed = true;
            }
            PluginInput::InspectorConnected {
                conn,
                instance,
                settings: opened_with,
            } => {
                let settings = self
                    .plugin_settings(&instance.actionid)
                    .or_else(|| self.plugin_keys.pending.get(&instance.actionid).cloned())
                    .or(opened_with)
                    .unwrap_or_default();
                let fields = key_fields(&settings);
                host.to_conn(conn, &event("add", &instance, fields.clone()));
                host.to_conn(conn, &event("paramfromapp", &instance, fields.clone()));
                host.to_main(
                    &host.plugin_id(&instance.action),
                    &event("paramfromapp", &instance, fields),
                );
            }
            PluginInput::GetSettings { actionid, conn, id } => {
                if let Some((instance, settings)) = self.plugin_instance(&actionid) {
                    let mut answer = with_settings(
                        &event("didReceiveSettings", &instance, json!({})),
                        "didReceiveSettings",
                        settings,
                    );
                    if host.stream_deck(&instance.action) {
                        answer["resources"] = self.plugin_resources(&actionid);
                    }
                    if let Some(id) = id.filter(|id| !id.is_null()) {
                        answer["id"] = id;
                    }
                    host.to_conn(conn, &answer);
                }
            }
            PluginInput::GetResources { actionid, conn, id } => {
                if let Some((instance, settings)) = self.plugin_instance(&actionid) {
                    let mut answer = with_settings(
                        &event("didReceiveResources", &instance, json!({})),
                        "didReceiveResources",
                        settings,
                    );
                    answer["resources"] = self.plugin_resources(&actionid);
                    if let Some(id) = id.filter(|id| !id.is_null()) {
                        answer["id"] = id;
                    }
                    host.to_conn(conn, &answer);
                }
            }
            PluginInput::SaveResources {
                actionid,
                resources,
                from,
            } => {
                let Some((instance, settings)) = self.plugin_instance(&actionid) else {
                    return;
                };
                self.store_plugin_resources(&actionid, resources.clone());
                let mut told = with_settings(
                    &event("didReceiveResources", &instance, json!({})),
                    "didReceiveResources",
                    settings,
                );
                told["resources"] = resources;
                host.to_instance_except(from, &host.plugin_id(&instance.action), &actionid, &told);
            }
            PluginInput::SaveSettings {
                actionid,
                settings,
                from,
                forward,
            } => {
                // Settings of no instance would reach whichever inspector comes next without one.
                if actionid.is_empty() {
                    return;
                }
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
            PluginInput::Result { actionid, ok } => {
                if let Some(slot) = self.plugin_slot(&actionid) {
                    self.on_key_result(slot, ok);
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
        let states = self
            .plugin_instance(actionid)
            .and_then(|(instance, _)| {
                let registry = self
                    .plugin_keys
                    .registry
                    .read()
                    .unwrap_or_else(PoisonError::into_inner);
                registry
                    .action(&instance.action)
                    .map(|(_, a)| a.states.len().max(1) as u32)
            })
            .unwrap_or(1);
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
            VisualChange::EveryState(image) => {
                visual.image = None;
                visual.state_images = (0..states).map(|state| (state, image.clone())).collect();
            }
            VisualChange::ClearStateImage(state) => {
                visual.state_images.remove(&state);
            }
            VisualChange::StateImages(images) => {
                visual.state_images = images.into_iter().collect();
                visual.image = None;
            }
            VisualChange::StateImage { state, image } => {
                if visual.state == Some(state) {
                    visual.image = None;
                }
                visual.state_images.insert(state, image);
            }
            VisualChange::ClearImages => {
                visual.state_images.clear();
                visual.image = None;
            }
            VisualChange::Title { state: None, title } => {
                visual.title = title;
                visual.state_titles.clear();
            }
            VisualChange::Title {
                state: Some(state),
                title,
            } => {
                visual.state_titles.insert(state, title);
            }
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

    /// Whether the key is of a plugin that speaks the Stream Deck protocol.
    pub(super) fn stream_deck_key(&self, key: &Key) -> bool {
        plugin_action(key).is_some_and(|(action, _, _)| {
            self.plugin_keys
                .registry
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .action(action)
                .is_some_and(|(plugin, _)| plugin.protocol == Protocol::StreamDeck)
        })
    }

    /// Whether the user may title the key of a Stream Deck action over the plugin.
    pub(super) fn user_title_enabled(&self, key: &Key) -> bool {
        plugin_action(key).is_none_or(|(action, _, _)| {
            self.plugin_keys
                .registry
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .action(action)
                .is_none_or(|(_, a)| a.user_title_enabled)
        })
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
        // Asked before the registry is held, which the hub holds itself to read.
        let kept = self
            .plugin_keys
            .host
            .as_ref()
            .and_then(|host| host.known_state(&id.to_string()));
        let registry = self
            .plugin_keys
            .registry
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let found = registry.action(action);
        let state = visual
            .state
            .or(kept)
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
                let image = match image_file(&path)? {
                    image if plugin.protocol == Protocol::StreamDeck => as_qt_draws(image),
                    image => image,
                };
                cache.insert(path, image.clone());
                Some(image)
            });
        let image = image.map(|image| Layer::Image {
            image,
            fit: Fit::Contain,
            inset: 0.0,
            tint: None,
        });
        let title = match found {
            Some((plugin, manifest)) if plugin.protocol == Protocol::StreamDeck => {
                let look = manifest
                    .states
                    .get(state as usize)
                    .or_else(|| manifest.states.first());
                stream_deck_title(
                    visual.title_of(state),
                    look,
                    plugin.manifest.made_for_stream_dock(),
                )
            }
            _ => visual.title_of(state).map(|content| Layer::Text {
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
            }),
        };
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
                    && *stored != settings
                {
                    *stored = settings.clone();
                    found = true;
                }
            }
        }
        if found {
            self.plugin_keys.unsaved.get_or_insert_with(Instant::now);
        }
    }

    fn store_plugin_resources(&mut self, actionid: &str, resources: Value) {
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
                    resources: stored,
                    ..
                } = action
                    && id.to_string() == actionid
                    && *stored != resources
                {
                    *stored = resources.clone();
                    self.plugin_keys.unsaved.get_or_insert_with(Instant::now);
                }
            }
        }
    }

    fn plugin_resources(&self, actionid: &str) -> Value {
        self.config
            .profiles
            .iter()
            .flat_map(|p| &p.pages)
            .flat_map(|page| page.keys.values())
            .flat_map(|key| &key.actions)
            .find_map(|action| match action {
                Action::Plugin { id, resources, .. } if id.to_string() == actionid => {
                    Some(settings_object(resources))
                }
                _ => None,
            })
            .unwrap_or_else(|| json!({}))
    }

    /// When the settings plugins saved are due to be written.
    pub(super) fn plugin_save_deadline(&self) -> Option<Instant> {
        self.plugin_keys
            .unsaved
            .map(|since| since + SETTINGS_SAVE_DELAY)
    }

    pub(super) fn save_plugin_settings(&mut self, now: Instant) {
        if self.plugin_save_deadline().is_some_and(|due| due <= now) {
            self.flush_plugin_settings();
        }
    }

    /// Writes what plugins saved and has not been written yet, as the app quits too.
    pub(super) fn flush_plugin_settings(&mut self) {
        if self.plugin_keys.unsaved.take().is_some()
            && let Err(err) = self.save()
        {
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

/// The title of a Stream Deck key: the plugin's, or the one of its state in the manifest, in
/// the look the manifest gives it, as the app the plugin was made for draws it (Stream Dock for
/// plugins with Mirabox's `APIVersion`, the Stream Deck otherwise) but within the edges the bezel
/// hides.
fn stream_deck_title(
    set: Option<String>,
    look: Option<&StateManifest>,
    mirabox: bool,
) -> Option<Layer> {
    if look.is_some_and(|l| l.show_title == Some(false)) {
        return None;
    }
    let content = set
        .or_else(|| look.and_then(|l| l.title.clone()))?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if content.trim().is_empty() {
        return None;
    }
    let font_size = look.map_or(&Value::Null, |l| &l.font_size);
    let style = look.map(|l| l.font_style.as_str()).unwrap_or_default();
    let bold = style.contains("Bold");
    let (size, weight) = if mirabox {
        let points = match font_size {
            Value::Null => STREAM_DOCK_SIZE,
            Value::Number(n) => n.as_f64().unwrap_or(0.0),
            Value::String(s) => s.trim().parse().unwrap_or(0.0),
            _ => 0.0,
        };
        let points = if points > 0.0 {
            points
        } else {
            STREAM_DOCK_UNREADABLE_SIZE
        };
        let size = (points as f32 + 2.0) * STREAM_DOCK_POINT * KEY_SIZE.0 as f32 / STREAM_DOCK_KEY;
        (size, if bold { 700 } else { 400 })
    } else {
        let points = font_size
            .as_f64()
            .filter(|p| *p > 0.0)
            .unwrap_or(ELGATO_SIZE);
        let size = points as f32 * KEY_SIZE.0 as f32 / ELGATO_KEY;
        (size, if bold || style.is_empty() { 700 } else { 400 })
    };
    let color = look
        .and_then(|l| Color::from_hex(&l.title_color))
        .unwrap_or(Color::WHITE);
    let family = look
        .map(|l| l.font_family.trim())
        .filter(|f| !f.is_empty())
        .map_or_else(
            || DEFAULT_FAMILY.to_owned(),
            |f| format!("{f}, {DEFAULT_FAMILY}"),
        );
    Some(Layer::Text {
        content,
        font_family: family,
        font_weight: weight,
        size,
        color,
        position: match look
            .map(|l| l.title_alignment.trim().to_ascii_lowercase())
            .as_deref()
        {
            None | Some("" | "bottom") => VerticalPosition::Bottom,
            Some("top") => VerticalPosition::Top,
            Some(_) => VerticalPosition::Middle,
        },
        outline: Some(Outline {
            color: Color {
                r: 255 - color.r,
                g: 255 - color.g,
                b: 255 - color.b,
            },
            width: KEY_SIZE.0 as f32 / STREAM_DOCK_KEY,
        }),
    })
}

/// What Ulanzi Studio sends when a key of a plugin shows up: `add` with its settings, then
/// `setactive`.
fn announce(host: &PluginHost, instance: &Instance, settings: &Value, resources: Value) {
    let plugin = host.plugin_id(&instance.action);
    let mut add = event("add", instance, key_fields(settings));
    if host.stream_deck(&instance.action) {
        add["resources"] = resources;
    }
    host.to_main(&plugin, &add);
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
    use crate::config::{
        Appearance, Config, DeviceSettings, LayerConfig, Page, Position, Profile, Store,
    };
    use crate::driver::d200::D200;
    use crate::engine::handle::Activity;
    use crate::engine::runtime::Links;
    use crate::plugins::{Desktop, Plugins, PluginsSetup};
    use crate::render::{Background, ImageFormat, Renderer};
    use crate::secrets::Secrets;

    const PLUGIN: &str = "com.taurilanzi.e2e.counter";
    const ACTION: &str = "com.taurilanzi.e2e.counter.count";
    const STREAM_DECK_ACTION: &str = "com.taurilanzi.streamdock.count";
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

    /// The test plugins without their main services, so nothing is started.
    fn package(plugins: &Path) {
        let stream_dock = fixture().with_file_name("com.taurilanzi.streamdock.sdPlugin");
        let dir = plugins.join("com.taurilanzi.streamdock.sdPlugin");
        fs::create_dir_all(dir.join("images")).unwrap();
        let mut manifest: Value =
            serde_json::from_slice(&fs::read(stream_dock.join("manifest.json")).unwrap()).unwrap();
        manifest.as_object_mut().unwrap().remove("CodePath");
        manifest["Actions"][0]["States"][0] = json!({
            "Image": "images/off",
            "Title": "Service\r\nName",
            "TitleAlignment": "top",
            "FontSize": "8",
        });
        fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        for image in ["icon.svg", "on.svg", "off.svg"] {
            fs::copy(
                stream_dock.join("images").join(image),
                dir.join("images").join(image),
            )
            .unwrap();
        }

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
            kind: None,
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
            resources: Value::Null,
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
            Vec::<Value>::new(),
            "the same connection does not hear of a key twice"
        );
    }

    #[test]
    fn the_page_open_in_the_editor_draws_too() {
        let (x, z) = (Uuid::new_v4(), Uuid::new_v4());
        let mut setup = setup(x, z);
        let mut main = setup.main();
        setup.engine.sync_plugins();
        assert_eq!(drain(&mut main), added(7, x, json!({ "n": 3 })));

        let other = setup.other;
        setup.engine.edit_page(Some(other));
        assert_eq!(
            drain(&mut main),
            added(0, z, json!({})),
            "off the dock, for its preview"
        );
        setup.engine.paused = true;
        setup.engine.sync_plugins();
        assert_eq!(drain(&mut main), vec![left(7, x)], "the editor's stays");
        setup.engine.paused = false;
        setup.engine.sync_plugins();
        drain(&mut main);

        setup.engine.edit_page(None);
        assert_eq!(drain(&mut main), vec![left(0, z)]);
        setup.engine.edit_page(None);
        assert!(drain(&mut main).is_empty(), "only changes are sent");
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
            id: None,
        });
        assert_eq!(drain(&mut inspector), vec![saved]);

        let due = setup
            .engine
            .plugin_save_deadline()
            .expect("waits to be written");
        setup.engine.save_plugin_settings(due);
        assert!(setup.engine.plugin_save_deadline().is_none());
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

    fn title(set: Option<&str>, look: Value, mirabox: bool) -> Option<Layer> {
        let look: StateManifest = serde_json::from_value(look).unwrap();
        stream_deck_title(set.map(str::to_owned), Some(&look), mirabox)
    }

    fn sized(layer: Option<Layer>) -> (String, f32, u16, VerticalPosition) {
        match layer {
            Some(Layer::Text {
                content,
                size,
                font_weight,
                position,
                ..
            }) => (content, size, font_weight, position),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn stream_deck_titles_are_sized_in_points_of_a_key_of_72() {
        let (content, size, weight, position) = sized(title(
            None,
            json!({ "Title": "Service\r\nName", "TitleAlignment": "top", "FontSize": "8" }),
            false,
        ));
        assert_eq!(content, "Service\nName");
        // A size written as text is not read, as on the Stream Deck.
        assert_eq!(size, 12.0 * 196.0 / 72.0);
        assert_eq!((weight, position), (700, VerticalPosition::Top));

        let (_, size, weight, position) = sized(title(
            Some("Hi"),
            json!({ "FontSize": 18, "FontStyle": "Regular" }),
            false,
        ));
        assert_eq!(size, 18.0 * 196.0 / 72.0);
        assert_eq!((weight, position), (400, VerticalPosition::Bottom));
    }

    #[test]
    fn mirabox_titles_are_sized_as_stream_dock_draws_them() {
        let dock = |points: f32| (points + 2.0) * 4.0 / 3.0 * 196.0 / 112.0;
        let near = |size: f32, points: f32| assert!((size - dock(points)).abs() < 1e-3);
        let (_, size, weight, position) = sized(title(Some("Hi"), json!({}), true));
        near(size, 10.0);
        assert_eq!((weight, position), (400, VerticalPosition::Bottom));

        let (_, size, weight, position) = sized(title(
            Some("Hi"),
            json!({ "FontSize": "8", "FontStyle": "Bold Italic", "TitleAlignment": "middle" }),
            true,
        ));
        near(size, 8.0);
        assert_eq!((weight, position), (700, VerticalPosition::Middle));

        let (_, size, ..) = sized(title(Some("Hi"), json!({ "FontSize": "big" }), true));
        near(size, 12.0);
    }

    #[test]
    fn stream_deck_titles_show_what_they_should_in_their_colour() {
        let look = json!({ "Title": "Manifest", "TitleColor": "#ff0000" });
        assert!(title(Some(""), look.clone(), false).is_none());
        assert!(
            title(
                None,
                json!({ "Title": "Manifest", "ShowTitle": false }),
                false
            )
            .is_none()
        );
        assert!(
            title(Some("Set"), json!({ "ShowTitle": false }), false).is_none(),
            "a title the user hid stays hidden whatever the plugin sets"
        );
        match title(None, look, false) {
            Some(Layer::Text {
                content,
                color,
                outline: Some(outline),
                ..
            }) => {
                assert_eq!(content, "Manifest");
                assert_eq!(color, Color { r: 255, g: 0, b: 0 });
                assert_eq!(
                    outline.color,
                    Color {
                        r: 0,
                        g: 255,
                        b: 255
                    }
                );
                assert!(outline.width < 2.0);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_text_of_the_user_is_the_title_of_a_stream_deck_key() {
        let mut setup = setup(Uuid::new_v4(), Uuid::new_v4());
        let home = setup.home;
        let place = |engine: &mut Engine, key: Key| {
            let profile = &mut engine.config.profiles[0];
            let page = profile.pages.iter_mut().find(|p| p.id == home).unwrap();
            page.keys.insert(3, key);
        };
        let texts = |engine: &Engine| -> Vec<String> {
            engine
                .design_for(3)
                .unwrap()
                .layers
                .into_iter()
                .filter_map(|layer| match layer {
                    Layer::Text { content, .. } => Some(content),
                    _ => None,
                })
                .collect()
        };
        let mut titled = key(vec![Action::Plugin {
            action: STREAM_DECK_ACTION.into(),
            id: Uuid::new_v4(),
            settings: Value::Null,
            resources: Value::Null,
        }]);
        place(&mut setup.engine, titled.clone());
        assert_eq!(texts(&setup.engine), ["Service\nName"]);

        titled.appearance.layers.push(LayerConfig::Text {
            content: "Plex".into(),
            font_family: DEFAULT_FAMILY.into(),
            font_weight: 700,
            size: 30.0,
            color: Color::WHITE,
            position: VerticalPosition::Top,
            outline: None,
        });
        place(&mut setup.engine, titled);
        assert_eq!(texts(&setup.engine), ["Plex"]);
    }

    fn stream_deck_key(setup: &mut Setup, layers: Vec<LayerConfig>) -> Uuid {
        let id = Uuid::new_v4();
        let mut placed = key(vec![Action::Plugin {
            action: STREAM_DECK_ACTION.into(),
            id,
            settings: json!({ "n": 1 }),
            resources: Value::Null,
        }]);
        placed.appearance.layers = layers;
        let home = setup.home;
        let profile = &mut setup.engine.config.profiles[0];
        let page = profile.pages.iter_mut().find(|p| p.id == home).unwrap();
        page.keys.insert(3, placed);
        id
    }

    #[test]
    fn stream_deck_images_and_titles_go_by_state() {
        let mut setup = setup(Uuid::new_v4(), Uuid::new_v4());
        let id = stream_deck_key(&mut setup, Vec::new());
        let placed = setup.engine.config.profiles[0]
            .page(setup.home)
            .unwrap()
            .keys[&3]
            .clone();
        let change = |setup: &mut Setup, change: VisualChange| {
            setup.engine.on_plugin(PluginInput::Visual {
                actionid: id.to_string(),
                change,
            });
        };
        let drawn = |setup: &Setup| match setup.engine.plugin_layers(&placed) {
            Some((Some(Layer::Image { image, .. }), title)) => (
                image,
                title.and_then(|t| match t {
                    Layer::Text { content, .. } => Some(content),
                    _ => None,
                }),
            ),
            other => panic!("{other:?}"),
        };
        let png = ImageData {
            format: ImageFormat::Png,
            bytes: Arc::from(&b"\x89PNG"[..]),
        };
        let on = fs::read(
            fixture()
                .with_file_name("com.taurilanzi.streamdock.sdPlugin")
                .join("images/on.svg"),
        )
        .unwrap();

        change(&mut setup, VisualChange::EveryState(png.clone()));
        change(&mut setup, VisualChange::SetState(1));
        assert_eq!(
            drawn(&setup).0,
            png,
            "for every state, whichever the key is in"
        );
        change(&mut setup, VisualChange::ClearStateImage(1));
        assert_eq!(
            *drawn(&setup).0.bytes,
            *on,
            "back to the manifest's for that state"
        );
        change(&mut setup, VisualChange::SetState(0));
        assert_eq!(drawn(&setup).0, png, "the other state keeps it");

        change(
            &mut setup,
            VisualChange::Title {
                state: None,
                title: Some("All".into()),
            },
        );
        change(
            &mut setup,
            VisualChange::Title {
                state: Some(1),
                title: Some("One".into()),
            },
        );
        assert_eq!(drawn(&setup).1.as_deref(), Some("All"));
        change(&mut setup, VisualChange::SetState(1));
        assert_eq!(drawn(&setup).1.as_deref(), Some("One"));
        change(
            &mut setup,
            VisualChange::Title {
                state: None,
                title: Some(String::new()),
            },
        );
        assert_eq!(drawn(&setup).1, None, "an empty title shows none");
        change(
            &mut setup,
            VisualChange::Title {
                state: None,
                title: None,
            },
        );
        assert_eq!(
            drawn(&setup).1,
            None,
            "the second state of the manifest has none"
        );
        change(&mut setup, VisualChange::SetState(0));
        assert_eq!(
            drawn(&setup).1.as_deref(),
            Some("Service\nName"),
            "the manifest's again"
        );
    }

    /// SDKs that read the font into a list of the Stream Deck's refuse the whole message if it is
    /// another, such as the app's own.
    #[test]
    fn stream_deck_plugins_are_told_only_the_fonts_the_stream_deck_offers() {
        assert_eq!(told_as(&TITLE_FONTS, "arial black"), "Arial Black");
        assert_eq!(told_as(&TITLE_FONTS, DEFAULT_FAMILY), "");
        assert_eq!(told_as(&TITLE_FONTS, ""), "");
        assert_eq!(told_as(&TITLE_STYLES, "Bold Italic"), "Bold Italic");
        assert_eq!(told_as(&TITLE_STYLES, "Heavy"), "");
    }

    #[test]
    fn stream_deck_keys_keep_their_resources_and_hear_their_title() {
        let mut setup = setup(Uuid::new_v4(), Uuid::new_v4());
        let user = LayerConfig::Text {
            content: "Plex".into(),
            font_family: DEFAULT_FAMILY.into(),
            font_weight: 400,
            size: 49.0,
            color: Color::rgb(255, 0, 0),
            position: VerticalPosition::Top,
            outline: None,
        };
        let id = stream_deck_key(&mut setup, vec![user]);
        let host = setup.engine.plugin_keys.host().unwrap().clone();
        let instance = Instance {
            action: STREAM_DECK_ACTION.into(),
            key: "3_0".into(),
            actionid: id.to_string(),
        };
        let told = setup.engine.title_event(&host, &instance).unwrap();
        assert_eq!(told["event"], "titleParametersDidChange");
        assert_eq!(told["payload"]["title"], "Plex");
        assert_eq!(
            told["payload"]["titleParameters"],
            json!({
                "fontFamily": "",
                "fontSize": 18,
                "fontStyle": "Regular",
                "fontUnderline": false,
                "showTitle": true,
                "titleAlignment": "top",
                "titleColor": "#ff0000",
            })
        );
        assert_eq!(told["payload"]["settings"], json!({ "n": 1 }));

        setup.engine.on_plugin(PluginInput::SaveResources {
            actionid: id.to_string(),
            resources: json!({ "sound": "a.mp3" }),
            from: 0,
        });
        assert_eq!(
            setup.engine.plugin_resources(&id.to_string()),
            json!({ "sound": "a.mp3" })
        );
        setup.engine.flush_plugin_settings();
        let reloaded = setup.engine.store.load().unwrap().unwrap();
        let saved = reloaded.profiles[0].page(setup.home).unwrap().keys[&3].actions[0].clone();
        assert!(
            matches!(saved, Action::Plugin { resources, .. } if resources == json!({ "sound": "a.mp3" }))
        );
    }

    #[test]
    fn the_users_image_wins_on_a_stream_deck_key() {
        let mut setup = setup(Uuid::new_v4(), Uuid::new_v4());
        let star = setup
            .engine
            .store
            .assets
            .put(
                ImageFormat::Svg,
                br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#,
            )
            .unwrap();
        stream_deck_key(
            &mut setup,
            vec![LayerConfig::Image {
                asset: star,
                fit: Fit::Contain,
                inset: 0.0,
                tint: None,
                icon: None,
                source: None,
                crop: None,
            }],
        );
        let images: Vec<Vec<u8>> = setup
            .engine
            .design_for(3)
            .unwrap()
            .layers
            .into_iter()
            .filter_map(|layer| match layer {
                Layer::Image { image, .. } => Some(image.bytes.to_vec()),
                _ => None,
            })
            .collect();
        assert_eq!(
            images,
            [br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#.to_vec()]
        );
    }
}
