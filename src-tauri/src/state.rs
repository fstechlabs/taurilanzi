use std::sync::{Arc, Mutex};

use crate::engine::Snapshot;
use crate::login::Placement;
use crate::tray::Model;
use crate::updates::Updates;

/// Shared by the menu bar, the update checks and the window handlers.
pub struct AppState {
    pub placement: Placement,
    pub updates: Updates,
    /// The engine's latest state, as the menu bar needs it.
    pub snapshot: Mutex<Option<Arc<Snapshot>>>,
    /// What the menu bar currently shows; its menu is rebuilt only when this changes.
    pub menu: Mutex<Option<Model>>,
}

impl AppState {
    pub fn new(placement: Placement, updates: Updates) -> Self {
        Self {
            placement,
            updates,
            snapshot: Mutex::default(),
            menu: Mutex::default(),
        }
    }

    pub fn snapshot(&self) -> Option<Arc<Snapshot>> {
        self.snapshot.lock().expect("snapshot").clone()
    }
}
