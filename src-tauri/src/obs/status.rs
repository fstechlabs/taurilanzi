//! What the app knows about OBS: the connection, and the states keys of two states show.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::config::ObsCommand;

/// How the link to OBS is doing.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ObsStatus {
    /// No key uses OBS and nobody asked to connect.
    #[default]
    Off,
    Connecting,
    Connected {
        obs_version: String,
        websocket_version: String,
    },
    /// Still trying every few seconds, except after a wrong password.
    Failed {
        reason: ObsFailure,
        detail: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ObsFailure {
    /// Nothing answers at that address: OBS is closed or its WebSocket server is off.
    Unreachable,
    /// OBS asks for a password and none is saved.
    PasswordRequired,
    WrongPassword,
    /// An OBS older than 28, without obs-websocket 5.
    Incompatible,
    Other,
}

/// What the UI shows about OBS. Never the password itself.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, specta::Type)]
pub struct ObsInfo {
    pub status: ObsStatus,
    pub password_set: bool,
}

/// A state of OBS a key of two states can follow, showing its second state while it is on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Watch {
    Recording,
    RecordPaused,
    Streaming,
    VirtualCam,
    ReplayBuffer,
    StudioMode,
    ProgramScene(String),
    PreviewScene(String),
    SourceVisible { scene: String, source: String },
    FilterEnabled { source: String, filter: String },
    InputMuted(String),
}

impl Watch {
    pub fn of(command: &ObsCommand) -> Option<Self> {
        Some(match command {
            ObsCommand::Record { .. } => Self::Recording,
            ObsCommand::RecordPause { .. } => Self::RecordPaused,
            ObsCommand::Stream { .. } => Self::Streaming,
            ObsCommand::VirtualCam { .. } => Self::VirtualCam,
            ObsCommand::ReplayBuffer { .. } => Self::ReplayBuffer,
            ObsCommand::StudioMode { .. } => Self::StudioMode,
            ObsCommand::Scene {
                scene,
                preview: false,
            } => Self::ProgramScene(scene.clone()),
            ObsCommand::Scene {
                scene,
                preview: true,
            } => Self::PreviewScene(scene.clone()),
            ObsCommand::SourceVisibility { scene, source, .. } => Self::SourceVisible {
                scene: scene.clone(),
                source: source.clone(),
            },
            ObsCommand::Filter { source, filter, .. } => Self::FilterEnabled {
                source: source.clone(),
                filter: filter.clone(),
            },
            ObsCommand::Mute { input, .. } => Self::InputMuted(input.clone()),
            _ => return None,
        })
    }

    /// States of one source, filter or input, which OBS has to be asked for one by one.
    pub fn is_of_an_item(&self) -> bool {
        matches!(
            self,
            Self::SourceVisible { .. } | Self::FilterEnabled { .. } | Self::InputMuted(_)
        )
    }
}

/// OBS's states as it last reported them; all off while it is not connected.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ObsStates {
    pub recording: bool,
    pub record_paused: bool,
    pub streaming: bool,
    pub virtual_cam: bool,
    pub replay_buffer: bool,
    pub studio_mode: bool,
    pub program_scene: Option<String>,
    pub preview_scene: Option<String>,
    /// Only for the items some key follows.
    pub items: BTreeMap<Watch, bool>,
}

impl ObsStates {
    pub fn is_on(&self, watch: &Watch) -> bool {
        match watch {
            Watch::Recording => self.recording,
            Watch::RecordPaused => self.record_paused,
            Watch::Streaming => self.streaming,
            Watch::VirtualCam => self.virtual_cam,
            Watch::ReplayBuffer => self.replay_buffer,
            Watch::StudioMode => self.studio_mode,
            Watch::ProgramScene(scene) => self.program_scene.as_ref() == Some(scene),
            Watch::PreviewScene(scene) => {
                self.studio_mode && self.preview_scene.as_ref() == Some(scene)
            }
            item => self.items.get(item).copied().unwrap_or(false),
        }
    }
}

/// Sent to the engine whenever any of it changes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ObsReport {
    pub info: ObsInfo,
    pub states: ObsStates,
}

impl ObsReport {
    pub fn connected(&self) -> bool {
        matches!(self.info.status, ObsStatus::Connected { .. })
    }
}

/// What the engine needs from the client, from the configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wants {
    /// Some key of some profile sends commands to OBS.
    pub connect: bool,
    pub host: String,
    pub port: u16,
    /// States keys of two states follow.
    pub watches: BTreeSet<Watch>,
}
