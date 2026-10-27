//! Tells a press from a double press and a long press, from the key downs and ups of the dock.
//! Keys without those gestures act at once on key down, as they always did.

use std::time::{Duration, Instant};

/// How long a key must be held for a long press, and how soon a second press must follow for a
/// double press. A key with a double press waits that long before doing its press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub long_press: Duration,
    pub double_press: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            long_press: Duration::from_millis(500),
            double_press: Duration::from_millis(300),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Gesture {
    Press,
    DoublePress,
    LongPress,
}

/// Which gestures a key reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reacts {
    pub double: bool,
    pub long: bool,
}

#[derive(Debug, Clone, Copy)]
struct Held {
    index: u8,
    reacts: Reacts,
    since: Instant,
    /// Its gesture already happened, so letting go does nothing.
    done: bool,
}

#[derive(Debug, Clone, Copy)]
struct Waiting {
    index: u8,
    until: Instant,
}

#[derive(Debug, Default)]
pub struct Gestures {
    held: Option<Held>,
    waiting: Option<Waiting>,
    timing: Timing,
}

pub type Fired = Vec<(u8, Gesture)>;

impl Gestures {
    pub fn set_timing(&mut self, timing: Timing) {
        self.timing = timing;
    }

    pub fn down(&mut self, index: u8, reacts: Reacts, now: Instant) -> Fired {
        let mut fired = Vec::new();
        if let Some(waiting) = self.waiting.take() {
            if waiting.index == index && now < waiting.until {
                fired.push((index, Gesture::DoublePress));
                self.held = Some(Held {
                    index,
                    reacts,
                    since: now,
                    done: true,
                });
                return fired;
            }
            fired.push((waiting.index, Gesture::Press));
        }
        if reacts == Reacts::default() {
            fired.push((index, Gesture::Press));
        } else {
            self.held = Some(Held {
                index,
                reacts,
                since: now,
                done: false,
            });
        }
        fired
    }

    pub fn up(&mut self, index: u8, now: Instant) -> Fired {
        let Some(held) = self.held.filter(|held| held.index == index) else {
            return Vec::new();
        };
        self.held = None;
        if held.done {
            Vec::new()
        } else if held.reacts.double {
            self.waiting = Some(Waiting {
                index,
                until: now + self.timing.double_press,
            });
            Vec::new()
        } else {
            vec![(index, Gesture::Press)]
        }
    }

    /// What became due by `now`: a key held long enough, or a press no second one followed.
    pub fn tick(&mut self, now: Instant) -> Fired {
        let mut fired = Vec::new();
        if let Some(held) = &mut self.held
            && held.reacts.long
            && !held.done
            && now >= held.since + self.timing.long_press
        {
            held.done = true;
            fired.push((held.index, Gesture::LongPress));
        }
        if let Some(waiting) = self.waiting
            && now >= waiting.until
        {
            self.waiting = None;
            fired.push((waiting.index, Gesture::Press));
        }
        fired
    }

    /// When `tick` has something to do next.
    pub fn deadline(&self) -> Option<Instant> {
        let long = self
            .held
            .filter(|held| held.reacts.long && !held.done)
            .map(|held| held.since + self.timing.long_press);
        let double = self.waiting.map(|waiting| waiting.until);
        long.into_iter().chain(double).min()
    }

    /// The dock moved to another page: half-made gestures belong to keys no longer there.
    pub fn cancel(&mut self) {
        self.held = None;
        self.waiting = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Reacts = Reacts {
        double: false,
        long: false,
    };
    const BOTH: Reacts = Reacts {
        double: true,
        long: true,
    };

    fn ms(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn plain_keys_act_on_key_down() {
        let mut g = Gestures::default();
        let t = Instant::now();
        assert_eq!(g.down(1, PLAIN, t), vec![(1, Gesture::Press)]);
        assert!(g.up(1, ms(t, 50)).is_empty());
        assert!(g.deadline().is_none());
    }

    #[test]
    fn a_short_press_waits_to_see_whether_a_second_one_comes() {
        let mut g = Gestures::default();
        let t = Instant::now();
        assert!(g.down(2, BOTH, t).is_empty());
        assert!(g.up(2, ms(t, 80)).is_empty());
        assert!(g.tick(ms(t, 200)).is_empty());
        assert_eq!(g.tick(ms(t, 400)), vec![(2, Gesture::Press)]);
    }

    #[test]
    fn two_quick_presses_are_a_double_press() {
        let mut g = Gestures::default();
        let t = Instant::now();
        g.down(2, BOTH, t);
        g.up(2, ms(t, 80));
        assert_eq!(g.down(2, BOTH, ms(t, 200)), vec![(2, Gesture::DoublePress)]);
        assert!(g.up(2, ms(t, 260)).is_empty());
        assert!(g.tick(ms(t, 1000)).is_empty());
    }

    #[test]
    fn holding_is_a_long_press_and_letting_go_does_nothing_more() {
        let mut g = Gestures::default();
        let t = Instant::now();
        g.down(4, BOTH, t);
        assert_eq!(g.deadline(), Some(ms(t, 500)));
        assert_eq!(g.tick(ms(t, 520)), vec![(4, Gesture::LongPress)]);
        assert!(g.up(4, ms(t, 900)).is_empty());
        assert!(g.tick(ms(t, 2000)).is_empty());
    }

    #[test]
    fn without_a_double_press_a_short_press_acts_on_release() {
        let mut g = Gestures::default();
        let t = Instant::now();
        let long_only = Reacts {
            double: false,
            long: true,
        };
        g.down(5, long_only, t);
        assert_eq!(g.up(5, ms(t, 100)), vec![(5, Gesture::Press)]);
    }

    #[test]
    fn follows_the_timing_it_is_given() {
        let mut g = Gestures::default();
        g.set_timing(Timing {
            long_press: Duration::from_millis(900),
            double_press: Duration::from_millis(150),
        });
        let t = Instant::now();
        g.down(4, BOTH, t);
        assert!(g.tick(ms(t, 600)).is_empty(), "not held long enough yet");
        assert_eq!(g.tick(ms(t, 900)), vec![(4, Gesture::LongPress)]);
        g.up(4, ms(t, 950));
        g.down(2, BOTH, ms(t, 1000));
        g.up(2, ms(t, 1050));
        assert_eq!(g.tick(ms(t, 1200)), vec![(2, Gesture::Press)]);
    }

    #[test]
    fn another_key_settles_a_pending_press_first() {
        let mut g = Gestures::default();
        let t = Instant::now();
        g.down(2, BOTH, t);
        g.up(2, ms(t, 50));
        assert_eq!(
            g.down(3, PLAIN, ms(t, 100)),
            vec![(2, Gesture::Press), (3, Gesture::Press)]
        );
    }
}
