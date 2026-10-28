use serde::{Deserialize, Serialize};

use super::plugins::MissingPlugin;
use uuid::Uuid;

/// How this computer is doing with the cloud, as the window shows it.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct CloudStatus {
    /// The web of the cloud, where the account and the versions are.
    pub url: String,
    pub signed_in: bool,
    /// Waiting for the user to sign in in the browser.
    pub signing_in: bool,
    pub device_name: String,
    pub paused: bool,
    pub syncing: bool,
    /// When the last sync ended well, in RFC 3339.
    pub last_sync: Option<String>,
    /// What went wrong the last time, for the user.
    pub problem: Option<String>,
    /// The account already has profiles: the user chooses what to do with this computer's.
    pub join: Option<JoinOffer>,
    /// Synced profiles with changes the cloud does not have yet.
    pub pending: Vec<Uuid>,
    /// This computer has changes to upload, in profiles or settings.
    pub outgoing: bool,
    /// The cloud has changes of other computers to download.
    pub incoming: bool,
    /// Plugins of the other computers this one could not install by itself.
    pub missing_plugins: Vec<MissingPlugin>,
    /// Profiles of the cloud deleted on this computer only.
    pub elsewhere: Vec<CloudProfile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct JoinOffer {
    pub cloud_profiles: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct CloudProfile {
    pub id: Uuid,
    pub name: String,
}

/// What to do with this computer's profiles when it joins an account that has some already.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum JoinMode {
    /// Keeps them too: both sets end up everywhere.
    Merge,
    /// Leaves only the cloud's.
    Replace,
}
