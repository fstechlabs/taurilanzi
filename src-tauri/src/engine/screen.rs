//! What the dock screen shows, the keys, the screensaver or nothing, and when that changes: when
//! the Mac locks and unlocks, after a while without touching the dock, and while trying a
//! screensaver from the settings.

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{LockAction, ScreensaverSettings};

/// A screensaver tried from the settings stops by itself after this long.
pub const TRIAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Locked,
    Idle,
    Trial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Showing {
    Keys,
    Screensaver { reason: Reason, since: Instant },
    Dark { reason: Reason },
}

/// What the engine must do to the dock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Keys,
    Screensaver,
    Dark,
}

/// What a key press means while the dock is not showing the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// The keys are up: the press does what the key does.
    Act,
    /// The press only wakes the dock up.
    Wake(Change),
    Ignore,
}

/// For the UI: what the dock shows now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ScreenMode {
    Keys,
    Screensaver,
    Trial,
    Dark,
}

pub struct Rules {
    on_lock: LockAction,
    idle: Option<Duration>,
    off_after: Option<Duration>,
}

impl From<&ScreensaverSettings> for Rules {
    fn from(settings: &ScreensaverSettings) -> Self {
        let minutes = |m: u32| (m > 0).then(|| Duration::from_secs(u64::from(m) * 60));
        Self {
            on_lock: settings.on_lock,
            idle: minutes(settings.idle_minutes),
            off_after: minutes(settings.off_after_minutes),
        }
    }
}

pub struct Screen {
    showing: Showing,
    locked: bool,
}

impl Default for Screen {
    fn default() -> Self {
        Self {
            showing: Showing::Keys,
            locked: false,
        }
    }
}

impl Screen {
    pub fn showing_keys(&self) -> bool {
        self.showing == Showing::Keys
    }

    pub fn showing_screensaver(&self) -> bool {
        matches!(self.showing, Showing::Screensaver { .. })
    }

    pub fn mode(&self) -> ScreenMode {
        match self.showing {
            Showing::Keys => ScreenMode::Keys,
            Showing::Screensaver {
                reason: Reason::Trial,
                ..
            } => ScreenMode::Trial,
            Showing::Screensaver { .. } => ScreenMode::Screensaver,
            Showing::Dark { .. } => ScreenMode::Dark,
        }
    }

