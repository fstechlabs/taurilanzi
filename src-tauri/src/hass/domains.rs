//! How each kind of entity does what a key asks: which action of its domain, with what fields,
//! worked out from its state where it depends on it. And when an entity counts as on.

use serde_json::{Map, Value, json};

use super::entities::{Entity, domain};
use super::error::HassError;
use crate::config::{
    HassAlarm, HassChange, HassChoice, HassCommand, HassLevel, HassMedia, HassOption, HassSwitch,
    HassTarget,
};

const REPEAT_MODES: [&str; 3] = ["off", "all", "one"];
const DEFAULT_KELVIN: (f64, f64) = (2000.0, 6500.0);

/// What a command comes down to.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    Service {
        domain: String,
        service: String,
        data: Map<String, Value>,
        target: Map<String, Value>,
    },
    Assist {
        text: String,
        agent: String,
    },
    Event {
        event: String,
        data: Value,
    },
    /// Already as asked: dimming a light that is off.
    Nothing,
}

impl Plan {
    fn on(entity: &str, service: &str, data: Map<String, Value>) -> Self {
        Self::on_domain(domain(entity), entity, service, data)
    }

    fn on_domain(domain: &str, entity: &str, service: &str, data: Map<String, Value>) -> Self {
        let mut target = Map::new();
        target.insert("entity_id".into(), entity.into());
        Self::Service {
            domain: domain.into(),
            service: service.into(),
            data,
            target,
        }
    }

    /// The `call_service` command, for a plan that is a service.
    pub fn message(&self) -> Option<Value> {
        let Self::Service {
            domain,
            service,
            data,
            target,
        } = self
        else {
            return None;
        };
        let mut message = json!({ "domain": domain, "service": service, "service_data": data });
        if !target.is_empty() {
            message["target"] = Value::Object(target.clone());
        }
        Some(message)
    }
}

fn fields<const N: usize>(pairs: [(&str, Value); N]) -> Map<String, Value> {
    pairs
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect()
}

