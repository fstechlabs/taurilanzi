//! The tick or cross a key shows for a moment after a request, so a press that changes nothing
//! on screen still says whether it worked.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use log::warn;
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
        self.redraw_key(slot.page, slot.index);
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
            self.redraw_key(page, index);
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

    fn redraw_key(&mut self, page: Uuid, index: u8) {
        if self.paused
            || !self.screen.showing_keys()
            || self.current_page().map(|p| p.id) != Some(page)
        {
            return;
        }
        let Some(design) = self.design_for(index) else {
            return;
        };
        match self.draw(index, &design) {
            Ok(png) => self.dock.set_keys(BTreeMap::from([(index, png)])),
            Err(err) => warn!("key {index} could not be drawn: {err}"),
        }
    }
}
