//! What the UI needs to know about the system to set up actions.

use crate::actions;

/// Whether the app may press keys and type, which shortcuts and text need: macOS asks for
/// Accessibility, Linux for a udev rule; Windows always lets it.
#[tauri::command]
#[specta::specta]
pub fn accessibility_granted() -> bool {
    actions::accessibility_granted()
}

/// Asks for the permission: on macOS shows the system prompt, which adds the app to the
/// Accessibility list, and opens that list; on Linux asks for the password to install the rule,
/// and returns once it is done.
#[tauri::command]
#[specta::specta]
pub async fn request_accessibility() {
    let _ = tauri::async_runtime::spawn_blocking(|| {
        actions::request_accessibility();
        if let Err(err) = actions::open_accessibility_settings() {
            log::warn!("could not open the Accessibility settings: {err}");
        }
    })
    .await;
}

/// Audio outputs a sound can be sent to.
#[tauri::command]
#[specta::specta]
pub async fn audio_outputs() -> Vec<String> {
    actions::output_devices()
}
