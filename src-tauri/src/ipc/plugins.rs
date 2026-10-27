//! The plugins installed, and installing, removing and restarting them.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use super::error::{ErrorKind, IpcError};
use crate::plugins::{CommunityPlugin, PluginCatalog, Plugins};

type Result<T> = std::result::Result<T, IpcError>;
type PluginState<'a> = State<'a, Option<Arc<Plugins>>>;

fn plugins(state: &PluginState<'_>) -> Result<Arc<Plugins>> {
    state
        .inner()
        .clone()
        .ok_or_else(|| IpcError::new(ErrorKind::Unavailable, tr!("plugins.notStarted")))
}

/// Every plugin found, with its actions and whether it runs.
#[tauri::command]
#[specta::specta]
pub async fn plugin_catalog(state: PluginState<'_>) -> Result<PluginCatalog> {
    Ok(plugins(&state)?.catalog())
}

/// Installs a plugin from its ZIP or its `.ulanziPlugin` folder, and returns its id.
#[tauri::command]
#[specta::specta]
pub async fn install_plugin(state: PluginState<'_>, path: String) -> Result<String> {
    let plugins = plugins(&state)?;
    tauri::async_runtime::spawn_blocking(move || plugins.install(&PathBuf::from(path)))
        .await
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))?
        .map_err(|err| IpcError::new(ErrorKind::Invalid, err.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn uninstall_plugin(state: PluginState<'_>, id: String) -> Result<()> {
    let plugins = plugins(&state)?;
    tauri::async_runtime::spawn_blocking(move || plugins.uninstall(&id))
        .await
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))?
        .map_err(|err| IpcError::new(ErrorKind::Invalid, err.to_string()))
}

/// The plugins of the community catalog that run here. Read again after a while, or at once
/// with `refresh`.
#[tauri::command]
#[specta::specta]
pub async fn community_plugins(
    state: PluginState<'_>,
    refresh: bool,
) -> Result<Vec<CommunityPlugin>> {
    let plugins = plugins(&state)?;
    tauri::async_runtime::spawn_blocking(move || plugins.community(refresh))
        .await
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))?
        .map_err(|err| IpcError::new(ErrorKind::Unavailable, err.to_string()))
}

/// Downloads a plugin of the community catalog and installs it, or updates it. Returns its id.
#[tauri::command]
#[specta::specta]
pub async fn install_community_plugin(state: PluginState<'_>, id: String) -> Result<String> {
    let plugins = plugins(&state)?;
    tauri::async_runtime::spawn_blocking(move || plugins.install_community(&id))
        .await
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))?
        .map_err(|err| IpcError::new(ErrorKind::Invalid, err))
}

/// Reads the plugin folder again, starting what is new.
#[tauri::command]
#[specta::specta]
pub async fn reload_plugins(state: PluginState<'_>) -> Result<()> {
    let plugins = plugins(&state)?;
    tauri::async_runtime::spawn_blocking(move || plugins.reload())
        .await
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn restart_plugin(state: PluginState<'_>, id: String) -> Result<()> {
    let plugins = plugins(&state)?;
    tauri::async_runtime::spawn_blocking(move || plugins.restart(&id))
        .await
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))
}

/// Opens the app's plugin folder in the file manager.
#[tauri::command]
#[specta::specta]
pub async fn show_plugins_folder(app: tauri::AppHandle, state: PluginState<'_>) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;

    let folder = plugins(&state)?.folder().to_path_buf();
    app.opener()
        .open_path(folder.to_string_lossy(), None::<&str>)
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))
}
