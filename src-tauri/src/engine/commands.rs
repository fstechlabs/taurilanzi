use tokio::sync::oneshot;
use uuid::Uuid;

use super::gestures::Gesture;
use super::snapshot::Snapshot;
use crate::config::{
    Action, AppRef, Appearance, DeviceSettings, Integration, Key, Position, Preferences,
};
use crate::driver::d200::Event as DockEvent;
use crate::hass::HassReport;
use crate::obs::ObsReport;
use crate::plugins::PluginInput;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("key {0} does not exist on this device")]
    InvalidKey(u8),
    #[error("no active profile")]
    NoActiveProfile,
    #[error("profile {0} does not exist")]
    UnknownProfile(Uuid),
    #[error("page {0} does not exist in this profile")]
    UnknownPage(Uuid),
    #[error("the home page cannot be deleted")]
    DeleteHome,
    #[error("{0}")]
    InvalidActions(&'static str),
    #[error("could not save the configuration: {0}")]
    Save(#[from] std::io::Error),
    #[error("could not draw the key: {0}")]
    Render(String),
    #[error("the engine is not running")]
    EngineStopped,
    #[error("the last profile cannot be deleted")]
    LastProfile,
    #[error("{app} is already linked to {profile}")]
    AppAlreadyLinked { app: String, profile: String },
    #[error("that is not an app")]
    UnknownApp,
}

/// Changes to a page's name or place on the canvas; missing fields stay as they are.
#[derive(Debug, Clone, Default, serde::Deserialize, specta::Type)]
pub struct PageChanges {
    pub name: Option<String>,
    pub position: Option<Position>,
}

/// A place for a key: a page of the active profile and a key index on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct KeySlot {
    pub page: Uuid,
    pub index: u8,
}

pub(super) type Reply<T> = oneshot::Sender<T>;
type Done = Reply<Result<(), CommandError>>;

pub enum Command {
    Snapshot(Reply<Snapshot>),
    SetKey {
        page: Uuid,
        index: u8,
        key: Option<Key>,
        reply: Done,
    },
    SetDevice {
        settings: DeviceSettings,
        reply: Done,
    },
    SetPreferences {
        preferences: Preferences,
        reply: Done,
    },
    /// Shows the screensaver on the dock for a while, or stops showing it.
    TryScreensaver {
        on: bool,
        reply: Reply<()>,
    },
    /// A stored key as the dock shows it.
    KeyImage {
        page: Uuid,
        index: u8,
        reply: Reply<Result<Vec<u8>, CommandError>>,
    },
    /// Unsaved edits, drawn at the size of key `index`, with the values of `page` in its text.
    Preview {
        page: Option<Uuid>,
        index: u8,
        appearance: Appearance,
        reply: Reply<Result<Vec<u8>, CommandError>>,
    },
    Fonts(Reply<Vec<String>>),
    SetPaused {
        paused: bool,
        reply: Reply<()>,
    },
    SetProfile {
        id: Uuid,
        reply: Done,
    },
    CreatePage {
        name: Option<String>,
        position: Position,
        reply: Reply<Result<Uuid, CommandError>>,
    },
    UpdatePage {
        id: Uuid,
        changes: PageChanges,
        reply: Done,
    },
    DuplicatePage {
        id: Uuid,
        reply: Reply<Result<Uuid, CommandError>>,
    },
    DeletePage {
        id: Uuid,
        reply: Done,
    },
    SetHomePage {
        id: Uuid,
        reply: Done,
    },
    /// Navigates the dock to a page, as if a key leading there had been pressed.
    ShowPage {
        id: Uuid,
        reply: Done,
    },
    /// Moves a key to another place, on its page or another one; swaps if that place is taken.
    MoveKey {
        from: KeySlot,
        to: KeySlot,
        reply: Done,
    },
    /// Runs a key's actions as if it had been pressed, so they can be tried without the dock.
    PressKey {
        page: Uuid,
        index: u8,
        gesture: Gesture,
        reply: Done,
    },
    Undo(Done),
    Redo(Done),
    CreateProfile {
        name: Option<String>,
        reply: Reply<Result<Uuid, CommandError>>,
    },
    RenameProfile {
        id: Uuid,
        name: String,
        reply: Done,
    },
    DuplicateProfile {
        id: Uuid,
        reply: Reply<Result<Uuid, CommandError>>,
    },
    DeleteProfile {
        id: Uuid,
        reply: Done,
    },
    MoveProfile {
        id: Uuid,
        index: usize,
        reply: Done,
    },
    LinkApp {
        profile: Uuid,
        app: AppRef,
        reply: Done,
    },
    UnlinkApp {
        profile: Uuid,
        app: String,
        reply: Done,
    },
    SaveIntegration {
        integration: Integration,
        reply: Done,
    },
    DeleteIntegration {
        id: Uuid,
        reply: Done,
    },
}

pub enum Input {
    Dock(DockEvent),
    Command(Command),
    /// A step of a key's actions that moves the dock, sent back by the thread running them.
    Navigate(Action),
    /// The screen of the Mac locked or unlocked.
    Locked(bool),
    /// Another app came to the front.
    FrontApp(AppRef),
    /// The connection to an OBS or one of its states changed.
    Obs(Uuid, ObsReport),
    /// The connection to a Home Assistant or something its keys show changed.
    Hass(Uuid, HassReport),
    /// An automation of a Home Assistant asks to move the dock.
    HassRemote(serde_json::Value),
    /// A key's request went through or failed, for the key to show it.
    KeyResult {
        slot: KeySlot,
        ok: bool,
    },
    /// A plugin connected or asked for something about its keys.
    Plugin(PluginInput),
}

impl Command {
    /// Whether it edits or uses the keys, which wakes a dock left idle.
    pub fn changes_keys(&self) -> bool {
        matches!(
            self,
            Self::SetKey { .. }
                | Self::SetProfile { .. }
                | Self::CreatePage { .. }
                | Self::UpdatePage { .. }
                | Self::DuplicatePage { .. }
                | Self::DeletePage { .. }
                | Self::SetHomePage { .. }
                | Self::ShowPage { .. }
                | Self::MoveKey { .. }
                | Self::PressKey { .. }
                | Self::Undo(_)
                | Self::Redo(_)
                | Self::DeleteProfile { .. }
        )
    }
}
