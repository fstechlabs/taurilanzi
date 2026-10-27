//! One command per operation the UI can ask for. Each one only hands the request to the engine.

use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use super::error::IpcError;
use crate::config::{DeviceSettings, Key, Position, Preferences};
use crate::engine::{EngineHandle, Gesture, KeySlot, PageChanges, Snapshot};
use crate::plugins::Plugins;

type Result<T> = std::result::Result<T, IpcError>;

#[tauri::command]
#[specta::specta]
pub async fn get_state(engine: State<'_, EngineHandle>) -> Result<Snapshot> {
    Ok(engine.snapshot().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn set_key(
    engine: State<'_, EngineHandle>,
    page: Uuid,
    index: u8,
    key: Key,
) -> Result<()> {
    Ok(engine.set_key(page, index, Some(key)).await?)
}

/// Removes the key, and with it the line that leaves from it.
#[tauri::command]
#[specta::specta]
pub async fn clear_key(engine: State<'_, EngineHandle>, page: Uuid, index: u8) -> Result<()> {
    Ok(engine.set_key(page, index, None).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn move_key(engine: State<'_, EngineHandle>, from: KeySlot, to: KeySlot) -> Result<()> {
    Ok(engine.move_key(from, to).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn create_page(
    engine: State<'_, EngineHandle>,
    name: Option<String>,
    position: Position,
) -> Result<Uuid> {
    Ok(engine.create_page(name, position).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn update_page(
    engine: State<'_, EngineHandle>,
    page: Uuid,
    changes: PageChanges,
) -> Result<()> {
    Ok(engine.update_page(page, changes).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn duplicate_page(engine: State<'_, EngineHandle>, page: Uuid) -> Result<Uuid> {
    Ok(engine.duplicate_page(page).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_page(engine: State<'_, EngineHandle>, page: Uuid) -> Result<()> {
    Ok(engine.delete_page(page).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn set_home_page(engine: State<'_, EngineHandle>, page: Uuid) -> Result<()> {
    Ok(engine.set_home_page(page).await?)
}

/// Sends the dock to a page, as if a key leading there had been pressed.
#[tauri::command]
#[specta::specta]
pub async fn show_page(engine: State<'_, EngineHandle>, page: Uuid) -> Result<()> {
    Ok(engine.show_page(page).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn set_device(engine: State<'_, EngineHandle>, settings: DeviceSettings) -> Result<()> {
    Ok(engine.set_device(settings).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn set_paused(engine: State<'_, EngineHandle>, paused: bool) -> Result<()> {
    Ok(engine.set_paused(paused).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn set_profile(engine: State<'_, EngineHandle>, profile: Uuid) -> Result<()> {
    Ok(engine.set_profile(profile).await?)
}

/// Font families the renderer can draw text with.
#[tauri::command]
#[specta::specta]
pub async fn fonts(engine: State<'_, EngineHandle>) -> Result<Vec<String>> {
    Ok(engine.fonts().await?)
}

/// Does what a press, double press or long press of a key does, so it can be tried without the
/// dock.
#[tauri::command]
#[specta::specta]
pub async fn press_key(
    engine: State<'_, EngineHandle>,
    page: Uuid,
    index: u8,
    gesture: Gesture,
) -> Result<()> {
    Ok(engine.press_key(page, index, gesture).await?)
}

/// Goes back to how things were before the last edit.
#[tauri::command]
#[specta::specta]
pub async fn undo(engine: State<'_, EngineHandle>) -> Result<()> {
    Ok(engine.undo().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn redo(engine: State<'_, EngineHandle>) -> Result<()> {
    Ok(engine.redo().await?)
}

/// Gesture timing, the look of new keys and the screensaver.
#[tauri::command]
#[specta::specta]
pub async fn set_preferences(
    engine: State<'_, EngineHandle>,
    plugins: State<'_, Option<Arc<Plugins>>>,
    preferences: Preferences,
) -> Result<()> {
    let language = crate::i18n::current();
    engine.set_preferences(preferences).await?;
    if crate::i18n::current() != language
        && let Some(plugins) = plugins.inner().clone()
    {
        // Plugins name their actions in the user's language.
        let _ = tauri::async_runtime::spawn_blocking(move || plugins.reload()).await;
    }
    Ok(())
}

/// Shows the screensaver on the dock for a minute, or until a key is pressed; `false` stops it.
#[tauri::command]
#[specta::specta]
pub async fn try_screensaver(engine: State<'_, EngineHandle>, on: bool) -> Result<()> {
    Ok(engine.try_screensaver(on).await?)
}
