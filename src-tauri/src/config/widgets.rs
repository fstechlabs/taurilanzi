use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::render::Color;

/// Live content drawn on a key and redrawn while it is on the dock. Any key can hold one; the
/// editor offers it on the wide key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Widget {
    DigitalClock {
        seconds: bool,
        hour12: bool,
        second_line: ClockLine,
        /// IANA name, such as `Asia/Tokyo`; none is the Mac's own time zone.
        zone: Option<String>,
        /// Shown above the time, to tell world clocks apart. Empty shows nothing.
        label: String,
        style: WidgetStyle,
    },
    AnalogClock {
        seconds: bool,
        /// Weekday and date next to the dial, on a key wide enough.
        date: bool,
        zone: Option<String>,
        label: String,
        style: WidgetStyle,
    },
    SystemMonitor {
        /// One to four, in this order.
        metrics: Vec<Metric>,
        /// The history of each, instead of a bar with the level now.
        #[serde(default)]
        graph: bool,
        /// How far back the graph goes.
        #[serde(default = "graph_minutes")]
        minutes: u32,
        style: WidgetStyle,
    },
    Network {
        /// The history of both speeds, next to the speeds now.
        #[serde(default)]
        graph: bool,
        #[serde(default = "graph_minutes")]
        minutes: u32,
        style: WidgetStyle,
    },
    Countdown {
        /// Local date and time, as `2026-12-31T23:59`.
        target: String,
        label: String,
        /// Shown once the time arrives.
        done: String,
        style: WidgetStyle,
    },
    /// The time in several places at once, side by side on the wide key.
    WorldClocks {
        /// One to three; square keys show the first two.
        clocks: Vec<WorldClock>,
        hour12: bool,
        style: WidgetStyle,
    },
    /// Today, and on the wide key the whole month.
    Calendar { style: WidgetStyle },
    /// Counts down from `seconds` once the key is pressed; pressing again pauses it, and holding
    /// the key down starts it over.
    Timer {
        seconds: u32,
        label: String,
        /// Shown once it reaches zero.
        done: String,
        style: WidgetStyle,
    },
    /// The Mac's battery: how full, whether it charges and how long it lasts.
    Battery { style: WidgetStyle },
    /// The song playing in Music or Spotify.
    NowPlaying { style: WidgetStyle },
    /// How hot the processor, the disk and the battery run.
    Temperatures { style: WidgetStyle },
    /// The apps using the most processor or memory.
    TopProcesses { by: Usage, style: WidgetStyle },
    /// How long another machine takes to answer, with its recent history.
    Latency {
        /// A name or an address; empty is the router.
        host: String,
        style: WidgetStyle,
    },
    /// The volume of the speakers and the microphone.
    Volume { style: WidgetStyle },
    /// The phase of the moon tonight.
    Moon { style: WidgetStyle },
    /// How much of the day, week, month or year has gone.
    Progress {
        /// One to four, in this order.
        periods: Vec<Period>,
        style: WidgetStyle,
    },
    /// Counts up once the key is pressed; pressing again pauses it, holding it starts over.
    Stopwatch { label: String, style: WidgetStyle },
    /// Adds `step` on every press; holding the key down starts over. Kept when the app closes.
    Counter {
        label: String,
        step: i32,
        style: WidgetStyle,
    },
    /// The time in words: "Son las tres y cuarto".
    WordClock { style: WidgetStyle },
    /// Texts of the user's, one after another every `seconds`, or on each press with 0.
    Phrases {
        phrases: Vec<String>,
        seconds: u32,
        style: WidgetStyle,
    },
    /// An entity of Home Assistant as Home Assistant shows it: a sensor's value, whether a light
    /// is on, a climate's temperature, what a media player plays…
    HassEntity {
        #[serde(default)]
        server: Uuid,
        entity: String,
        /// One of its attributes instead of its state.
        #[serde(default)]
        attribute: Option<String>,
        /// Empty is the entity's name.
        #[serde(default)]
        label: String,
        /// An image of the asset store chosen by the user, over the ones of Home Assistant.
        #[serde(default)]
        icon: Option<String>,
        /// Home Assistant's own icons for the entity, as images of the asset store, by state:
        /// `""` for any other state and `#20` for values from 20 up, as a battery's.
        #[serde(default)]
        icons: BTreeMap<String, String>,
        style: WidgetStyle,
    },
    /// How a number of Home Assistant went in the last hours.
    HassGraph {
        #[serde(default)]
        server: Uuid,
        entity: String,
        #[serde(default = "graph_hours")]
        hours: u32,
        #[serde(default)]
        label: String,
        style: WidgetStyle,
    },
    /// Several entities of Home Assistant side by side.
    HassEntities {
        #[serde(default)]
        server: Uuid,
        /// One to four; square keys show the first two.
        entities: Vec<HassItem>,
        style: WidgetStyle,
    },
    /// What a camera sees, an image entity, what a media player plays or a person's picture.
    HassPicture {
        #[serde(default)]
        server: Uuid,
        entity: String,
        /// How often a camera's picture is taken again.
        #[serde(default = "picture_seconds")]
        seconds: u32,
        style: WidgetStyle,
    },
    /// The weather now and, on a wide key, the next days.
    HassWeather {
        #[serde(default)]
        server: Uuid,
        entity: String,
        #[serde(default = "yes")]
        forecast: bool,
        style: WidgetStyle,
    },
    /// A template of Home Assistant, worked out by it whenever what it reads changes.
    HassTemplate {
        #[serde(default)]
        server: Uuid,
        template: String,
        #[serde(default)]
        label: String,
        style: WidgetStyle,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct HassItem {
    pub entity: String,
    /// Empty is the entity's name.
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Usage {
    Cpu,
    Memory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct WorldClock {
    /// IANA name, such as `Asia/Tokyo`.
    pub zone: String,
    /// Empty shows the city of the zone.
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct WidgetStyle {
    pub color: Color,
    /// Seconds, labels, bars and hands that stand out.
    pub accent: Color,
    pub font_family: String,
}

/// What a digital clock shows under the time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ClockLine {
    None,
    Date,
    Weekday,
    WeekdayDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    Cpu,
    Ram,
    Gpu,
    /// How full the startup volume is.
    Disk,
}

fn graph_minutes() -> u32 {
    5
}

fn graph_hours() -> u32 {
    24
}

fn picture_seconds() -> u32 {
    10
}

fn yes() -> bool {
    true
}

impl Widget {
    pub fn style(&self) -> &WidgetStyle {
        match self {
            Self::DigitalClock { style, .. }
            | Self::AnalogClock { style, .. }
            | Self::SystemMonitor { style, .. }
            | Self::Network { style, .. }
            | Self::Countdown { style, .. }
            | Self::WorldClocks { style, .. }
            | Self::Calendar { style }
            | Self::Timer { style, .. }
            | Self::Battery { style }
            | Self::NowPlaying { style }
            | Self::Temperatures { style }
            | Self::TopProcesses { style, .. }
            | Self::Latency { style, .. }
            | Self::Volume { style }
            | Self::Moon { style }
            | Self::Progress { style, .. }
            | Self::Stopwatch { style, .. }
            | Self::Counter { style, .. }
            | Self::WordClock { style }
            | Self::Phrases { style, .. }
            | Self::HassEntity { style, .. }
            | Self::HassGraph { style, .. }
            | Self::HassEntities { style, .. }
            | Self::HassPicture { style, .. }
            | Self::HassWeather { style, .. }
            | Self::HassTemplate { style, .. } => style,
        }
    }

    /// The server of Home Assistant it shows something of.
    pub fn hass_server(&self) -> Option<Uuid> {
        match self {
            Self::HassEntity { server, .. }
            | Self::HassGraph { server, .. }
            | Self::HassEntities { server, .. }
            | Self::HassPicture { server, .. }
            | Self::HassWeather { server, .. }
            | Self::HassTemplate { server, .. } => Some(*server),
            _ => None,
        }
    }

    /// Images of the asset store it draws.
    pub fn assets(&self) -> Vec<&str> {
        match self {
            Self::HassEntity { icon, icons, .. } => icon
                .iter()
                .chain(icons.values())
                .map(String::as_str)
                .collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> WidgetStyle {
        WidgetStyle {
            color: Color::WHITE,
            accent: Color::rgb(255, 102, 0),
            font_family: "Inter".into(),
        }
    }

    #[test]
    fn every_kind_round_trips_through_json() {
        let widgets = [
            Widget::DigitalClock {
                seconds: true,
                hour12: true,
                second_line: ClockLine::WeekdayDate,
                zone: Some("Asia/Tokyo".into()),
                label: "Tokio".into(),
                style: style(),
            },
            Widget::AnalogClock {
                seconds: false,
                date: true,
                zone: None,
                label: String::new(),
                style: style(),
            },
            Widget::SystemMonitor {
                metrics: vec![Metric::Cpu, Metric::Ram, Metric::Gpu, Metric::Disk],
                graph: true,
                minutes: 60,
                style: style(),
            },
            Widget::Network {
                graph: true,
                minutes: 15,
                style: style(),
            },
            Widget::Countdown {
                target: "2026-12-31T23:59".into(),
                label: "Año nuevo".into(),
                done: "¡Feliz año!".into(),
                style: style(),
            },
            Widget::WorldClocks {
                clocks: vec![WorldClock {
                    zone: "America/New_York".into(),
                    label: "NY".into(),
                }],
                hour12: false,
                style: style(),
            },
            Widget::Calendar { style: style() },
            Widget::Timer {
                seconds: 25 * 60,
                label: "Pomodoro".into(),
                done: "¡Descanso!".into(),
                style: style(),
            },
            Widget::Battery { style: style() },
            Widget::NowPlaying { style: style() },
            Widget::Temperatures { style: style() },
            Widget::TopProcesses {
                by: Usage::Memory,
                style: style(),
            },
            Widget::Latency {
                host: "1.1.1.1".into(),
                style: style(),
            },
            Widget::Volume { style: style() },
            Widget::Moon { style: style() },
            Widget::Progress {
                periods: vec![Period::Day, Period::Year],
                style: style(),
            },
            Widget::Stopwatch {
                label: "Reunión".into(),
                style: style(),
            },
            Widget::Counter {
                label: "Cafés".into(),
                step: 1,
                style: style(),
            },
            Widget::WordClock { style: style() },
            Widget::Phrases {
                phrases: vec!["Hola".into(), "Adiós".into()],
                seconds: 10,
                style: style(),
            },
        ];
        for widget in widgets {
            let json = serde_json::to_string(&widget).unwrap();
            assert_eq!(
                serde_json::from_str::<Widget>(&json).unwrap(),
                widget,
                "{json}"
            );
            assert_eq!(widget.style(), &style());
        }
        for line in [
            ClockLine::None,
            ClockLine::Date,
            ClockLine::Weekday,
            ClockLine::WeekdayDate,
        ] {
            let json = serde_json::to_string(&line).unwrap();
            assert_eq!(serde_json::from_str::<ClockLine>(&json).unwrap(), line);
        }
    }

    #[test]
    fn uses_snake_case_tags() {
        let json = serde_json::to_value(Widget::DigitalClock {
            seconds: false,
            hour12: false,
            second_line: ClockLine::WeekdayDate,
            zone: None,
            label: String::new(),
            style: style(),
        })
        .unwrap();
        assert_eq!(json["kind"], "digital_clock");
        assert_eq!(json["second_line"], "weekday_date");
    }

    #[test]
    fn graphs_saved_before_they_existed_load_off_with_five_minutes() {
        let style = r#""style":{"color":{"r":255,"g":255,"b":255},"accent":{"r":0,"g":0,"b":0},"font_family":"Inter"}"#;
        let monitor: Widget = serde_json::from_str(&format!(
            r#"{{"kind":"system_monitor","metrics":["cpu"],{style}}}"#
        ))
        .unwrap();
        assert!(matches!(
            monitor,
            Widget::SystemMonitor {
                graph: false,
                minutes: 5,
                ..
            }
        ));
        let network: Widget =
            serde_json::from_str(&format!(r#"{{"kind":"network",{style}}}"#)).unwrap();
        assert!(matches!(
            network,
            Widget::Network {
                graph: false,
                minutes: 5,
                ..
            }
        ));
    }
}