/// What to call for `command`. `entity` is the state of the entity it is about, if known; `code`
/// the one saved for it, for alarms and locks.
pub fn plan(
    command: &HassCommand,
    entity: Option<&Entity>,
    code: Option<&str>,
) -> Result<Plan, HassError> {
    let state = |id: &str| entity.ok_or_else(|| HassError::UnknownEntity(id.to_string()));
    let unsupported = |id: &str| HassError::Unsupported(id.to_string());
    let with_code = |mut data: Map<String, Value>| {
        if let Some(code) = code.filter(|c| !c.is_empty()) {
            data.insert("code".into(), code.into());
        }
        data
    };
    match command {
        HassCommand::Switch { entity: id, mode } => switch(id, *mode, entity, code),
        HassCommand::Run { entity: id } => {
            let service = match domain(id) {
                "button" | "input_button" => "press",
                "scene" | "script" => "turn_on",
                "automation" => "trigger",
                "lock" => return Ok(Plan::on(id, "open", with_code(Map::new()))),
                "update" => "install",
                "timer" => "start",
                "counter" => "increment",
                _ => return Ok(Plan::on_domain("homeassistant", id, "turn_on", Map::new())),
            };
            Ok(Plan::on(id, service, Map::new()))
        }
        HassCommand::Level {
            entity: id,
            level,
            change,
        } => self::level(id, *level, *change, entity),
        HassCommand::Choose {
            entity: id,
            option,
            choice,
        } => {
            let spec = option_spec(*option, domain(id)).ok_or_else(|| unsupported(id))?;
            let chosen = match choice {
                HassChoice::Set { option } => option.clone(),
                HassChoice::Next | HassChoice::Previous => {
                    let current = state(id)?;
                    let list = spec.options(current);
                    if list.is_empty() {
                        return Err(unsupported(id));
                    }
                    let now = spec.current(current);
                    let at = list.iter().position(|o| Some(o.as_str()) == now.as_deref());
                    let next = match (choice, at) {
                        (HassChoice::Next, Some(at)) => (at + 1) % list.len(),
                        (_, Some(at)) => (at + list.len() - 1) % list.len(),
                        (HassChoice::Next, None) => 0,
                        (_, None) => list.len() - 1,
                    };
                    list[next].clone()
                }
            };
            Ok(Plan::on(
                id,
                spec.service,
                fields([(spec.field, chosen.into())]),
            ))
        }
        HassCommand::Media {
            entity: id,
            control,
        } => {
            if domain(id) != "media_player" {
                return Err(unsupported(id));
            }
            let (service, data) = match control {
                HassMedia::PlayPause => ("media_play_pause", Map::new()),
                HassMedia::Play => ("media_play", Map::new()),
                HassMedia::Pause => ("media_pause", Map::new()),
                HassMedia::Stop => ("media_stop", Map::new()),
                HassMedia::Next => ("media_next_track", Map::new()),
                HassMedia::Previous => ("media_previous_track", Map::new()),
                HassMedia::VolumeUp => ("volume_up", Map::new()),
                HassMedia::VolumeDown => ("volume_down", Map::new()),
                HassMedia::Mute => {
                    let muted = state(id)?.flag("is_volume_muted").unwrap_or(false);
                    (
                        "volume_mute",
                        fields([("is_volume_muted", (!muted).into())]),
                    )
                }
                HassMedia::Shuffle => {
                    let shuffle = state(id)?.flag("shuffle").unwrap_or(false);
                    ("shuffle_set", fields([("shuffle", (!shuffle).into())]))
                }
            };
            Ok(Plan::on(id, service, data))
        }
        HassCommand::Color { entity: id, color } => {
            if domain(id) != "light" {
                return Err(unsupported(id));
            }
            Ok(Plan::on(
                id,
                "turn_on",
                fields([("rgb_color", json!([color.r, color.g, color.b]))]),
            ))
        }
        HassCommand::Alarm { entity: id, mode } => {
            if domain(id) != "alarm_control_panel" {
                return Err(unsupported(id));
            }
            let service = match mode {
                HassAlarm::Disarm => "alarm_disarm",
                HassAlarm::ArmHome => "alarm_arm_home",
                HassAlarm::ArmAway => "alarm_arm_away",
                HassAlarm::ArmNight => "alarm_arm_night",
                HassAlarm::ArmVacation => "alarm_arm_vacation",
                HassAlarm::ArmCustomBypass => "alarm_arm_custom_bypass",
                HassAlarm::Trigger => "alarm_trigger",
            };
            Ok(Plan::on(id, service, with_code(Map::new())))
        }
        HassCommand::Call {
            action,
            target,
            data,
        } => {
            let (domain, service) = action
                .trim()
                .split_once('.')
                .ok_or_else(|| HassError::Protocol(format!("{action} is not an action")))?;
            Ok(Plan::Service {
                domain: domain.into(),
                service: service.into(),
                data: data.as_object().cloned().unwrap_or_default(),
                target: target_fields(target),
            })
        }
        HassCommand::Assist { text, agent } => Ok(Plan::Assist {
            text: text.trim().into(),
            agent: agent.trim().into(),
        }),
        HassCommand::Event { event, data } => Ok(Plan::Event {
            event: event.trim().into(),
            data: data.clone(),
        }),
    }
}

fn target_fields(target: &HassTarget) -> Map<String, Value> {
    let mut out = Map::new();
    for (name, ids) in [
        ("entity_id", &target.entities),
        ("device_id", &target.devices),
        ("area_id", &target.areas),
        ("floor_id", &target.floors),
        ("label_id", &target.labels),
    ] {
        let ids: Vec<&str> = ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .collect();
        if !ids.is_empty() {
            out.insert(name.into(), ids.into());
        }
    }
    out
}

