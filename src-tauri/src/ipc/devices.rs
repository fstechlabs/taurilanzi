//! The docks: their models, which one the editor shows, their settings and names, and taking a
//! profile from one to another.

use tauri::State;
use uuid::Uuid;

use super::error::IpcError;
use crate::config::DeviceSettings;
use crate::driver::DeviceModel;
use crate::engine::{Change, EngineHandle, TransferPlan};

type Result<T> = std::result::Result<T, IpcError>;

/// Every dock model the app drives, to draw them.
#[tauri::command]
#[specta::specta]
pub fn dock_models() -> Vec<DeviceModel> {
    crate::driver::models().to_vec()
}

#[tauri::command]
#[specta::specta]
pub async fn set_device(
    engine: State<'_, EngineHandle>,
    device: Uuid,
    settings: DeviceSettings,
) -> Result<()> {
    Ok(engine.set_device(device, settings).await?)
}

/// The dock the editor shows.
#[tauri::command]
#[specta::specta]
pub async fn focus_device(engine: State<'_, EngineHandle>, device: Uuid) -> Result<()> {
    Ok(engine.focus_device(device).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn rename_device(
    engine: State<'_, EngineHandle>,
    device: Uuid,
    name: String,
) -> Result<()> {
    Ok(engine.rename_device(device, name).await?)
}

/// Drops a dock that is not connected, with its profiles.
#[tauri::command]
#[specta::specta]
pub async fn forget_device(engine: State<'_, EngineHandle>, device: Uuid) -> Result<()> {
    Ok(engine.forget_device(device).await?)
}

/// What taking a profile to another dock would leave out, and what would not work there.
#[tauri::command]
#[specta::specta]
pub async fn transfer_plan(
    engine: State<'_, EngineHandle>,
    profile: Uuid,
    device: Uuid,
) -> Result<TransferPlan> {
    Ok(engine.transfer_plan(profile, device).await?)
}

/// Moves a profile to another dock, or puts a copy there; answers the id it has there. One step
/// of undo.
#[tauri::command]
#[specta::specta]
pub async fn transfer_profile(
    engine: State<'_, EngineHandle>,
    profile: Uuid,
    device: Uuid,
    copy: bool,
) -> Result<Uuid> {
    let copy = copy.then(Uuid::new_v4);
    engine
        .apply(vec![Change::TransferProfile {
            profile,
            device,
            copy,
        }])
        .await?;
    Ok(copy.unwrap_or(profile))
}
