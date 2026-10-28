use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::actions::Action;
use super::integrations::Integration;
use super::known_plugins::KnownPlugin;
use super::linked_apps::AppRef;
use super::preferences::Preferences;
use super::widgets::Widget;
use crate::render::{Background, Color, Fit, Outline, VerticalPosition};

pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub schema: u32,
    /// Increases on every save, so sync can tell which copy is newer.
    pub revision: u64,
    pub device: DeviceSettings,
    #[serde(default)]
    pub preferences: Preferences,
    pub active_profile: Uuid,
    pub profiles: Vec<Profile>,
    /// What each plugin shares between its keys, by plugin id: tokens and addresses included,
    /// so they travel with the configuration like the rest.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plugin_settings: BTreeMap<String, serde_json::Value>,
    /// The OBS Studios and Home Assistants keys talk to.
    #[serde(default)]
    pub integrations: Vec<Integration>,
    /// Profiles kept on this computer only, out of the cloud. The others are synced.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub local_profiles: BTreeSet<Uuid>,
    /// The plugins installed on any computer of the account, by plugin id. Synced, so each
    /// computer installs what the others have.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plugins: BTreeMap<String, KnownPlugin>,
    /// Plugins uninstalled on this computer, which the others do not install here again.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub declined_plugins: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct DeviceSettings {
    pub brightness: u8,
    /// `0` disables sleep.
    pub sleep_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    /// Where the dock starts, and where "Ir al inicio" leads. Schema 1 called it `active_page`.
    #[serde(alias = "active_page")]
    pub home_page: Uuid,
    pub pages: Vec<Page>,
    /// Apps that bring this profile to the dock when they come to the front. An app belongs to
    /// one profile at most.
    #[serde(default)]
    pub apps: Vec<AppRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Page {
    pub id: Uuid,
    pub name: String,
    /// Keyed by key index; the wide key is index 13.
    pub keys: BTreeMap<u8, Key>,
    /// Where the page sits on its profile's canvas.
    pub position: Position,
}

/// Schema 1 had no canvas, so its pages have no position.
#[derive(Deserialize)]
struct StoredPage {
    id: Uuid,
    name: String,
    keys: BTreeMap<u8, Key>,
    #[serde(default)]
    position: Position,
}

