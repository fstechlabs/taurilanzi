//! The offer to install a downloaded update, shown in a corner of the window.

use tauri::{AppHandle, State};

use crate::state::AppState;
use crate::updates::{self, UpdateOffer};

#[tauri::command]
#[specta::specta]
pub fn update_offer(state: State<'_, AppState>) -> Option<UpdateOffer> {
    state.updates.offer()
}

#[tauri::command]
#[specta::specta]
pub fn install_update(app: AppHandle) {
    updates::install(&app);
}

#[tauri::command]
#[specta::specta]
pub fn remind_update_later(app: AppHandle) {
    updates::remind_later(&app);
}
