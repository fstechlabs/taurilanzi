//! Widgets that answer to their key: timers and stopwatches start, pause and start over,
//! counters add up and phrases move on. Timers, stopwatches and phrases live while the app runs,
//! like the state of keys of two states, and so does which of the widgets of a key shows; counters
//! are kept in a file of their own.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Run {
    Running { ends: Instant },
    Paused { left: Duration },
}

/// Where a timer is now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerNow {
    /// Not started, or started over.
    Idle,
    Running(Duration),
    Paused(Duration),
    Done,
}

type Slot = (Uuid, u8);

/// Where a stopwatch is now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StopwatchNow {
    pub elapsed: Duration,
    pub running: bool,
}

/// What the widget of a key needs from its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyNow {
    pub timer: TimerNow,
    pub stopwatch: StopwatchNow,
    pub count: i64,
    /// Presses on a phrases widget, each moving one phrase on.
    pub presses: u32,
    /// Presses on a key of several widgets, each moving on to the next one.
    pub turn: u32,
}

impl Default for KeyNow {
    fn default() -> Self {
        Self {
            timer: TimerNow::Idle,
            stopwatch: StopwatchNow::default(),
            count: 0,
            presses: 0,
            turn: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Watch {
    Running { since: Instant, before: Duration },
    Paused { elapsed: Duration },
}

#[derive(Debug, Default)]
pub struct KeyWidgets {
    runs: HashMap<Slot, Run>,
    watches: HashMap<Slot, Watch>,
    presses: HashMap<Slot, u32>,
    turns: HashMap<Slot, u32>,
    /// By `page:index`, as they are saved.
    counters: BTreeMap<String, i64>,
}

/// How counters are saved: by page and key.
pub fn counter_key((page, index): Slot) -> String {
    format!("{page}:{index}")
}

impl KeyWidgets {
    pub fn with_counters(counters: BTreeMap<String, i64>) -> Self {
        Self {
            counters,
            ..Self::default()
        }
    }

    pub fn counters(&self) -> &BTreeMap<String, i64> {
        &self.counters
    }

    pub fn key_now(&self, slot: Slot, now: Instant) -> KeyNow {
        KeyNow {
            timer: self.now(slot, now),
            stopwatch: self.stopwatch(slot, now),
            count: self.counters.get(&counter_key(slot)).copied().unwrap_or(0),
            presses: self.presses.get(&slot).copied().unwrap_or(0),
            turn: self.turn(slot),
        }
    }

    pub fn turn(&self, slot: Slot) -> u32 {
        self.turns.get(&slot).copied().unwrap_or(0)
    }

    pub fn next_turn(&mut self, slot: Slot) {
        let turn = self.turns.entry(slot).or_insert(0);
        *turn = turn.wrapping_add(1);
    }

    fn stopwatch(&self, slot: Slot, now: Instant) -> StopwatchNow {
        match self.watches.get(&slot) {
            None => StopwatchNow::default(),
            Some(Watch::Paused { elapsed }) => StopwatchNow {
                elapsed: *elapsed,
                running: false,
            },
            Some(Watch::Running { since, before }) => StopwatchNow {
                elapsed: *before + now.saturating_duration_since(*since),
                running: true,
            },
        }
    }

    /// Starts or pauses a stopwatch.
    pub fn press_stopwatch(&mut self, slot: Slot, now: Instant) {
        let watch = match self.stopwatch(slot, now) {
            StopwatchNow {
                running: true,
                elapsed,
            } => Watch::Paused { elapsed },
            StopwatchNow { elapsed, .. } => Watch::Running {
                since: now,
                before: elapsed,
            },
        };
        self.watches.insert(slot, watch);
    }

    pub fn reset_stopwatch(&mut self, slot: Slot) {
        self.watches.remove(&slot);
    }

    /// Adds `step` to a counter; true if it changed.
    pub fn count(&mut self, slot: Slot, step: i32) -> bool {
        let count = self.counters.entry(counter_key(slot)).or_insert(0);
        let next = count.saturating_add(i64::from(step));
        let changed = next != *count;
        *count = next;
        changed
    }

    pub fn reset_counter(&mut self, slot: Slot) -> bool {
        self.counters
            .remove(&counter_key(slot))
            .is_some_and(|c| c != 0)
    }

    pub fn press_phrases(&mut self, slot: Slot) {
        let presses = self.presses.entry(slot).or_insert(0);
        *presses = presses.wrapping_add(1);
    }

    pub fn now(&self, slot: Slot, now: Instant) -> TimerNow {
        match self.runs.get(&slot) {
            None => TimerNow::Idle,
            Some(Run::Paused { left }) => TimerNow::Paused(*left),
            Some(Run::Running { ends }) if *ends <= now => TimerNow::Done,
            Some(Run::Running { ends }) => TimerNow::Running(*ends - now),
        }
    }

    /// Starts, pauses or resumes; a finished timer goes back to its start.
    pub fn press(&mut self, slot: Slot, length: Duration, now: Instant) {
        let next = match self.now(slot, now) {
            TimerNow::Idle => Some(Run::Running { ends: now + length }),
            TimerNow::Running(left) => Some(Run::Paused { left }),
            TimerNow::Paused(left) => Some(Run::Running { ends: now + left }),
            TimerNow::Done => None,
        };
        match next {
            Some(run) => self.runs.insert(slot, run),
            None => self.runs.remove(&slot),
        };
    }

    pub fn reset(&mut self, slot: Slot) {
        self.runs.remove(&slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_starts_pauses_and_resumes_and_a_finished_one_starts_over() {
        let mut timers = KeyWidgets::default();
        let slot = (Uuid::new_v4(), 3);
        let start = Instant::now();
        let length = Duration::from_secs(60);
        assert_eq!(timers.now(slot, start), TimerNow::Idle);

        timers.press(slot, length, start);
        let later = start + Duration::from_secs(10);
        assert_eq!(
            timers.now(slot, later),
            TimerNow::Running(Duration::from_secs(50))
        );

        timers.press(slot, length, later);
        let much_later = later + Duration::from_secs(100);
        assert_eq!(
            timers.now(slot, much_later),
            TimerNow::Paused(Duration::from_secs(50)),
            "paused, it waits"
        );

        timers.press(slot, length, much_later);
        let end = much_later + Duration::from_secs(50);
        assert_eq!(timers.now(slot, end), TimerNow::Done);

        timers.press(slot, length, end);
        assert_eq!(timers.now(slot, end), TimerNow::Idle);
    }

    #[test]
    fn a_stopwatch_counts_up_pauses_and_starts_over() {
        let mut keys = KeyWidgets::default();
        let slot = (Uuid::new_v4(), 1);
        let start = Instant::now();
        keys.press_stopwatch(slot, start);
        let later = start + Duration::from_secs(5);
        assert_eq!(
            keys.key_now(slot, later).stopwatch,
            StopwatchNow {
                elapsed: Duration::from_secs(5),
                running: true
            }
        );
        keys.press_stopwatch(slot, later);
        let much_later = later + Duration::from_secs(60);
        assert_eq!(
            keys.key_now(slot, much_later).stopwatch.elapsed,
            Duration::from_secs(5)
        );
        keys.press_stopwatch(slot, much_later);
        assert_eq!(
            keys.key_now(slot, much_later + Duration::from_secs(1))
                .stopwatch
                .elapsed,
            Duration::from_secs(6)
        );
        keys.reset_stopwatch(slot);
        assert_eq!(
            keys.key_now(slot, much_later).stopwatch,
            StopwatchNow::default()
        );
    }

    #[test]
    fn counters_add_up_and_are_kept_by_page_and_key() {
        let page = Uuid::new_v4();
        let mut keys = KeyWidgets::default();
        let now = Instant::now();
        assert!(keys.count((page, 2), 1));
        assert!(keys.count((page, 2), 5));
        assert_eq!(keys.key_now((page, 2), now).count, 6);
        assert_eq!(keys.counters().get(&format!("{page}:2")), Some(&6));
        let kept = KeyWidgets::with_counters(keys.counters().clone());
        assert_eq!(kept.key_now((page, 2), now).count, 6);
        assert!(!keys.count((page, 2), 0));
        assert!(keys.reset_counter((page, 2)));
        assert_eq!(keys.key_now((page, 2), now).count, 0);
        assert!(!keys.reset_counter((page, 2)), "already at zero");
        keys.press_phrases((page, 3));
        keys.press_phrases((page, 3));
        assert_eq!(keys.key_now((page, 3), now).presses, 2);
    }

    #[test]
    fn holding_starts_it_over() {
        let mut timers = KeyWidgets::default();
        let slot = (Uuid::new_v4(), 0);
        let now = Instant::now();
        timers.press(slot, Duration::from_secs(5), now);
        timers.reset(slot);
        assert_eq!(timers.now(slot, now), TimerNow::Idle);
        assert_eq!(
            timers.now((Uuid::new_v4(), 0), now),
            TimerNow::Idle,
            "each key has its own"
        );
    }

    #[test]
    fn each_key_moves_through_its_widgets_on_its_own() {
        let page = Uuid::new_v4();
        let mut keys = KeyWidgets::default();
        let now = Instant::now();
        assert_eq!(keys.key_now((page, 13), now).turn, 0);
        keys.next_turn((page, 13));
        keys.next_turn((page, 13));
        assert_eq!(keys.turn((page, 13)), 2);
        assert_eq!(keys.key_now((page, 13), now).turn, 2);
        assert_eq!(keys.turn((page, 0)), 0);
        assert_eq!(keys.turn((Uuid::new_v4(), 13)), 0);
    }
}
