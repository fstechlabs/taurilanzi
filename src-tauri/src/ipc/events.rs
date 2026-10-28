use std::sync::Arc;

use serde::Serialize;
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::sync::broadcast::error::RecvError;

use crate::cloud::CloudStatus;
use crate::engine::{EngineEvent, EngineHandle, Snapshot};
use crate::updates::UpdateOffer;

/// The whole state, after anything in it changed.
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct StateChanged(pub Arc<Snapshot>);

/// ⌘Z or ⌘⇧Z from the menu. The UI decides whether it undoes text being typed or an edit.
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct HistoryShortcut {
    pub redo: bool,
}

/// A plugin wants to tell the user something.
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct PluginToast {
    pub plugin: String,
    pub message: String,
}

/// The update the window offers, after any change in it; none when there is nothing to offer.
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct UpdateOffered(pub Option<UpdateOffer>);

/// How this computer is doing with the cloud, after any change.
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct CloudChanged(pub CloudStatus);

/// A plugin redrew a key: its image is to be loaded again.
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct KeyImageChanged {
    pub page: uuid::Uuid,
    pub index: u8,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct KeyChanged {
    pub index: u8,
    pub pressed: bool,
}

/// Passes engine events on to the UI for as long as the app runs.
pub fn forward(app: AppHandle, engine: &EngineHandle) {
    let mut events = engine.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            let sent = match events.recv().await {
                Ok(EngineEvent::State(snapshot)) => StateChanged(snapshot).emit(&app),
                Ok(EngineEvent::Key { index, pressed }) => KeyChanged { index, pressed }.emit(&app),
                Ok(EngineEvent::KeyImage { page, index }) => {
                    KeyImageChanged { page, index }.emit(&app)
                }
                // The next state event carries everything anyway.
                Err(RecvError::Lagged(_)) => Ok(()),
                Err(RecvError::Closed) => break,
            };
            if let Err(err) = sent {
                log::warn!("could not send an event to the UI: {err}");
            }
        }
    });
}