fn switch(
    id: &str,
    mode: HassSwitch,
    entity: Option<&Entity>,
    code: Option<&str>,
) -> Result<Plan, HassError> {
    let on = || {
        entity
            .map(|state| is_on(id, state))
            .ok_or_else(|| HassError::UnknownEntity(id.to_string()))
    };
    // On, off and toggle, or None where toggling depends on the state.
    let (turn_on, turn_off, toggle) = match domain(id) {
        "light" | "switch" | "fan" | "input_boolean" | "siren" | "humidifier" | "remote"
        | "group" | "media_player" | "automation" | "script" | "climate" => {
            ("turn_on", "turn_off", Some("toggle"))
        }
        "water_heater" | "camera" => ("turn_on", "turn_off", None),
        "cover" => ("open_cover", "close_cover", Some("toggle")),
        "valve" => ("open_valve", "close_valve", Some("toggle")),
        "lock" => ("lock", "unlock", None),
        "vacuum" => ("start", "return_to_base", None),
        "lawn_mower" => ("start_mowing", "dock", None),
        "timer" => ("start", "pause", None),
        "alarm_control_panel" => ("alarm_arm_away", "alarm_disarm", None),
        "button" | "input_button" => ("press", "press", Some("press")),
        "scene" => ("turn_on", "turn_on", Some("turn_on")),
        "update" => ("install", "skip", Some("install")),
        "counter" => ("increment", "decrement", Some("increment")),
        "sensor" | "binary_sensor" | "weather" | "person" | "device_tracker" | "sun" | "zone"
        | "event" | "calendar" | "todo" | "image" | "date" | "time" | "datetime" | "text"
        | "input_text" | "input_datetime" | "number" | "input_number" | "select"
        | "input_select" | "notify" | "tts" | "stt" | "conversation" | "air_quality"
        | "geo_location" | "image_processing" | "wake_word" | "assist_satellite" | "ai_task" => {
            return Err(HassError::Unsupported(id.to_string()));
        }
        _ => {
            let service = match mode {
                HassSwitch::On => "turn_on",
                HassSwitch::Off => "turn_off",
                HassSwitch::Toggle => "toggle",
            };
            return Ok(Plan::on_domain("homeassistant", id, service, Map::new()));
        }
    };
    let service = match mode {
        HassSwitch::On => turn_on,
        HassSwitch::Off => turn_off,
        HassSwitch::Toggle => match toggle {
            Some(toggle) => toggle,
            None if on()? => turn_off,
            None => turn_on,
        },
    };
    let mut data = Map::new();
    if matches!(domain(id), "lock" | "alarm_control_panel")
        && let Some(code) = code.filter(|c| !c.is_empty())
    {
        data.insert("code".into(), code.into());
    }
    Ok(Plan::on(id, service, data))
}

/// Where a level is now and how far it goes, and the action that sets it.
struct LevelSpec {
    now: Option<f64>,
    min: f64,
    max: f64,
    /// Values are rounded to this.
    step: f64,
    service: &'static str,
    set: fn(f64) -> Map<String, Value>,
}

