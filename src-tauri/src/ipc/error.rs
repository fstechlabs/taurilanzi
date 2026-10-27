use serde::Serialize;

use crate::engine::CommandError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    NotFound,
    Invalid,
    Conflict,
    /// The engine is starting or has stopped.
    Unavailable,
    Failed,
}

/// What every command rejects with.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct IpcError {
    pub kind: ErrorKind,
    pub message: String,
}

impl IpcError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl From<CommandError> for IpcError {
    fn from(err: CommandError) -> Self {
        let kind = match err {
            CommandError::InvalidKey(_)
            | CommandError::UnknownProfile(_)
            | CommandError::UnknownPage(_) => ErrorKind::NotFound,
            CommandError::NoActiveProfile | CommandError::DeleteHome => ErrorKind::Conflict,
            CommandError::LastProfile | CommandError::AppAlreadyLinked { .. } => {
                ErrorKind::Conflict
            }
            CommandError::UnknownApp => ErrorKind::Invalid,
            CommandError::InvalidActions(_) => ErrorKind::Invalid,
            CommandError::EngineStopped => ErrorKind::Unavailable,
            CommandError::Save(_) | CommandError::Render(_) => ErrorKind::Failed,
        };
        Self::new(kind, err.to_string())
    }
}
