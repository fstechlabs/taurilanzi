//! Profiles and the apps linked to them.

use std::path::PathBuf;

use tauri::State;
use uuid::Uuid;

use super::error::{ErrorKind, IpcError};
use crate::config::AppRef;
use crate::engine::EngineHandle;
use crate::foreground::{self, AppDetection};

type Result<T> = std::result::Result<T, IpcError>;

/// A profile with an empty home page; without a name it is called "Perfil N".
#[tauri::command]
#[specta::specta]
pub async fn create_profile(engine: State<'_, EngineHandle>, name: Option<String>) -> Result<Uuid> {
    Ok(engine.create_profile(name).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn rename_profile(
    engine: State<'_, EngineHandle>,
    profile: Uuid,
    name: String,
) -> Result<()> {
    Ok(engine.rename_profile(profile, name).await?)
}

/// A copy with its own pages, without the linked apps.
#[tauri::command]
#[specta::specta]
pub async fn duplicate_profile(engine: State<'_, EngineHandle>, profile: Uuid) -> Result<Uuid> {
    Ok(engine.duplicate_profile(profile).await?)
}

/// Never the last one. Keys that switched to it stop doing so.
#[tauri::command]
#[specta::specta]
pub async fn delete_profile(engine: State<'_, EngineHandle>, profile: Uuid) -> Result<()> {
    Ok(engine.delete_profile(profile).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn move_profile(
    engine: State<'_, EngineHandle>,
    profile: Uuid,
    index: u32,
) -> Result<()> {
    Ok(engine.move_profile(profile, index as usize).await?)
}

/// Fails with `conflict` if the app is linked to another profile.
#[tauri::command]
#[specta::specta]
pub async fn link_app(engine: State<'_, EngineHandle>, profile: Uuid, app: AppRef) -> Result<()> {
    Ok(engine.link_app(profile, app).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn unlink_app(engine: State<'_, EngineHandle>, profile: Uuid, app: String) -> Result<()> {
    Ok(engine.unlink_app(profile, app).await?)
}

/// The app at a path chosen in the file dialog.
#[tauri::command]
#[specta::specta]
pub async fn app_at_path(path: String) -> Result<AppRef> {
    let path = PathBuf::from(path);
    tauri::async_runtime::spawn_blocking(move || foreground::app_at(&path))
        .await
        .ok()
        .flatten()
        .ok_or_else(|| IpcError::new(ErrorKind::Invalid, "that is not an app"))
}

/// Apps open now, to link one with a click.
#[tauri::command]
#[specta::specta]
pub async fn running_apps() -> Vec<AppRef> {
    tauri::async_runtime::spawn_blocking(foreground::running_apps)
        .await
        .unwrap_or_default()
}

/// Whether this desktop lets the app know which app is in front, which linked apps need.
#[tauri::command]
#[specta::specta]
pub async fn app_detection() -> AppDetection {
    tauri::async_runtime::spawn_blocking(foreground::detection)
        .await
        .unwrap_or(AppDetection::Unavailable)
}

/// Installs what this desktop needs to tell the app which app is in front.
#[tauri::command]
#[specta::specta]
pub async fn set_up_app_detection() -> Result<AppDetection> {
    tauri::async_runtime::spawn_blocking(foreground::set_up_detection)
        .await
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))?
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))
}
