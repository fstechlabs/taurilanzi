//! OBS Studio: connecting to it, its password and the names the editor suggests. Its status
//! travels in the snapshot.

use tauri::State;
use uuid::Uuid;

use super::error::{ErrorKind, IpcError};
use crate::engine::EngineHandle;
use crate::obs::{Obs, ObsError, ObsList};

type Result<T> = std::result::Result<T, IpcError>;

impl From<ObsError> for IpcError {
    fn from(err: ObsError) -> Self {
        let kind = match err {
            ObsError::NotConnected | ObsError::Lost | ObsError::Stopped => ErrorKind::Unavailable,
            ObsError::Rejected { .. } => ErrorKind::NotFound,
            ObsError::Timeout | ObsError::Protocol(_) | ObsError::Secrets(_) => ErrorKind::Failed,
        };
        Self::new(kind, err.to_string())
    }
}

fn client(engine: &EngineHandle, server: Uuid) -> Result<Obs> {
    engine
        .obs(server)
        .ok_or_else(|| IpcError::new(ErrorKind::NotFound, tr!("integrations.missing")))
}

/// Tries to connect now, and keeps trying for a while even if no key uses the server yet.
#[tauri::command]
#[specta::specta]
pub fn obs_connect(engine: State<'_, EngineHandle>, server: Uuid) {
    if let Some(client) = engine.obs(server) {
        client.connect();
    }
}

/// Saves the password with the secrets and connects again with it; empty or `null` removes it.
/// It is never sent back.
#[tauri::command]
#[specta::specta]
pub async fn obs_set_password(
    engine: State<'_, EngineHandle>,
    server: Uuid,
    password: Option<String>,
) -> Result<()> {
    Ok(client(&engine, server)?.set_password(password).await?)
}

/// Names of scenes, sources, filters… as OBS has them now, to suggest them in the editor.
#[tauri::command]
#[specta::specta]
pub async fn obs_names(
    engine: State<'_, EngineHandle>,
    server: Uuid,
    list: ObsList,
) -> Result<Vec<String>> {
    Ok(client(&engine, server)?.names(list).await?)
}