impl<'de> Deserialize<'de> for Page {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let StoredPage {
            id,
            name,
            keys,
            position,
        } = StoredPage::deserialize(deserializer)?;
        Ok(Self {
            id,
            name,
            keys,
            position,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Position {
    #[specta(type = specta_typescript::Number)]
    pub x: f32,
    #[specta(type = specta_typescript::Number)]
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Key {
    pub appearance: Appearance,
    /// Run in order on press. At most one of them navigates.
    pub actions: Vec<Action>,
    /// Run instead of `actions` on a second press right after the first.
    pub double_press: Vec<Action>,
    /// Run instead of `actions` when the key is held down.
    pub long_press: Vec<Action>,
    /// A key with two states shows and does this every other press. Not with gestures.
    pub second_state: Option<KeyState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct KeyState {
    pub appearance: Appearance,
    pub actions: Vec<Action>,
}

/// Schema 1 had a single optional `action` per key, and keys without gestures or states.
#[derive(Deserialize)]
struct StoredKey {
    appearance: Appearance,
    #[serde(default)]
    actions: Vec<Action>,
    #[serde(default)]
    action: Option<Action>,
    #[serde(default)]
    double_press: Vec<Action>,
    #[serde(default)]
    long_press: Vec<Action>,
    #[serde(default)]
    second_state: Option<KeyState>,
}

impl From<StoredKey> for Key {
    fn from(stored: StoredKey) -> Self {
        let mut actions = stored.actions;
        if actions.is_empty() {
            actions.extend(stored.action);
        }
        Self {
            appearance: stored.appearance,
            actions,
            double_press: stored.double_press,
            long_press: stored.long_press,
            second_state: stored.second_state,
        }
    }
}

impl Key {
    /// Every action of the key: on press, on the gestures and in the second state.
    pub fn all_actions(&self) -> impl Iterator<Item = &Action> {
        self.actions
            .iter()
            .chain(&self.double_press)
            .chain(&self.long_press)
            .chain(self.second_state.iter().flat_map(|state| &state.actions))
    }

    pub fn appearances(&self) -> impl Iterator<Item = &Appearance> {
        std::iter::once(&self.appearance).chain(self.second_state.iter().map(|s| &s.appearance))
    }

    pub fn has_gestures(&self) -> bool {
        !self.double_press.is_empty() || !self.long_press.is_empty()
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        StoredKey::deserialize(deserializer).map(Self::from)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Appearance {
    pub background: Background,
    pub layers: Vec<LayerConfig>,
}

/// The renderer never reads it: it draws the edited asset as it is. The image is scaled to
/// cover the frame, then zoomed, moved and rotated around the frame center. Offsets are fractions
/// of the frame, so they do not depend on the key size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Crop {
    #[specta(type = specta_typescript::Number)]
    pub zoom: f32,
    #[specta(type = specta_typescript::Number)]
    pub x: f32,
    #[specta(type = specta_typescript::Number)]
    pub y: f32,
    /// Degrees, clockwise.
    #[specta(type = specta_typescript::Number)]
    pub rotation: f32,
    pub flip_x: bool,
    pub flip_y: bool,
}

/// Same as the renderer layers, but images point to the asset store instead of holding bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayerConfig {
    Image {
        asset: String,
        fit: Fit,
        #[specta(type = specta_typescript::Number)]
        inset: f32,
        #[serde(default)]
        tint: Option<Color>,
        /// Iconify key the image came from, so the editor can show it.
        #[serde(default)]
        icon: Option<String>,
        /// Original upload when `asset` is an edited copy of it.
        #[serde(default)]
        source: Option<String>,
        /// How `source` was framed to produce `asset`, so the editor can reopen it.
        #[serde(default)]
        crop: Option<Crop>,
    },
    Text {
        content: String,
        font_family: String,
        font_weight: u16,
        #[specta(type = specta_typescript::Number)]
        size: f32,
        color: Color,
        position: VerticalPosition,
        outline: Option<Outline>,
    },
    Border {
        color: Color,
        #[specta(type = specta_typescript::Number)]
        width: f32,
        #[specta(type = specta_typescript::Number)]
        radius: f32,
    },
    Widget {
        widget: Widget,
    },
}

impl Profile {
    pub fn page(&self, id: Uuid) -> Option<&Page> {
        self.pages.iter().find(|p| p.id == id)
    }

    pub fn page_mut(&mut self, id: Uuid) -> Option<&mut Page> {
        self.pages.iter_mut().find(|p| p.id == id)
    }
}

impl Config {
    pub fn active_profile(&self) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == self.active_profile)
    }

    pub fn active_profile_mut(&mut self) -> Option<&mut Profile> {
        let id = self.active_profile;
        self.profiles.iter_mut().find(|p| p.id == id)
    }
}

/// Every image and sound the keys point to, edited copies and their originals included.
pub fn assets_in(profiles: &[Profile]) -> HashSet<&str> {
    profiles
        .iter()
        .flat_map(|profile| &profile.pages)
        .flat_map(|page| page.keys.values())
        .flat_map(|key| {
            let images = key
                .appearances()
                .flat_map(|a| &a.layers)
                .flat_map(|layer| match layer {
                    LayerConfig::Image { asset, source, .. } => std::iter::once(asset.as_str())
                        .chain(source.as_deref())
                        .collect(),
                    LayerConfig::Widget { widget } => widget.assets(),
                    _ => Vec::new(),
                });
            let sounds = key.all_actions().flat_map(Action::sounds);
            images.chain(sounds).collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_of_second_states_and_gestures_are_in_use() {
        let image = |asset: &str| Appearance {
            background: Background::Solid {
                color: Color::BLACK,
            },
            layers: vec![LayerConfig::Image {
                asset: asset.into(),
                fit: Fit::Contain,
                inset: 0.0,
                tint: None,
                icon: None,
                source: None,
                crop: None,
            }],
        };
        let sound = |id: &str| Action::PlaySound {
            sound: id.into(),
            name: String::new(),
            volume: 50,
            mode: crate::config::SoundMode::Restart,
            device: None,
        };
        let key = Key {
            appearance: image("first.png"),
            actions: Vec::new(),
            double_press: vec![sound("double.wav")],
            long_press: Vec::new(),
            second_state: Some(KeyState {
                appearance: image("second.png"),
                actions: vec![sound("second.wav")],
            }),
        };
        let profile = Profile {
            id: Uuid::nil(),
            name: String::new(),
            home_page: Uuid::nil(),
            pages: vec![Page {
                id: Uuid::nil(),
                name: String::new(),
                keys: BTreeMap::from([(0, key)]),
                position: Position::default(),
            }],
            apps: Vec::new(),
        };
        let used = assets_in(std::slice::from_ref(&profile));
        for id in ["first.png", "second.png", "double.wav", "second.wav"] {
            assert!(used.contains(id), "{id}");
        }
    }

    #[test]
    fn round_trips_through_json() {
        let page = Page {
            id: Uuid::new_v4(),
            name: "Main".into(),
            keys: BTreeMap::from([(
                0,
                Key {
                    appearance: Appearance {
                        background: Background::Solid {
                            color: Color::rgb(1, 2, 3),
                        },
                        layers: vec![LayerConfig::Image {
                            asset: "abc".into(),
                            fit: Fit::Contain,
                            inset: 0.2,
                            tint: None,
                            icon: None,
                            source: None,
                            crop: None,
                        }],
                    },
                    actions: vec![
                        Action::OpenUrl {
                            url: "https://example.com".into(),
                        },
                        Action::Back,
                    ],
                    double_press: Vec::new(),
                    long_press: Vec::new(),
                    second_state: None,
                },
            )]),
            position: Position { x: 40.0, y: -20.0 },
        };
        let profile = Profile {
            id: Uuid::new_v4(),
            name: "Default".into(),
            home_page: page.id,
            pages: vec![page],
            apps: Vec::new(),
        };
        let config = Config {
            schema: SCHEMA_VERSION,
            revision: 3,
            device: DeviceSettings {
                brightness: 70,
                sleep_minutes: 0,
            },
            preferences: Preferences::default(),
            active_profile: profile.id,
            profiles: vec![profile],
            plugin_settings: Default::default(),
            integrations: Vec::new(),
            local_profiles: Default::default(),
            plugins: Default::default(),
            declined_plugins: Default::default(),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), config);
        let profile = config.active_profile().unwrap();
        assert_eq!(profile.page(profile.home_page).unwrap().name, "Main");
    }

    #[test]
    fn lines_keep_their_waypoints_and_omit_none() {
        let page = Uuid::new_v4();
        let bent = Action::OpenPage {
            page,
            waypoints: vec![Position { x: 1.0, y: 2.0 }],
        };
        let json = serde_json::to_string(&bent).unwrap();
        assert_eq!(serde_json::from_str::<Action>(&json).unwrap(), bent);
        let straight = serde_json::to_string(&Action::OpenPage {
            page,
            waypoints: Vec::new(),
        })
        .unwrap();
        assert!(!straight.contains("waypoints"), "{straight}");
    }

    #[test]
    fn reads_schema_1_keys_and_profiles() {
        let profile: Profile = serde_json::from_str(
            r#"{"id":"6f1c0d52-4a43-4f39-9a0e-1a2b3c4d5e6f","name":"P",
                "active_page":"0e1f2a3b-4c5d-4e6f-8a9b-0c1d2e3f4a5b",
                "pages":[{"id":"0e1f2a3b-4c5d-4e6f-8a9b-0c1d2e3f4a5b","name":"Inicio","keys":{
                  "0":{"appearance":{"background":{"type":"solid","color":{"r":0,"g":0,"b":0}},"layers":[]},
                       "action":{"type":"open_url","url":"https://a.example"}},
                  "1":{"appearance":{"background":{"type":"solid","color":{"r":0,"g":0,"b":0}},"layers":[]},
                       "action":null}}}]}"#,
        )
        .unwrap();
        assert_eq!(profile.home_page, profile.pages[0].id);
        let keys = &profile.pages[0].keys;
        assert_eq!(
            keys[&0].actions,
            [Action::OpenUrl {
                url: "https://a.example".into()
            }]
        );
        assert!(keys[&1].actions.is_empty());
        assert_eq!(profile.pages[0].position, Position::default());
    }

    #[test]
    fn keeps_the_original_image_and_its_crop() {
        let layer: LayerConfig = serde_json::from_str(
            r#"{"type":"image","asset":"edited.png","fit":"cover","inset":0,
                "source":"original.jpg","adjust":{"zoom":3},
                "crop":{"zoom":1.5,"x":0.1,"y":0,"rotation":90,"flip_x":true,"flip_y":false}}"#,
        )
        .unwrap();
        let LayerConfig::Image { source, crop, .. } = layer else {
            panic!("not an image");
        };
        assert_eq!(source.as_deref(), Some("original.jpg"));
        assert_eq!(
            crop.map(|c| (c.zoom, c.rotation, c.flip_x)),
            Some((1.5, 90.0, true))
        );
    }
}