fn level(
    id: &str,
    level: HassLevel,
    change: HassChange,
    entity: Option<&Entity>,
) -> Result<Plan, HassError> {
    let unsupported = || HassError::Unsupported(id.to_string());
    let attr = |name: &str| entity.and_then(|e| e.number(name));
    let limit = |name: &str, default: f64| attr(name).unwrap_or(default);
    let on = entity.map(|e| is_on(id, e));
    let spec = match (level, domain(id)) {
        (HassLevel::Brightness, "light") => {
            if matches!(change, HassChange::Set { to } if to <= 0.0)
                || matches!(on, Some(false))
                    && matches!(change, HassChange::Step { by } if by < 0.0)
            {
                return Ok(match on {
                    Some(false) => Plan::Nothing,
                    _ => Plan::on(id, "turn_off", Map::new()),
                });
            }
            LevelSpec {
                now: if on == Some(true) {
                    attr("brightness").map(|b| b / 255.0 * 100.0)
                } else {
                    on.map(|_| 0.0)
                },
                min: 0.0,
                max: 100.0,
                step: 1.0,
                service: "turn_on",
                set: |v| fields([("brightness_pct", v.into())]),
            }
        }
        (HassLevel::ColorTemp, "light") => LevelSpec {
            now: attr("color_temp_kelvin"),
            min: limit("min_color_temp_kelvin", DEFAULT_KELVIN.0),
            max: limit("max_color_temp_kelvin", DEFAULT_KELVIN.1),
            step: 1.0,
            service: "turn_on",
            set: |v| fields([("color_temp_kelvin", v.into())]),
        },
        (HassLevel::Position, "cover") => LevelSpec {
            now: attr("current_position"),
            min: 0.0,
            max: 100.0,
            step: 1.0,
            service: "set_cover_position",
            set: |v| fields([("position", v.into())]),
        },
        (HassLevel::Position, "valve") => LevelSpec {
            now: attr("current_position"),
            min: 0.0,
            max: 100.0,
            step: 1.0,
            service: "set_valve_position",
            set: |v| fields([("position", v.into())]),
        },
        (HassLevel::Tilt, "cover") => LevelSpec {
            now: attr("current_tilt_position"),
            min: 0.0,
            max: 100.0,
            step: 1.0,
            service: "set_cover_tilt_position",
            set: |v| fields([("tilt_position", v.into())]),
        },
        (HassLevel::Speed, "fan") => LevelSpec {
            now: if on == Some(false) {
                Some(0.0)
            } else {
                attr("percentage")
            },
            min: 0.0,
            max: 100.0,
            step: 1.0,
            service: "set_percentage",
            set: |v| fields([("percentage", v.into())]),
        },
        (HassLevel::Volume, "media_player") => LevelSpec {
            now: attr("volume_level").map(|v| v * 100.0),
            min: 0.0,
            max: 100.0,
            step: 1.0,
            service: "volume_set",
            set: |v| fields([("volume_level", (v / 100.0).into())]),
        },
        (HassLevel::Temperature, domain @ ("climate" | "water_heater")) => {
            let step = attr("target_temp_step").unwrap_or(0.5);
            if domain == "climate"
                && attr("temperature").is_none()
                && let (Some(low), Some(high)) = (attr("target_temp_low"), attr("target_temp_high"))
            {
                // A climate keeping a range moves both ends of it.
                let HassChange::Step { by } = change else {
                    return Err(unsupported());
                };
                let (min, max) = (limit("min_temp", f64::MIN), limit("max_temp", f64::MAX));
                let low = round_to((low + by).clamp(min, max), step);
                let high = round_to((high + by).clamp(min, max), step);
                return Ok(Plan::on(
                    id,
                    "set_temperature",
                    fields([
                        ("target_temp_low", low.into()),
                        ("target_temp_high", high.into()),
                    ]),
                ));
            }
            LevelSpec {
                now: attr("temperature"),
                min: limit("min_temp", f64::MIN),
                max: limit("max_temp", f64::MAX),
                step,
                service: "set_temperature",
                set: |v| fields([("temperature", v.into())]),
            }
        }
        (HassLevel::Humidity, "humidifier" | "climate") => LevelSpec {
            now: attr("humidity"),
            min: limit("min_humidity", 0.0),
            max: limit("max_humidity", 100.0),
            step: attr("target_humidity_step").unwrap_or(1.0),
            service: "set_humidity",
            set: |v| fields([("humidity", v.into())]),
        },
        (HassLevel::Value, "number" | "input_number") => LevelSpec {
            now: entity.and_then(Entity::value),
            min: limit("min", f64::MIN),
            max: limit("max", f64::MAX),
            step: attr("step").unwrap_or(0.0),
            service: "set_value",
            set: |v| fields([("value", v.into())]),
        },
        (HassLevel::Value, "counter") => LevelSpec {
            now: entity.and_then(Entity::value),
            min: limit("minimum", f64::MIN),
            max: limit("maximum", f64::MAX),
            step: 1.0,
            service: "set_value",
            set: |v| fields([("value", (v as i64).into())]),
        },
        _ => return Err(unsupported()),
    };
    let target = match change {
        HassChange::Set { to } => to,
        HassChange::Step { by } => {
            let now = match spec.now {
                Some(now) => now,
                None if entity.is_some() => (spec.min.max(-1e9) + spec.max.min(1e9)) / 2.0,
                None => return Err(HassError::UnknownEntity(id.to_string())),
            };
            now + by
        }
    };
    let value = round_to(target.clamp(spec.min, spec.max), spec.step);
    if spec
        .now
        .is_some_and(|now| (now - value).abs() < f64::EPSILON)
        && matches!(change, HassChange::Step { .. })
    {
        return Ok(Plan::Nothing);
    }
    Ok(Plan::on(id, spec.service, (spec.set)(value)))
}

