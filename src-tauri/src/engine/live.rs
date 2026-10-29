//! Keeps the live widgets of the page on the dock up to date. Each widget changes on a boundary
//! of the wall clock (every second, every two or every minute), so the keys holding them are
//! redrawn just after it, all in one update. Nothing is sent while the dock is paused or shows
//! the screensaver or nothing.

use std::collections::BTreeMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use log::warn;
use uuid::Uuid;

use super::runtime::Engine;
use crate::config::{Appearance, Key, LayerConfig, Widget};
use crate::hass::domain;

/// Lands just after the boundary, so the new second is already there when drawing.
const LATE: Duration = Duration::from_millis(3);

#[derive(Default)]
pub struct Live {
    /// When the live keys were last brought up to date.
    last: Option<SystemTime>,
}

/// Seconds between changes of what a widget shows.
fn period(widget: &Widget) -> u64 {
    match widget {
        Widget::DigitalClock { seconds, .. } | Widget::AnalogClock { seconds, .. } => {
            if *seconds {
                1
            } else {
                60
            }
        }
        Widget::SystemMonitor { .. } | Widget::Network { .. } => 2,
        Widget::Countdown { .. } | Widget::Timer { .. } | Widget::NowPlaying { .. } => 1,
        Widget::WorldClocks { .. } | Widget::Calendar { .. } => 60,
        Widget::Battery { .. } => 20,
        Widget::Stopwatch { .. } | Widget::Volume { .. } => 1,
        Widget::TopProcesses { .. } | Widget::Latency { .. } => 2,
        Widget::Temperatures { .. } => 5,
        Widget::Moon { .. } | Widget::Progress { .. } | Widget::WordClock { .. } => 60,
        // Changes only when pressed.
        Widget::Counter { .. } => 3600,
        Widget::Phrases { seconds, .. } => match seconds {
            0 => 3600,
            seconds => u64::from(*seconds),
        },
        // Drawn again when Home Assistant reports a change; timers and players also tick.
        Widget::HassEntity { .. } => 1,
        Widget::HassTile { entity, .. } => match domain(entity) {
            "timer" | "media_player" => 1,
            _ => 60,
        },
        Widget::HassGraph { .. } | Widget::HassEntities { .. } => 60,
        Widget::HassPicture { .. } | Widget::HassWeather { .. } | Widget::HassTemplate { .. } => {
            3600
        }
    }
}

/// The shortest period of the widgets in an appearance, if it has any.
fn live_period(appearance: &Appearance) -> Option<u64> {
    appearance
        .layers
        .iter()
        .filter_map(|layer| match layer {
            LayerConfig::Widget { widget } => Some(period(widget)),
            _ => None,
        })
        .min()
}

fn slot(time: SystemTime, period: u64) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() / period)
}

fn until_next(time: SystemTime, period: u64) -> Duration {
    let millis = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64);
    let period = period * 1000;
    Duration::from_millis(period - millis % period)
}

impl Engine {
    /// Graphs show the past, so while any key has one the system is sampled all the time.
    pub(super) fn update_sampling(&self) {
        let graphs = self
            .config
            .profiles
            .iter()
            .flat_map(|profile| &profile.pages)
            .flat_map(|page| page.keys.values())
            .flat_map(Key::appearances)
            .flat_map(|appearance| &appearance.layers)
            .any(|layer| {
                matches!(
                    layer,
                    LayerConfig::Widget {
                        widget: Widget::SystemMonitor { graph: true, .. }
                            | Widget::Network { graph: true, .. }
                    }
                )
            });
        self.monitor.keep_sampling(graphs);
    }

    /// Keys of the page on the dock with live content, and how often each changes.
    fn live_keys(&self) -> Vec<(u8, u64)> {
        if self.paused || !self.screen.showing_keys() {
            return Vec::new();
        }
        let Some(page) = self.current_page() else {
            return Vec::new();
        };
        page.keys
            .iter()
            .filter_map(|(&index, key)| {
                live_period(self.appearance_now(page.id, index, key)).map(|p| (index, p))
            })
            .collect()
    }

