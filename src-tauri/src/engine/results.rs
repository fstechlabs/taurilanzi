//! The tick or cross a key shows for a moment after a request, so a press that changes nothing
//! on screen still says whether it worked.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use uuid::Uuid;

use super::commands::KeySlot;
use super::runtime::Engine;

const SHOWN_FOR: Duration = Duration::from_millis(1500);

/// Until when each key shows how its last request went.
#[derive(Default)]
pub struct Results(HashMap<(Uuid, u8), (bool, Instant)>);

impl Engine {
    pub(super) fn on_key_result(&mut self, slot: KeySlot, ok: bool) {
        self.results
            .0
            .insert((slot.page, slot.index), (ok, Instant::now() + SHOWN_FOR));
        self.redraw_on_dock(slot.page, slot.index);
    }

    pub(super) fn result_deadline(&self) -> Option<Instant> {
        self.results.0.values().map(|&(_, until)| until).min()
    }

    pub(super) fn tick_results(&mut self, now: Instant) {
        let expired: Vec<(Uuid, u8)> = self
            .results
            .0
            .iter()
            .filter(|(_, (_, until))| *until <= now)
            .map(|(&slot, _)| slot)
            .collect();
        for (page, index) in expired {
            self.results.0.remove(&(page, index));
            self.redraw_on_dock(page, index);
        }
    }

    /// Whether the last request of a key worked, while it is still shown.
    pub(super) fn result_of(&self, page: Uuid, index: u8) -> Option<bool> {
        self.results
            .0
            .get(&(page, index))
            .filter(|(_, until)| *until > Instant::now())
            .map(|&(ok, _)| ok)
    }
}