    /// Notes the lock without changing the screen, for a paused dock.
    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }

    pub fn lock(&mut self, rules: &Rules, now: Instant) -> Option<Change> {
        self.locked = true;
        self.reconsider(rules, now)
    }

    pub fn unlock(&mut self) -> Option<Change> {
        self.locked = false;
        match self.showing {
            Showing::Screensaver {
                reason: Reason::Locked,
                ..
            }
            | Showing::Dark {
                reason: Reason::Locked,
            } => self.go(Showing::Keys),
            _ => None,
        }
    }

    /// Puts the screen where the rules want it while the Mac is locked, after they changed.
    pub fn reconsider(&mut self, rules: &Rules, now: Instant) -> Option<Change> {
        if !self.locked {
            return None;
        }
        match (rules.on_lock, self.showing) {
            (_, Showing::Screensaver { reason, .. } | Showing::Dark { reason })
                if reason != Reason::Locked =>
            {
                None
            }
            (LockAction::Keep, _) => self.go(Showing::Keys),
            (LockAction::Screensaver, Showing::Screensaver { .. }) => None,
            (LockAction::Screensaver, _) => self.go(Showing::Screensaver {
                reason: Reason::Locked,
                since: now,
            }),
            (LockAction::Off, _) => self.go(Showing::Dark {
                reason: Reason::Locked,
            }),
        }
    }

    pub fn press(&mut self, rules: &Rules, now: Instant) -> Press {
        match self.showing {
            Showing::Keys => Press::Act,
            Showing::Screensaver {
                reason: Reason::Locked,
                ..
            } => Press::Ignore,
            // Gone dark after a while of screensaver: a press brings it back.
            Showing::Dark {
                reason: Reason::Locked,
            } if rules.on_lock == LockAction::Screensaver => self.wake(Showing::Screensaver {
                reason: Reason::Locked,
                since: now,
            }),
            Showing::Dark {
                reason: Reason::Locked,
            } => Press::Ignore,
            Showing::Screensaver { .. } | Showing::Dark { .. } => self.wake(Showing::Keys),
        }
    }

    /// What became due: the screensaver after a while idle, or the dark after a while of it.
    pub fn tick(&mut self, rules: &Rules, idle: Duration, now: Instant) -> Option<Change> {
        match self.showing {
            Showing::Keys if rules.idle.is_some_and(|limit| idle >= limit) => {
                self.go(Showing::Screensaver {
                    reason: Reason::Idle,
                    since: now,
                })
            }
            Showing::Screensaver {
                reason: Reason::Trial,
                since,
            } if now >= since + TRIAL => self.go(Showing::Keys),
            Showing::Screensaver { reason, since }
                if reason != Reason::Trial
                    && rules.off_after.is_some_and(|after| now >= since + after) =>
            {
                self.go(Showing::Dark { reason })
            }
            _ => None,
        }
    }

    /// When `tick` has something to do next.
    pub fn deadline(&self, rules: &Rules, idle: Duration, now: Instant) -> Option<Instant> {
        match self.showing {
            Showing::Keys => rules.idle.map(|limit| now + limit.saturating_sub(idle)),
            Showing::Screensaver {
                reason: Reason::Trial,
                since,
            } => Some(since + TRIAL),
            Showing::Screensaver { since, .. } => rules.off_after.map(|after| since + after),
            Showing::Dark { .. } => None,
        }
    }

    /// Shows the screensaver on the dock until a key is pressed, the trial is stopped or it times
    /// out.
    pub fn try_it(&mut self, now: Instant) -> Option<Change> {
        if self.locked {
            return None;
        }
        self.showing = Showing::Screensaver {
            reason: Reason::Trial,
            since: now,
        };
        Some(Change::Screensaver)
    }

    pub fn stop_trial(&mut self) -> Option<Change> {
        match self.showing {
            Showing::Screensaver {
                reason: Reason::Trial,
                ..
            } => self.go(Showing::Keys),
            _ => None,
        }
    }

    /// Back to the keys whatever the screen shows, as when a mutating edit wakes an idle dock.
    pub fn wake_from_idle(&mut self) -> Option<Change> {
        match self.showing {
            Showing::Screensaver {
                reason: Reason::Idle,
                ..
            }
            | Showing::Dark {
                reason: Reason::Idle,
            } => self.go(Showing::Keys),
            _ => None,
        }
    }

    /// Back to the keys, as when the dock is paused.
    pub fn reset(&mut self) -> Option<Change> {
        self.go(Showing::Keys)
    }

    fn wake(&mut self, showing: Showing) -> Press {
        self.go(showing).map_or(Press::Ignore, Press::Wake)
    }

    fn go(&mut self, showing: Showing) -> Option<Change> {
        let kind = |s: Showing| match s {
            Showing::Keys => Change::Keys,
            Showing::Screensaver { .. } => Change::Screensaver,
            Showing::Dark { .. } => Change::Dark,
        };
        let changed = kind(self.showing) != kind(showing);
        self.showing = showing;
        changed.then(|| kind(showing))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(on_lock: LockAction, idle: u32, off_after: u32) -> Rules {
        Rules::from(&ScreensaverSettings {
            on_lock,
            idle_minutes: idle,
            off_after_minutes: off_after,
            ..ScreensaverSettings::default()
        })
    }

    fn minutes(m: u64) -> Duration {
        Duration::from_secs(m * 60)
    }

    #[test]
    fn locking_plays_the_screensaver_until_unlocking() {
        let rules = rules(LockAction::Screensaver, 0, 0);
        let mut screen = Screen::default();
        let t = Instant::now();
        assert_eq!(screen.lock(&rules, t), Some(Change::Screensaver));
        assert_eq!(
            screen.press(&rules, t),
            Press::Ignore,
            "presses never run actions"
        );
        assert_eq!(screen.mode(), ScreenMode::Screensaver);
        assert_eq!(screen.unlock(), Some(Change::Keys));
        assert!(screen.showing_keys());
    }

    #[test]
    fn keep_leaves_the_keys_working() {
        let rules = rules(LockAction::Keep, 0, 0);
        let mut screen = Screen::default();
        assert_eq!(screen.lock(&rules, Instant::now()), None);
        assert_eq!(screen.press(&rules, Instant::now()), Press::Act);
    }

    #[test]
    fn the_screensaver_goes_dark_and_a_press_brings_it_back() {
        let rules = rules(LockAction::Screensaver, 0, 30);
        let mut screen = Screen::default();
        let t = Instant::now();
        screen.lock(&rules, t);
        assert_eq!(
            screen.deadline(&rules, Duration::ZERO, t),
            Some(t + minutes(30))
        );
        assert_eq!(screen.tick(&rules, Duration::ZERO, t + minutes(29)), None);
        assert_eq!(
            screen.tick(&rules, Duration::ZERO, t + minutes(30)),
            Some(Change::Dark)
        );
        assert_eq!(
            screen.press(&rules, t + minutes(31)),
            Press::Wake(Change::Screensaver)
        );
    }

    #[test]
    fn off_stays_dark_when_pressed() {
        let rules = rules(LockAction::Off, 0, 0);
        let mut screen = Screen::default();
        assert_eq!(screen.lock(&rules, Instant::now()), Some(Change::Dark));
        assert_eq!(screen.press(&rules, Instant::now()), Press::Ignore);
        assert_eq!(screen.unlock(), Some(Change::Keys));
    }

    #[test]
    fn an_idle_dock_starts_the_screensaver_and_a_press_wakes_it() {
        let rules = rules(LockAction::Keep, 5, 0);
        let mut screen = Screen::default();
        let t = Instant::now();
        assert_eq!(screen.deadline(&rules, minutes(2), t), Some(t + minutes(3)));
        assert_eq!(screen.tick(&rules, minutes(4), t), None);
        assert_eq!(
            screen.tick(&rules, minutes(5), t),
            Some(Change::Screensaver)
        );
        assert_eq!(screen.press(&rules, t), Press::Wake(Change::Keys));
        assert!(screen.showing_keys());
    }

    #[test]
    fn unlocking_leaves_an_idle_screensaver_alone() {
        let rules = rules(LockAction::Keep, 5, 0);
        let mut screen = Screen::default();
        let t = Instant::now();
        screen.tick(&rules, minutes(5), t);
        screen.lock(&rules, t);
        assert_eq!(screen.unlock(), None);
        assert_eq!(screen.mode(), ScreenMode::Screensaver);
    }

    #[test]
    fn changing_the_rules_while_locked_moves_the_screen() {
        let mut screen = Screen::default();
        let t = Instant::now();
        screen.lock(&rules(LockAction::Screensaver, 0, 0), t);
        assert_eq!(
            screen.reconsider(&rules(LockAction::Off, 0, 0), t),
            Some(Change::Dark)
        );
        assert_eq!(
            screen.reconsider(&rules(LockAction::Keep, 0, 0), t),
            Some(Change::Keys)
        );
    }

    #[test]
    fn a_trial_stops_on_a_press_or_by_itself() {
        let rules = rules(LockAction::Screensaver, 0, 1);
        let mut screen = Screen::default();
        let t = Instant::now();
        assert_eq!(screen.try_it(t), Some(Change::Screensaver));
        assert_eq!(screen.mode(), ScreenMode::Trial);
        assert_eq!(
            screen.tick(&rules, Duration::ZERO, t + minutes(1)),
            Some(Change::Keys)
        );
        screen.try_it(t);
        assert_eq!(screen.press(&rules, t), Press::Wake(Change::Keys));
        assert_eq!(screen.stop_trial(), None);
    }
}
