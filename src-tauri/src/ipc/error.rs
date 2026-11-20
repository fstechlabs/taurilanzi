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
        Self::new(kind_of(&err), err.to_string())
    }
}

fn kind_of(err: &CommandError) -> ErrorKind {
    match err {
        CommandError::InvalidKey(_)
        | CommandError::UnknownProfile(_)
        | CommandError::UnknownPage(_)
        | CommandError::UnknownPlacement(_) => ErrorKind::NotFound,
        CommandError::NoActiveProfile | CommandError::DeleteHome => ErrorKind::Conflict,
        CommandError::LastProfile
        | CommandError::AppAlreadyLinked { .. }
        | CommandError::IdTaken(_) => ErrorKind::Conflict,
        CommandError::UnknownApp => ErrorKind::Invalid,
        CommandError::InvalidActions(_) | CommandError::Invalid(_) => ErrorKind::Invalid,
        CommandError::EngineStopped => ErrorKind::Unavailable,
        CommandError::Save(_) | CommandError::Render(_) => ErrorKind::Failed,
        CommandError::Change { error, .. } => kind_of(error),
    }
}
