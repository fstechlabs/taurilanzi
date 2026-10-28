//! Home Assistant: connecting to it, its token, the codes of alarms and locks, and everything
//! the editor offers to pick from. Its status travels in the snapshot.

use tauri::State;
use uuid::Uuid;

use super::error::{ErrorKind, IpcError};
use crate::engine::EngineHandle;
use crate::hass::{Hass, HassCatalog, HassError};

type Result<T> = std::result::Result<T, IpcError>;

impl From<HassError> for IpcError {
    fn from(err: HassError) -> Self {
        let kind = match err {
            HassError::NotConnected | HassError::Lost | HassError::Stopped => {
                ErrorKind::Unavailable
            }
            HassError::Rejected { .. } | HassError::UnknownEntity(_) => ErrorKind::NotFound,
            HassError::Unsupported(_) => ErrorKind::Invalid,
            HassError::Timeout | HassError::Protocol(_) | HassError::Secrets(_) => {
                ErrorKind::Failed
            }
        };
        Self::new(kind, err.to_string())
    }
}

fn client(engine: &EngineHandle, server: Uuid) -> Result<Hass> {
    engine
        .hass(server)
        .ok_or_else(|| IpcError::new(ErrorKind::NotFound, tr!("integrations.missing")))
}

/// Tries to connect now, and keeps trying for a while even if no key uses the server yet.
#[tauri::command]
#[specta::specta]
pub fn hass_connect(engine: State<'_, EngineHandle>, server: Uuid) {
    if let Some(client) = engine.hass(server) {
        client.connect();
    }
}

/// Saves the token with the secrets and connects again with it; empty or `null` removes it. It
/// is never sent back.
#[tauri::command]
#[specta::specta]
pub async fn hass_set_token(
    engine: State<'_, EngineHandle>,
    server: Uuid,
    token: Option<String>,
) -> Result<()> {
    Ok(client(&engine, server)?.set_token(token).await?)
}

/// Entities, areas, devices and actions of Home Assistant, with its texts and icons.
#[tauri::command]
#[specta::specta]
pub async fn hass_catalog(engine: State<'_, EngineHandle>, server: Uuid) -> Result<HassCatalog> {
    Ok(client(&engine, server)?.catalog().await?)
}

/// Saves the code of an alarm panel or a lock with the secrets; empty or `null` removes it.
#[tauri::command]
#[specta::specta]
pub async fn hass_set_code(
    engine: State<'_, EngineHandle>,
    server: Uuid,
    entity: String,
    code: Option<String>,
) -> Result<()> {
    Ok(client(&engine, server)?.set_code(entity, code).await?)
}

/// Whether a code is saved for an alarm panel or a lock. The code itself is never sent back.
#[tauri::command]
#[specta::specta]
pub async fn hass_code_saved(
    engine: State<'_, EngineHandle>,
    server: Uuid,
    entity: String,
) -> Result<bool> {
    Ok(client(&engine, server)?.code_saved(entity).await?)
}
