//! The servers of the integrations: added, changed and removed from the settings or from the
//! editor of an action. Their status travels in the snapshot.

use tauri::State;
use uuid::Uuid;

use super::error::IpcError;
use crate::config::Integration;
use crate::engine::EngineHandle;

type Result<T> = std::result::Result<T, IpcError>;

/// Adds a server, or changes the one with its id. Its client starts before this returns, so
/// its password or token can be saved right after.
#[tauri::command]
#[specta::specta]
pub async fn save_integration(
    engine: State<'_, EngineHandle>,
    integration: Integration,
) -> Result<()> {
    Ok(engine.save_integration(integration).await?)
}

/// Removes a server, with its password or token.
#[tauri::command]
#[specta::specta]
pub async fn delete_integration(engine: State<'_, EngineHandle>, id: Uuid) -> Result<()> {
    Ok(engine.delete_integration(id).await?)
}
