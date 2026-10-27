//! What a key can ask of Home Assistant, and where the app finds it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::render::Color;

const KELVIN: (f64, f64) = (1000.0, 12_000.0);

/// One request to Home Assistant, about one of its entities by id (`light.kitchen`), or any of
/// its actions with whatever it takes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HassCommand {
    /// On, off or the other way, in the words of the entity's domain: open and close a cover,
    /// lock and unlock a lock, start and stop a vacuum…
    Switch {
        entity: String,
        mode: HassSwitch,
    },
    /// Scenes, scripts, buttons and automations: runs them. A lock opens its latch.
    Run {
        entity: String,
    },
    Level {
        entity: String,
        level: HassLevel,
        change: HassChange,
    },
    /// One of the options of an entity: a select, a light's effect, a climate's mode…
    Choose {
        entity: String,
        option: HassOption,
        choice: HassChoice,
    },
    Media {
        entity: String,
        control: HassMedia,
    },
    /// A light to a color.
    Color {
        entity: String,
        color: Color,
    },
    /// The code, if the panel needs one, lives in the keychain.
    Alarm {
        entity: String,
        mode: HassAlarm,
    },
    /// Any action of any integration, as `domain.action`.
    Call {
        action: String,
        #[serde(default)]
        target: HassTarget,
        /// Its fields, as Home Assistant takes them.
        #[serde(default = "empty_object")]
        #[specta(type = std::collections::BTreeMap<String, specta_typescript::Unknown>)]
        data: Value,
    },
    /// A sentence for Assist, as if said to Home Assistant.
    Assist {
        text: String,
        /// Empty is Home Assistant's own agent.
        #[serde(default)]
        agent: String,
    },
    /// Fires an event on Home Assistant's bus, for automations to react to.
    Event {
        event: String,
        #[serde(default = "empty_object")]
        #[specta(type = std::collections::BTreeMap<String, specta_typescript::Unknown>)]
        data: Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HassSwitch {
    #[default]
    Toggle,
    On,
    Off,
}

/// Something an entity has more or less of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HassLevel {
    /// Of a light, in percent.
    Brightness,
    /// Of a light, in kelvin.
    ColorTemp,
    /// Of a cover or a valve, in percent open.
    Position,
    /// Of a cover's slats, in percent.
    Tilt,
    /// Of a fan, in percent.
    Speed,
    /// Of a media player, in percent.
    Volume,
    /// The target of a climate or a water heater, in its own unit.
    Temperature,
    /// The target of a humidifier or a climate, in percent.
    Humidity,
    /// Of a number, an input number or a counter.
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HassChange {
    /// Up or down by this much from where it is, within the entity's limits.
    Step {
        #[specta(type = specta_typescript::Number)]
        by: f64,
    },
    Set {
        #[specta(type = specta_typescript::Number)]
        to: f64,
    },
}

/// A list of options an entity picks one from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HassOption {
    /// Of a select or an input select: its state.
    Option,
    /// Of a light.
    Effect,
    /// Of a climate: heat, cool, off…
    HvacMode,
    /// Of a climate, a fan or a humidifier.
    Preset,
    /// Of a climate.
    FanMode,
    /// Of a climate.
    SwingMode,
    /// Of a media player.
    Source,
    /// Of a media player.
    SoundMode,
    /// Of a media player: off, all or one.
    Repeat,
    /// Of a humidifier.
    Mode,
    /// Of a water heater.
    OperationMode,
    /// Of a vacuum.
    FanSpeed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HassChoice {
    /// The one after the current one, back to the first after the last.
    Next,
    Previous,
    Set {
        option: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HassMedia {
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    /// Mutes or unmutes.
    Mute,
    VolumeUp,
    VolumeDown,
    /// Turns shuffle on or off.
    Shuffle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HassAlarm {
    Disarm,
    ArmHome,
    ArmAway,
    ArmNight,
    ArmVacation,
    ArmCustomBypass,
    Trigger,
}

/// What an action acts on. Empty, the action needs no target or takes it in its fields.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct HassTarget {
    pub entities: Vec<String>,
    pub devices: Vec<String>,
    pub areas: Vec<String>,
    pub floors: Vec<String>,
    pub labels: Vec<String>,
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

impl HassLevel {
    /// The values it can be set to; `None` where the entity says.
    pub fn range(self) -> Option<(f64, f64)> {
        match self {
            Self::Brightness
            | Self::Position
            | Self::Tilt
            | Self::Speed
            | Self::Volume
            | Self::Humidity => Some((0.0, 100.0)),
            Self::ColorTemp => Some(KELVIN),
            Self::Temperature | Self::Value => None,
        }
    }
}

impl HassCommand {
    /// The entity it is about, if it is about one.
    pub fn entity(&self) -> Option<&str> {
        match self {
            Self::Switch { entity, .. }
            | Self::Run { entity }
            | Self::Level { entity, .. }
            | Self::Choose { entity, .. }
            | Self::Media { entity, .. }
            | Self::Color { entity, .. }
            | Self::Alarm { entity, .. } => Some(entity),
            Self::Call { target, .. } => target.entities.first().map(String::as_str),
            Self::Assist { .. } | Self::Event { .. } => None,
        }
    }

    /// What is missing for the command to be saved.
    pub fn problem(&self) -> Option<&'static str> {
        if let Some(entity) = self.entity()
            && !matches!(self, Self::Call { .. })
            && !is_entity_id(entity)
        {
            return Some("choose the entity");
        }
        match self {
            Self::Level { level, change, .. } => match change {
                HassChange::Step { by } if *by == 0.0 || !by.is_finite() => {
                    Some("a step cannot be zero")
                }
                HassChange::Set { to } if !to.is_finite() => Some("write the value"),
                HassChange::Set { to } => match level.range() {
                    Some((min, max)) if *to < min || *to > max => {
                        Some("that value is out of range")
                    }
                    _ => None,
                },
                HassChange::Step { .. } => None,
            },
            Self::Choose {
                choice: HassChoice::Set { option },
                ..
            } if option.trim().is_empty() => Some("choose the option"),
            Self::Call { action, data, .. } => {
                if !is_action(action) {
                    Some("choose the action")
                } else if !data.is_object() {
                    Some("the fields of an action go in an object")
                } else {
                    None
                }
            }
            Self::Assist { text, .. } if text.trim().is_empty() => Some("write what to say"),
            Self::Event { event, data } => {
                if event.trim().is_empty() || event.contains(char::is_whitespace) {
                    Some("name the event, without spaces")
                } else if !data.is_object() {
                    Some("the data of an event goes in an object")
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// `domain.object_id`, in lowercase letters, digits and underscores.
pub fn is_entity_id(id: &str) -> bool {
    let mut parts = id.split('.');
    let (Some(domain), Some(object), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let valid = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    valid(domain) && valid(object)
}

/// `domain.action`, as Home Assistant names its actions.
pub fn is_action(action: &str) -> bool {
    is_entity_id(action.trim())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn every_command() -> Vec<HassCommand> {
        use HassCommand::*;
        let entity = || "light.kitchen".to_string();
        vec![
            Switch {
                entity: entity(),
                mode: HassSwitch::Toggle,
            },
            Run {
                entity: "scene.movie".into(),
            },
            Level {
                entity: entity(),
                level: HassLevel::Brightness,
                change: HassChange::Step { by: -10.0 },
            },
            Level {
                entity: "climate.living_room".into(),
                level: HassLevel::Temperature,
                change: HassChange::Set { to: 21.5 },
            },
            Choose {
                entity: "input_select.mode".into(),
                option: HassOption::Option,
                choice: HassChoice::Set {
                    option: "Noche".into(),
                },
            },
            Choose {
                entity: entity(),
                option: HassOption::Effect,
                choice: HassChoice::Next,
            },
            Media {
                entity: "media_player.tv".into(),
                control: HassMedia::PlayPause,
            },
            Color {
                entity: entity(),
                color: crate::render::Color::rgb(255, 100, 0),
            },
            Alarm {
                entity: "alarm_control_panel.home".into(),
                mode: HassAlarm::ArmAway,
            },
            Call {
                action: "notify.notify".into(),
                target: HassTarget::default(),
                data: json!({ "message": "Hola" }),
            },
            Call {
                action: "light.turn_on".into(),
                target: HassTarget {
                    areas: vec!["cocina".into()],
                    ..Default::default()
                },
                data: json!({}),
            },
            Assist {
                text: "Enciende la luz".into(),
                agent: String::new(),
            },
            Event {
                event: "taurilanzi_pressed".into(),
                data: json!({ "key": 1 }),
            },
        ]
    }

    #[test]
    fn every_command_round_trips_and_is_valid() {
        for command in every_command() {
            let json = serde_json::to_string(&command).unwrap();
            assert_eq!(
                serde_json::from_str::<HassCommand>(&json).unwrap(),
                command,
                "{json}"
            );
            assert_eq!(command.problem(), None, "{json}");
        }
    }

    #[test]
    fn uses_snake_case_and_defaults_optional_fields() {
        let json = serde_json::to_value(HassCommand::Level {
            entity: "fan.office".into(),
            level: HassLevel::Speed,
            change: HassChange::Set { to: 50.0 },
        })
        .unwrap();
        assert_eq!(
            json,
            json!({ "kind": "level", "entity": "fan.office", "level": "speed", "change": { "type": "set", "to": 50.0 } })
        );
        let parse = |json: &str| serde_json::from_str::<HassCommand>(json).unwrap();
        assert_eq!(
            parse(r#"{"kind":"call","action":"script.turn_on"}"#),
            HassCommand::Call {
                action: "script.turn_on".into(),
                target: HassTarget::default(),
                data: json!({}),
            }
        );
        assert_eq!(
            parse(r#"{"kind":"assist","text":"hola"}"#),
            HassCommand::Assist {
                text: "hola".into(),
                agent: String::new(),
            }
        );
    }

    #[test]
    fn entities_and_actions_must_be_ids() {
        for id in ["light.kitchen", "sensor.temp_2", "a.b"] {
            assert!(is_entity_id(id), "{id}");
        }
        for id in [
            "",
            "light",
            "light.",
            ".kitchen",
            "Light.Kitchen",
            "light.kit chen",
            "a.b.c",
        ] {
            assert!(!is_entity_id(id), "{id}");
        }
        let switch = |entity: &str| HassCommand::Switch {
            entity: entity.into(),
            mode: HassSwitch::On,
        };
        assert_eq!(switch("").problem(), Some("choose the entity"));
        assert_eq!(switch("light").problem(), Some("choose the entity"));
        let call = |action: &str, data| HassCommand::Call {
            action: action.into(),
            target: HassTarget::default(),
            data,
        };
        assert_eq!(call("", json!({})).problem(), Some("choose the action"));
        assert_eq!(
            call("light.turn_on", json!([1])).problem(),
            Some("the fields of an action go in an object")
        );
    }

    #[test]
    fn levels_stay_in_range() {
        let level = |level, change| HassCommand::Level {
            entity: "light.kitchen".into(),
            level,
            change,
        };
        assert_eq!(
            level(HassLevel::Brightness, HassChange::Step { by: 0.0 }).problem(),
            Some("a step cannot be zero")
        );
        assert_eq!(
            level(HassLevel::Brightness, HassChange::Set { to: 101.0 }).problem(),
            Some("that value is out of range")
        );
        assert_eq!(
            level(HassLevel::ColorTemp, HassChange::Set { to: 500.0 }).problem(),
            Some("that value is out of range")
        );
        assert_eq!(
            level(HassLevel::Temperature, HassChange::Set { to: -5.0 }).problem(),
            None
        );
        assert_eq!(
            level(HassLevel::Value, HassChange::Set { to: f64::NAN }).problem(),
            Some("write the value")
        );
    }

    #[test]
    fn texts_are_required() {
        assert_eq!(
            HassCommand::Assist {
                text: " ".into(),
                agent: String::new()
            }
            .problem(),
            Some("write what to say")
        );
        assert_eq!(
            HassCommand::Event {
                event: "two words".into(),
                data: json!({})
            }
            .problem(),
            Some("name the event, without spaces")
        );
        assert_eq!(
            HassCommand::Choose {
                entity: "select.mode".into(),
                option: HassOption::Option,
                choice: HassChoice::Set {
                    option: String::new()
                },
            }
            .problem(),
            Some("choose the option")
        );
    }
}