/// `value` to the nearest multiple of `step`, keeping a tidy number of decimals.
fn round_to(value: f64, step: f64) -> f64 {
    if step <= 0.0 || !step.is_finite() {
        return (value * 1000.0).round() / 1000.0;
    }
    let rounded = (value / step).round() * step;
    (rounded * 1000.0).round() / 1000.0
}

/// Where an entity keeps a list of options, which one it is on, and how to pick another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionSpec {
    /// The attribute with the list; `None` for a fixed one.
    pub list: Option<&'static str>,
    /// The attribute of the current option; `None` is the state.
    pub current: Option<&'static str>,
    pub service: &'static str,
    pub field: &'static str,
}

impl OptionSpec {
    pub fn options(&self, entity: &Entity) -> Vec<String> {
        match self.list {
            Some(list) => entity.list(list),
            None => REPEAT_MODES.iter().map(|m| (*m).to_string()).collect(),
        }
    }

    pub fn current(&self, entity: &Entity) -> Option<String> {
        match self.current {
            Some(attribute) => entity.text(attribute).map(str::to_string),
            None => Some(entity.state.clone()),
        }
    }
}

pub fn option_spec(option: HassOption, domain: &str) -> Option<OptionSpec> {
    let spec = |list, current, service, field| OptionSpec {
        list,
        current,
        service,
        field,
    };
    Some(match (option, domain) {
        (HassOption::Option, "select" | "input_select") => {
            spec(Some("options"), None, "select_option", "option")
        }
        (HassOption::Effect, "light") => {
            spec(Some("effect_list"), Some("effect"), "turn_on", "effect")
        }
        (HassOption::HvacMode, "climate") => {
            spec(Some("hvac_modes"), None, "set_hvac_mode", "hvac_mode")
        }
        (HassOption::Preset, "climate" | "fan" | "humidifier") => spec(
            Some("preset_modes"),
            Some("preset_mode"),
            "set_preset_mode",
            "preset_mode",
        ),
        (HassOption::FanMode, "climate") => spec(
            Some("fan_modes"),
            Some("fan_mode"),
            "set_fan_mode",
            "fan_mode",
        ),
        (HassOption::SwingMode, "climate") => spec(
            Some("swing_modes"),
            Some("swing_mode"),
            "set_swing_mode",
            "swing_mode",
        ),
        (HassOption::Source, "media_player") => spec(
            Some("source_list"),
            Some("source"),
            "select_source",
            "source",
        ),
        (HassOption::SoundMode, "media_player") => spec(
            Some("sound_mode_list"),
            Some("sound_mode"),
            "select_sound_mode",
            "sound_mode",
        ),
        (HassOption::Repeat, "media_player") => spec(None, Some("repeat"), "repeat_set", "repeat"),
        (HassOption::Mode, "humidifier") => {
            spec(Some("available_modes"), Some("mode"), "set_mode", "mode")
        }
        (HassOption::OperationMode, "water_heater") => spec(
            Some("operation_list"),
            Some("operation_mode"),
            "set_operation_mode",
            "operation_mode",
        ),
        (HassOption::FanSpeed, "vacuum") => spec(
            Some("fan_speed_list"),
            Some("fan_speed"),
            "set_fan_speed",
            "fan_speed",
        ),
        _ => return None,
    })
}

