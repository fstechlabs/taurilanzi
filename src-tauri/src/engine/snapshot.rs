use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use uuid::Uuid;

use super::commands::KeySlot;
use super::screen::ScreenMode;
use crate::config::{AppRef, DeviceSettings, Integration, Page, Preferences};
use crate::driver::DeviceInfo;
use crate::hass::HassInfo;
use crate::obs::ObsInfo;

/// Everything the UI and the menu bar show, sent whole after every change.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Snapshot {
    /// The focused dock is connected.
    pub connected: bool,
    /// What the focused dock said it is, while connected.
    pub device: Option<DeviceInfo>,
    /// The dock the editor shows: the settings, profiles and pages below are its.
    pub device_id: Uuid,
    /// Every known dock, connected or not.
    pub devices: Vec<DeviceSummary>,
    /// Increases with every saved change, so key images can be fetched again.
    #[specta(type = specta_typescript::Number)]
    pub revision: u64,
    pub settings: DeviceSettings,
    pub preferences: Preferences,
    /// Whether the dock shows the keys, the screensaver or nothing.
    pub screen: ScreenMode,
    /// The dock shows nothing and its keys do nothing until it is resumed.
    pub paused: bool,
    pub profiles: Vec<ProfileSummary>,
    pub active_profile: Uuid,
    pub home_page: Uuid,
    /// Every page of the active profile, for the canvas.
    pub pages: Vec<Page>,
    /// Pages visited to reach the one on the dock, which is the last.
    pub path: Vec<Uuid>,
    /// Keys of two states now showing and doing their second one.
    pub second_states: Vec<KeySlot>,
    /// Keys whose action this dock cannot run, which show an alert.
    pub unsupported: Vec<KeySlot>,
    /// Whether there are edits to undo or redo.
    pub can_undo: bool,
    pub can_redo: bool,
    /// The last app other than this one that came to the front.
    pub front_app: Option<AppRef>,
    /// The active profile is on the dock because `front_app` is linked to it.
    pub automatic_profile: bool,
    /// The servers keys talk to.
    pub integrations: Vec<Integration>,
    /// How each OBS server is doing, by id.
    pub obs: BTreeMap<Uuid, ObsInfo>,
    /// How each Home Assistant server is doing, by id.
    pub hass: BTreeMap<Uuid, HassInfo>,
    /// The language the app shows, the system's or the one chosen.
    pub language: crate::i18n::Language,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct DeviceSummary {
    pub id: Uuid,
    /// Its model's id.
    pub model: String,
    /// As the app shows it.
    pub name: String,
    /// The name the user gave it, if any.
    pub custom_name: String,
    /// Its serial number, once a unit connected.
    pub serial: Option<String>,
    pub connected: bool,
    pub info: Option<DeviceInfo>,
    pub settings: DeviceSettings,
    pub active_profile: Uuid,
    /// The page it shows, or would show once connected.
    pub page: Uuid,
    pub profiles: u32,
    pub screen: ScreenMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct ProfileSummary {
    pub id: Uuid,
    pub name: String,
    pub apps: Vec<AppRef>,
    /// Kept on this computer only, out of the cloud.
    pub local: bool,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    State(Arc<Snapshot>),
    Key {
        device: Uuid,
        index: u8,
        pressed: bool,
    },
    /// A plugin changed how a key looks, outside a change of the configuration.
    KeyImage {
        page: Uuid,
        index: u8,
    },
}
