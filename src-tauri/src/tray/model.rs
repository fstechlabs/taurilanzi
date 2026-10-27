use uuid::Uuid;

use crate::config::DeviceSettings;
use crate::driver::DeviceInfo;
use crate::engine::{ProfileSummary, Snapshot};
use crate::login::Placement;
use crate::updates::Status;

/// Everything the menu bar shows. The menu is rebuilt only when this changes.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// None until the engine has sent its first state.
    pub dock: Option<DockState>,
    pub managed: bool,
    pub translocated: bool,
    pub update: Status,
}

/// The part of the engine state the menu shows.
#[derive(Debug, Clone, PartialEq)]
pub struct DockState {
    pub connected: bool,
    pub device: Option<DeviceInfo>,
    pub settings: DeviceSettings,
    pub paused: bool,
    pub profiles: Vec<ProfileSummary>,
    pub active_profile: Uuid,
    /// The app in front that put the active profile on the dock.
    pub chosen_by: Option<String>,
    pub language: crate::i18n::Language,
}

impl From<&Snapshot> for DockState {
    fn from(snapshot: &Snapshot) -> Self {
        Self {
            connected: snapshot.connected,
            device: snapshot.device.clone(),
            settings: snapshot.settings.clone(),
            paused: snapshot.paused,
            profiles: snapshot.profiles.clone(),
            active_profile: snapshot.active_profile,
            chosen_by: snapshot
                .front_app
                .as_ref()
                .filter(|_| snapshot.automatic_profile)
                .map(|app| app.name.clone()),
            language: snapshot.language,
        }
    }
}

impl Model {
    pub fn new(snapshot: Option<&Snapshot>, placement: &Placement, update: Status) -> Self {
        Self {
            dock: snapshot.map(DockState::from),
            managed: placement.login_item().is_some(),
            translocated: matches!(placement, Placement::Translocated),
            update,
        }
    }

    pub fn status_line(&self) -> String {
        match &self.dock {
            None => tr!("tray.status.starting").into(),
            Some(dock) if dock.paused => tr!("tray.status.paused").into(),
            Some(dock) => match &dock.device {
                Some(device) if dock.connected => {
                    trf!("tray.status.connected", model = device.model)
                }
                _ => tr!("tray.status.disconnected").into(),
            },
        }
    }
}