/// Whether an entity counts as on, in the words of its domain: a cover open, a lock locked, a
/// person at home, an alarm armed…
pub fn is_on(id: &str, entity: &Entity) -> bool {
    let state = entity.state.as_str();
    match domain(id) {
        "cover" | "valve" => matches!(state, "open" | "opening"),
        "lock" => matches!(state, "locked" | "locking"),
        "media_player" => !matches!(state, "off" | "standby" | "unavailable" | "unknown"),
        "climate" | "water_heater" => !matches!(state, "off" | "unavailable" | "unknown"),
        "vacuum" => state == "cleaning",
        "lawn_mower" => state == "mowing",
        "alarm_control_panel" => {
            state.starts_with("armed") || matches!(state, "arming" | "pending" | "triggered")
        }
        "timer" => state == "active",
        "person" | "device_tracker" => state == "home",
        "sun" => state == "above_horizon",
        _ => state == "on",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Color;

    fn entity(state: &str, attributes: Value) -> Entity {
        Entity {
            state: state.into(),
            attributes: attributes.as_object().cloned().unwrap_or_default(),
            ..Default::default()
        }
    }

    fn service(plan: Plan) -> (String, String, Value) {
        match plan {
            Plan::Service {
                domain,
                service,
                data,
                ..
            } => (domain, service, Value::Object(data)),
            other => panic!("not a service: {other:?}"),
        }
    }

    fn run(command: HassCommand, state: Option<Entity>) -> (String, String, Value) {
        service(plan(&command, state.as_ref(), None).unwrap())
    }

    fn switch(entity: &str, mode: HassSwitch) -> HassCommand {
        HassCommand::Switch {
            entity: entity.into(),
            mode,
        }
    }

    #[test]
    fn switches_speak_each_domain() {
        let cases = [
            ("light.a", HassSwitch::Toggle, None, "light", "toggle"),
            ("cover.a", HassSwitch::On, None, "cover", "open_cover"),
            ("cover.a", HassSwitch::Off, None, "cover", "close_cover"),
            ("valve.a", HassSwitch::Toggle, None, "valve", "toggle"),
            ("lock.a", HassSwitch::On, None, "lock", "lock"),
            (
                "lock.a",
                HassSwitch::Toggle,
                Some("locked"),
                "lock",
                "unlock",
            ),
            (
                "lock.a",
                HassSwitch::Toggle,
                Some("unlocked"),
                "lock",
                "lock",
            ),
            (
                "vacuum.a",
                HassSwitch::Toggle,
                Some("docked"),
                "vacuum",
                "start",
            ),
            (
                "vacuum.a",
                HassSwitch::Toggle,
                Some("cleaning"),
                "vacuum",
                "return_to_base",
            ),
            ("lawn_mower.a", HassSwitch::Off, None, "lawn_mower", "dock"),
            (
                "timer.a",
                HassSwitch::Toggle,
                Some("active"),
                "timer",
                "pause",
            ),
            (
                "water_heater.a",
                HassSwitch::Toggle,
                Some("eco"),
                "water_heater",
                "turn_off",
            ),
            ("button.a", HassSwitch::Off, None, "button", "press"),
            ("scene.a", HassSwitch::Toggle, None, "scene", "turn_on"),
            ("climate.a", HassSwitch::Off, None, "climate", "turn_off"),
            (
                "alarm_control_panel.a",
                HassSwitch::Toggle,
                Some("armed_home"),
                "alarm_control_panel",
                "alarm_disarm",
            ),
            (
                "my_domain.a",
                HassSwitch::Toggle,
                None,
                "homeassistant",
                "toggle",
            ),
        ];
        for (id, mode, state, domain, expected) in cases {
            let (d, s, _) = run(switch(id, mode), state.map(|s| entity(s, json!({}))));
            assert_eq!(
                (d.as_str(), s.as_str()),
                (domain, expected),
                "{id} {mode:?} {state:?}"
            );
        }
        assert!(matches!(
            plan(&switch("lock.a", HassSwitch::Toggle), None, None),
            Err(HassError::UnknownEntity(_))
        ));
        assert!(matches!(
            plan(&switch("sensor.a", HassSwitch::On), None, None),
            Err(HassError::Unsupported(_))
        ));
    }

    #[test]
    fn codes_go_to_locks_and_alarms_only() {
        let lock = plan(&switch("lock.a", HassSwitch::Off), None, Some("1234")).unwrap();
        assert_eq!(service(lock).2, json!({ "code": "1234" }));
        let light = plan(&switch("light.a", HassSwitch::Off), None, Some("1234")).unwrap();
        assert_eq!(service(light).2, json!({}));
        let alarm = HassCommand::Alarm {
            entity: "alarm_control_panel.home".into(),
            mode: HassAlarm::ArmNight,
        };
        let (_, s, data) = service(plan(&alarm, None, Some("9")).unwrap());
        assert_eq!(
            (s.as_str(), data),
            ("alarm_arm_night", json!({ "code": "9" }))
        );
    }

    #[test]
    fn levels_step_from_where_they_are() {
        let level = |id: &str, level, change| HassCommand::Level {
            entity: id.into(),
            level,
            change,
        };
        let bright = entity("on", json!({ "brightness": 128 }));
        let (_, s, data) = run(
            level(
                "light.a",
                HassLevel::Brightness,
                HassChange::Step { by: 10.0 },
            ),
            Some(bright.clone()),
        );
        assert_eq!(
            (s.as_str(), data),
            ("turn_on", json!({ "brightness_pct": 60.0 }))
        );
        let (_, s, _) = run(
            level(
                "light.a",
                HassLevel::Brightness,
                HassChange::Set { to: 0.0 },
            ),
            Some(bright),
        );
        assert_eq!(s, "turn_off");
        let off = entity("off", json!({}));
        assert_eq!(
            plan(
                &level(
                    "light.a",
                    HassLevel::Brightness,
                    HassChange::Step { by: -10.0 }
                ),
                Some(&off),
                None
            )
            .unwrap(),
            Plan::Nothing
        );
        let (_, s, data) = run(
            level(
                "light.a",
                HassLevel::Brightness,
                HassChange::Step { by: 20.0 },
            ),
            Some(off),
        );
        assert_eq!(
            (s.as_str(), data),
            ("turn_on", json!({ "brightness_pct": 20.0 }))
        );

        let climate = entity(
            "heat",
            json!({ "temperature": 21.0, "min_temp": 7, "max_temp": 22 }),
        );
        let (_, s, data) = run(
            level(
                "climate.a",
                HassLevel::Temperature,
                HassChange::Step { by: 1.5 },
            ),
            Some(climate),
        );
        assert_eq!(
            (s.as_str(), data),
            ("set_temperature", json!({ "temperature": 22.0 }))
        );

        let range = entity(
            "heat_cool",
            json!({ "temperature": null, "target_temp_low": 19.0, "target_temp_high": 24.0 }),
        );
        let (_, _, data) = run(
            level(
                "climate.a",
                HassLevel::Temperature,
                HassChange::Step { by: -0.5 },
            ),
            Some(range),
        );
        assert_eq!(
            data,
            json!({ "target_temp_low": 18.5, "target_temp_high": 23.5 })
        );

        let volume = entity("playing", json!({ "volume_level": 0.5 }));
        let (_, s, data) = run(
            level(
                "media_player.a",
                HassLevel::Volume,
                HassChange::Step { by: -10.0 },
            ),
            Some(volume),
        );
        assert_eq!(
            (s.as_str(), data),
            ("volume_set", json!({ "volume_level": 0.4 }))
        );

        let number = entity("3", json!({ "min": 0, "max": 10, "step": 0.5 }));
        let (_, s, data) = run(
            level(
                "input_number.a",
                HassLevel::Value,
                HassChange::Step { by: 0.7 },
            ),
            Some(number),
        );
        assert_eq!((s.as_str(), data), ("set_value", json!({ "value": 3.5 })));

        let cover = entity("open", json!({ "current_position": 100 }));
        assert_eq!(
            plan(
                &level(
                    "cover.a",
                    HassLevel::Position,
                    HassChange::Step { by: 10.0 }
                ),
                Some(&cover),
                None
            )
            .unwrap(),
            Plan::Nothing
        );
        assert!(matches!(
            plan(
                &level(
                    "switch.a",
                    HassLevel::Brightness,
                    HassChange::Set { to: 5.0 }
                ),
                None,
                None
            ),
            Err(HassError::Unsupported(_))
        ));
    }

    #[test]
    fn options_cycle_and_wrap_around() {
        let choose = |option, choice| HassCommand::Choose {
            entity: "input_select.mode".into(),
            option,
            choice,
        };
        let mode = entity("Noche", json!({ "options": ["Casa", "Fuera", "Noche"] }));
        let (_, s, data) = run(
            choose(HassOption::Option, HassChoice::Next),
            Some(mode.clone()),
        );
        assert_eq!(
            (s.as_str(), data),
            ("select_option", json!({ "option": "Casa" }))
        );
        let (_, _, data) = run(choose(HassOption::Option, HassChoice::Previous), Some(mode));
        assert_eq!(data, json!({ "option": "Fuera" }));
        let (_, _, data) = run(
            choose(
                HassOption::Option,
                HassChoice::Set {
                    option: "Fuera".into(),
                },
            ),
            None,
        );
        assert_eq!(data, json!({ "option": "Fuera" }));

        let effect = HassCommand::Choose {
            entity: "light.a".into(),
            option: HassOption::Effect,
            choice: HassChoice::Next,
        };
        let light = entity(
            "on",
            json!({ "effect_list": ["rainbow", "none"], "effect": null }),
        );
        let (_, s, data) = run(effect, Some(light));
        assert_eq!(
            (s.as_str(), data),
            ("turn_on", json!({ "effect": "rainbow" }))
        );

        let repeat = HassCommand::Choose {
            entity: "media_player.a".into(),
            option: HassOption::Repeat,
            choice: HassChoice::Next,
        };
        let player = entity("playing", json!({ "repeat": "one" }));
        assert_eq!(run(repeat, Some(player)).2, json!({ "repeat": "off" }));

        assert!(option_spec(HassOption::Effect, "switch").is_none());
    }

    #[test]
    fn media_toggles_read_the_player() {
        let media = |control| HassCommand::Media {
            entity: "media_player.tv".into(),
            control,
        };
        let player = entity(
            "playing",
            json!({ "is_volume_muted": false, "shuffle": true }),
        );
        let (_, s, data) = run(media(HassMedia::Mute), Some(player.clone()));
        assert_eq!(
            (s.as_str(), data),
            ("volume_mute", json!({ "is_volume_muted": true }))
        );
        let (_, s, data) = run(media(HassMedia::Shuffle), Some(player));
        assert_eq!(
            (s.as_str(), data),
            ("shuffle_set", json!({ "shuffle": false }))
        );
        assert_eq!(run(media(HassMedia::Next), None).1, "media_next_track");
    }

    #[test]
    fn actions_carry_their_target_and_fields() {
        let command = HassCommand::Call {
            action: "light.turn_on".into(),
            target: HassTarget {
                areas: vec!["cocina".into(), " ".into()],
                entities: vec!["light.a".into()],
                ..Default::default()
            },
            data: json!({ "brightness_pct": 40 }),
        };
        let plan = plan(&command, None, None).unwrap();
        assert_eq!(
            plan.message().unwrap(),
            json!({
                "domain": "light", "service": "turn_on",
                "service_data": { "brightness_pct": 40 },
                "target": { "entity_id": ["light.a"], "area_id": ["cocina"] },
            })
        );
        let color = HassCommand::Color {
            entity: "light.a".into(),
            color: Color::rgb(1, 2, 3),
        };
        assert_eq!(run(color, None).2, json!({ "rgb_color": [1, 2, 3] }));
    }

    #[test]
    fn what_counts_as_on() {
        let on = |id: &str, state: &str| is_on(id, &entity(state, json!({})));
        assert!(on("light.a", "on"));
        assert!(!on("light.a", "unavailable"));
        assert!(on("cover.a", "opening"));
        assert!(!on("cover.a", "closed"));
        assert!(on("lock.a", "locked"));
        assert!(on("media_player.a", "idle"));
        assert!(!on("media_player.a", "standby"));
        assert!(on("alarm_control_panel.a", "armed_vacation"));
        assert!(!on("alarm_control_panel.a", "disarmed"));
        assert!(on("person.a", "home"));
        assert!(on("climate.a", "heat_cool"));
    }
}