    pub(super) fn appearance_now<'a>(&self, page: Uuid, index: u8, key: &'a Key) -> &'a Appearance {
        key.second_state
            .as_ref()
            .filter(|_| self.second_states.contains(&(page, index)))
            .map_or(&key.appearance, |state| &state.appearance)
    }

    pub(super) fn live_deadline(&self) -> Option<Instant> {
        let now = SystemTime::now();
        self.live_keys()
            .into_iter()
            .map(|(_, period)| until_next(now, period))
            .min()
            .map(|wait| Instant::now() + wait + LATE)
    }

    /// Redraws the live keys whose content changed since the last time.
    pub(super) fn tick_live(&mut self) {
        let keys = self.live_keys();
        if keys.is_empty() {
            return;
        }
        let now = SystemTime::now();
        let Some(last) = self.live.last.replace(now) else {
            return;
        };
        let mut images = BTreeMap::new();
        for (index, period) in keys {
            if slot(last, period) == slot(now, period) {
                continue;
            }
            let Some(design) = self.design_for(index) else {
                continue;
            };
            match self.draw(index, &design) {
                Ok(png) => {
                    images.insert(index, png);
                }
                Err(err) => warn!("key {index} could not be drawn: {err}"),
            }
        }
        if !images.is_empty() {
            self.dock.set_keys(images);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::render::{Background, Color};

    fn widget(value: serde_json::Value) -> Widget {
        let mut value = value;
        value["style"] = json!({
            "color": { "r": 255, "g": 255, "b": 255 },
            "accent": { "r": 0, "g": 0, "b": 0 },
            "font_family": "Inter",
        });
        serde_json::from_value(value).unwrap()
    }

    fn clock(seconds: bool) -> Widget {
        widget(json!({
            "kind": "digital_clock", "seconds": seconds, "hour12": false,
            "second_line": "none", "zone": null, "label": "",
        }))
    }

    fn at(millis: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(millis)
    }

    #[test]
    fn each_widget_changes_on_its_own_period() {
        assert_eq!(period(&clock(true)), 1);
        assert_eq!(period(&clock(false)), 60);
        let analog = widget(json!({
            "kind": "analog_clock", "seconds": false, "date": true, "zone": null, "label": "",
        }));
        assert_eq!(period(&analog), 60);
        assert_eq!(
            period(&widget(
                json!({ "kind": "system_monitor", "metrics": ["cpu"] })
            )),
            2
        );
        assert_eq!(period(&widget(json!({ "kind": "network" }))), 2);
        let countdown = widget(json!({
            "kind": "countdown", "target": "2026-12-31T23:59", "label": "", "done": "",
        }));
        assert_eq!(period(&countdown), 1);
    }

    #[test]
    fn tiles_tick_only_for_what_counts_down_or_plays() {
        let tile = |entity: &str| widget(json!({ "kind": "hass_tile", "entity": entity }));
        assert_eq!(period(&tile("timer.tea")), 1);
        assert_eq!(period(&tile("media_player.tv")), 1);
        assert_eq!(period(&tile("cover.salon")), 60);
    }

    #[test]
    fn the_fastest_widget_of_a_key_sets_its_period() {
        let mut appearance = Appearance {
            background: Background::Solid {
                color: Color::BLACK,
            },
            layers: Vec::new(),
        };
        assert_eq!(live_period(&appearance), None);
        appearance.layers.push(LayerConfig::Widget {
            widget: clock(false),
        });
        assert_eq!(live_period(&appearance), Some(60));
        appearance.layers.push(LayerConfig::Widget {
            widget: widget(json!({ "kind": "network" })),
        });
        assert_eq!(live_period(&appearance), Some(2));
    }

    #[test]
    fn slots_count_whole_periods_since_the_epoch() {
        assert_eq!(slot(at(125_250), 60), 2);
        assert_eq!(slot(at(125_250), 1), 125);
        assert_eq!(slot(at(119_999), 60), 1);
        assert_eq!(slot(at(120_000), 60), 2);
        assert_eq!(slot(UNIX_EPOCH - Duration::from_secs(5), 1), 0);
    }

    #[test]
    fn waits_until_the_next_boundary() {
        assert_eq!(until_next(at(125_250), 60), Duration::from_millis(54_750));
        assert_eq!(until_next(at(125_250), 2), Duration::from_millis(750));
        assert_eq!(until_next(at(125_250), 1), Duration::from_millis(750));
        assert_eq!(until_next(at(126_000), 2), Duration::from_secs(2));
    }
}
