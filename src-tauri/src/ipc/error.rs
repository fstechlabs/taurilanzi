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
        Self::new(kind_of(&err), explain(&err))
    }
}

/// What went wrong, in the app's language, for the window to show as it is. The engine's own
/// messages stay in English for the logs and the MCP server.
fn explain(err: &CommandError) -> String {
    match err {
        CommandError::InvalidKey(key) => trf!("errors.invalidKey", key = key + 1),
        CommandError::NoActiveProfile => tr!("errors.noActiveProfile").into(),
        CommandError::UnknownDevice(_) => tr!("errors.unknownDevice").into(),
        CommandError::OtherDevice => tr!("errors.otherDevice").into(),
        CommandError::DeviceConnected => tr!("errors.deviceConnected").into(),
        CommandError::LastDevice => tr!("errors.lastDevice").into(),
        CommandError::UnknownProfile(_) => tr!("errors.unknownProfile").into(),
        CommandError::UnknownPage(_) => tr!("errors.unknownPage").into(),
        CommandError::DeleteHome => tr!("errors.deleteHome").into(),
        CommandError::InvalidActions(reason) => trf!("errors.invalid", reason = reason),
        CommandError::Invalid(reason) => trf!("errors.invalid", reason = reason),
        CommandError::Save(reason) => trf!("errors.save", reason = reason),
        CommandError::Render(reason) => trf!("errors.render", reason = reason),
        CommandError::EngineStopped => tr!("errors.engineStopped").into(),
        CommandError::LastProfile => tr!("errors.lastProfile").into(),
        CommandError::OnlyProfile => tr!("errors.onlyProfile").into(),
        CommandError::AppAlreadyLinked { app, profile } => {
            trf!("errors.appAlreadyLinked", app = app, profile = profile)
        }
        CommandError::UnknownApp => tr!("errors.unknownApp").into(),
        CommandError::IdTaken(_) => tr!("errors.idTaken").into(),
        CommandError::UnknownPlacement(_) => tr!("errors.unknownPlacement").into(),
        CommandError::Change { at, error } => {
            trf!("errors.change", at = at + 1, reason = explain(error))
        }
    }
}

fn kind_of(err: &CommandError) -> ErrorKind {
    match err {
        CommandError::InvalidKey(_)
        | CommandError::UnknownProfile(_)
        | CommandError::UnknownPage(_)
        | CommandError::UnknownDevice(_)
        | CommandError::UnknownPlacement(_) => ErrorKind::NotFound,
        CommandError::NoActiveProfile | CommandError::DeleteHome => ErrorKind::Conflict,
        CommandError::LastProfile
        | CommandError::OnlyProfile
        | CommandError::LastDevice
        | CommandError::DeviceConnected
        | CommandError::OtherDevice
        | CommandError::AppAlreadyLinked { .. }
        | CommandError::IdTaken(_) => ErrorKind::Conflict,
        CommandError::UnknownApp => ErrorKind::Invalid,
        CommandError::InvalidActions(_) | CommandError::Invalid(_) => ErrorKind::Invalid,
        CommandError::EngineStopped => ErrorKind::Unavailable,
        CommandError::Save(_) | CommandError::Render(_) => ErrorKind::Failed,
        CommandError::Change { error, .. } => kind_of(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_told_why_in_its_language() {
        let refused = IpcError::from(CommandError::OnlyProfile);
        assert!(matches!(refused.kind, ErrorKind::Conflict));
        assert!(refused.message.contains("cópialo"), "{}", refused.message);

        let within = IpcError::from(CommandError::Change {
            at: 1,
            error: Box::new(CommandError::DeleteHome),
        });
        assert_eq!(
            within.message,
            "Cambio 2: La página de inicio no se puede eliminar."
        );
        assert_eq!(
            CommandError::DeleteHome.to_string(),
            "the home page cannot be deleted",
            "the engine's own message stays in English for MCP"
        );
    }
}
