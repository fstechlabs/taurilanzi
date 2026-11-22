use tokio::sync::oneshot;
use uuid::Uuid;

use super::gestures::Gesture;
use super::snapshot::Snapshot;
use super::transfer::TransferPlan;
use crate::cloud::{SyncChanges, SyncExport};
use crate::config::{
    Action, AppRef, Appearance, DeviceSettings, Integration, Key, KnownPlugin, Position,
    Preferences, rules::Problem,
};
use crate::driver::DockEvent;
use crate::hass::HassReport;
use crate::obs::ObsReport;
use crate::plugins::PluginInput;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("key {0} does not exist on this device")]
    InvalidKey(u8),
    #[error("no active profile")]
    NoActiveProfile,
    #[error("dock {0} does not exist")]
    UnknownDevice(Uuid),
    #[error("a key only moves to another place of the same dock")]
    OtherDevice,
    #[error("a dock that is connected cannot be forgotten")]
    DeviceConnected,
    #[error("the last dock cannot be forgotten")]
    LastDevice,
    #[error("profile {0} does not exist")]
    UnknownProfile(Uuid),
    #[error("page {0} does not exist in this profile")]
    UnknownPage(Uuid),
    #[error("the home page cannot be deleted")]
    DeleteHome,
    #[error("{0}")]
    InvalidActions(&'static str),
    #[error("{0}")]
    Invalid(String),
    #[error("could not save the configuration: {0}")]
    Save(#[from] std::io::Error),
    #[error("could not draw the key: {0}")]
    Render(String),
    #[error("the engine is not running")]
    EngineStopped,
    #[error("the last profile cannot be deleted")]
    LastProfile,
    #[error("the only profile of a connected dock cannot be moved away: copy it instead")]
    OnlyProfile,
    #[error("{app} is already linked to {profile}")]
    AppAlreadyLinked { app: String, profile: String },
    #[error("that is not an app")]
    UnknownApp,
    #[error("the id {0} is already used by a page or a profile")]
    IdTaken(Uuid),
    #[error("no key has the plugin placement {0}")]
    UnknownPlacement(Uuid),
    #[error("change {at}: {error}")]
    Change { at: usize, error: Box<CommandError> },
}

impl From<Problem> for CommandError {
    fn from(problem: Problem) -> Self {
        Self::Invalid(problem.to_string())
    }
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

/// One edit of a lot applied all at once, which addresses pages and profiles by id, in any
/// profile. New pages and profiles get the id the caller chose, so later edits of the same lot
/// can point to them.
#[derive(Debug, Clone)]
pub enum Change {
    SetKey {
        page: Uuid,
        index: u8,
        key: Option<Key>,
    },
    MoveKey {
        from: KeySlot,
        to: KeySlot,
    },
    CreatePage {
        profile: Uuid,
        id: Uuid,
        name: Option<String>,
        position: Position,
    },
    UpdatePage {
        page: Uuid,
        changes: PageChanges,
    },
    DuplicatePage {
        page: Uuid,
        id: Uuid,
    },
    DeletePage {
        page: Uuid,
    },
    SetHomePage {
        page: Uuid,
    },
    /// On `device`, or the focused dock.
    CreateProfile {
        id: Uuid,
        home: Uuid,
        name: Option<String>,
        device: Option<Uuid>,
    },
    /// Takes a profile to another dock, or a copy of it with the id `copy`, leaving out what the
    /// dock cannot have.
    TransferProfile {
        profile: Uuid,
        device: Uuid,
        copy: Option<Uuid>,
    },
    RenameProfile {
        profile: Uuid,
        name: String,
    },
    DuplicateProfile {
        profile: Uuid,
        id: Uuid,
    },
    DeleteProfile {
        profile: Uuid,
    },
    MoveProfile {
        profile: Uuid,
        index: usize,
    },
    LinkApp {
        profile: Uuid,
        app: AppRef,
    },
    UnlinkApp {
        profile: Uuid,
        app: String,
    },
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
        device: Uuid,
        settings: DeviceSettings,
        reply: Done,
    },
    /// The dock the editor shows, which commands without a dock are about.
    FocusDevice {
        device: Uuid,
        reply: Done,
    },
    RenameDevice {
        device: Uuid,
        name: String,
        reply: Done,
    },
    /// Drops a dock that is not connected, with its profiles.
    ForgetDevice {
        device: Uuid,
        reply: Done,
    },
    /// What taking a profile to another dock would leave out or keep without working.
    TransferPlan {
        profile: Uuid,
        device: Uuid,
        reply: Reply<Result<TransferPlan, CommandError>>,
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
    /// The page open in the editor, whose plugin keys draw too, for their previews.
    EditPage {
        id: Option<Uuid>,
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
    /// What there is to sync with the cloud.
    SyncExport(Reply<SyncExport>),
    /// What other computers changed, from the cloud; answers what there is to sync after it.
    ApplySync {
        changes: SyncChanges,
        reply: Reply<Result<SyncExport, CommandError>>,
    },
    SetProfileLocal {
        id: Uuid,
        local: bool,
        reply: Done,
    },
    PluginsInstalled {
        plugins: std::collections::BTreeMap<String, KnownPlugin>,
        reply: Done,
    },
    PluginUninstalled {
        id: String,
        reply: Done,
    },
    /// Writes what is waiting to be saved, as the app quits.
    Flush(Reply<()>),
    /// The whole configuration, every profile included.
    Config(Reply<Box<crate::config::Config>>),
    /// Edits applied all or none, and undone as one.
    Apply {
        changes: Vec<Change>,
        reply: Reply<Result<bool, CommandError>>,
    },
    /// The settings of a plugin's key, as if its inspector had saved them.
    SetPluginSettings {
        placement: Uuid,
        settings: serde_json::Value,
        reply: Done,
    },
    /// What a plugin shares between its keys, as if the plugin had saved it.
    SetGlobalPluginSettings {
        plugin: String,
        settings: serde_json::Value,
        reply: Done,
    },
}

pub enum Input {
    Dock(DockEvent),
    Command(Command),
    /// A step of the actions of a key of `page` that moves its dock, sent back by the thread
    /// running them.
    Navigate {
        page: Uuid,
        step: Action,
    },
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
                | Self::Apply { .. }
                | Self::FocusDevice { .. }
        )
    }
}
