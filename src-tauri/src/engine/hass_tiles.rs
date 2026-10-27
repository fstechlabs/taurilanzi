//! An entity of Home Assistant as a card on a key, like the tiles of Home Assistant's own
//! dashboards: what matters of each kind of entity, and the color of its state.

use std::collections::BTreeMap;

use jiff::Timestamp;

use super::hass_widgets::{clock, duration_seconds, number, precision, timer_left, when};
use crate::config::AssetStore;
use crate::hass::{Entity, HassData, domain, is_on, labels};
use crate::render::{Color, ImageData};

const AMBER: Color = Color::rgb(255, 193, 7);
const ORANGE: Color = Color::rgb(255, 129, 0);
const BLUE: Color = Color::rgb(43, 154, 249);
const CYAN: Color = Color::rgb(0, 188, 212);
const GREEN: Color = Color::rgb(76, 175, 80);
const RED: Color = Color::rgb(244, 67, 54);
const PURPLE: Color = Color::rgb(186, 104, 200);
const INDIGO: Color = Color::rgb(124, 77, 255);
const TEAL: Color = Color::rgb(0, 150, 136);
const GREY: Color = Color::rgb(140, 140, 140);

#[derive(Debug, Default, PartialEq)]
pub struct Tile {
    pub value: String,
    pub unit: Option<String>,
    pub detail: Option<String>,
    pub extra: Option<String>,
    pub level: Option<f64>,
    pub on: Option<bool>,
    pub color: Option<Color>,
}

/// A temperature to reach, short enough for a corner of a key: `21°`, `21,5°`.
fn target_degrees(value: f64) -> String {
    let digits = if value.fract() == 0.0 { 0 } else { 1 };
    format!("{}°", number(value, Some(digits)))
}

fn percent(value: f64) -> String {
    format!("{} %", number(value, Some(0)))
}

/// `hace 5 min`, `en 2 h`: how long from now, as people say it.
pub fn ago(at: Timestamp, now: Timestamp) -> String {
    let seconds = now.as_second() - at.as_second();
    let (past, seconds) = (seconds >= 0, seconds.abs());
    match (past, seconds) {
        (_, ..60) => tr!("hass.time.now").into(),
        (true, ..3600) => trf!("hass.time.minutesAgo", amount = seconds / 60),
        (false, ..3600) => trf!("hass.time.inMinutes", amount = seconds / 60),
        (true, ..86_400) => trf!("hass.time.hoursAgo", amount = seconds / 3600),
        (false, ..86_400) => trf!("hass.time.inHours", amount = seconds / 3600),
        (true, _) => trf!("hass.time.daysAgo", amount = seconds / 86_400),
        (false, _) => trf!("hass.time.inDays", amount = seconds / 86_400),
    }
}

fn timestamp(text: &str) -> Option<Timestamp> {
    text.trim().parse::<Timestamp>().ok()
}

fn since(entity: &Entity, now: Timestamp) -> Option<String> {
    Timestamp::from_millisecond((entity.last_changed * 1000.0) as i64)
        .ok()
        .filter(|_| entity.last_changed > 0.0)
        .map(|at| ago(at, now))
}

/// The light's own color while it is on.
fn light_color(entity: &Entity) -> Color {
    entity
        .attribute("rgb_color")
        .and_then(|rgb| rgb.as_array())
        .filter(|rgb| rgb.len() >= 3)
        .map(|rgb| {
            let part = |i: usize| rgb[i].as_u64().unwrap_or(0).min(255) as u8;
            Color::rgb(part(0), part(1), part(2))
        })
        .unwrap_or(AMBER)
}

fn climate_color(entity: &Entity) -> Option<Color> {
    let action = entity.text("hvac_action");
    let by_action = match action {
        Some("heating" | "preheating") => Some(ORANGE),
        Some("cooling") => Some(BLUE),
        Some("drying") => Some(AMBER),
        Some("fan") => Some(CYAN),
        Some("idle" | "off") => Some(GREY),
        _ => None,
    };
    by_action.or(match entity.state.as_str() {
        "heat" => Some(ORANGE),
        "cool" => Some(BLUE),
        "heat_cool" => Some(AMBER),
        "auto" => Some(GREEN),
        "dry" => Some(AMBER),
        "fan_only" => Some(CYAN),
        _ => None,
    })
}

fn binary_sensor_color(entity: &Entity) -> Color {
    match entity.text("device_class") {
        Some(
            "problem" | "safety" | "smoke" | "gas" | "carbon_monoxide" | "moisture" | "tamper"
            | "battery" | "heat" | "cold",
        ) => RED,
        Some("connectivity" | "running" | "update") => GREEN,
        _ => AMBER,
    }
}

fn battery_color(level: f64) -> Color {
    if level < 20.0 {
        RED
    } else if level < 50.0 {
        AMBER
    } else {
        GREEN
    }
}

/// The card of an entity that is not unavailable.
pub fn tile(data: &HassData, id: &str, entity: &Entity, now: Timestamp) -> Tile {
    let state = || labels::state(data, id, entity);
    let attribute = |name: &str| {
        entity
            .text(name)
            .map(|value| labels::attribute(data, id, entity, name, value))
    };
    if !entity.available() {
        return Tile {
            value: state(),
            ..Default::default()
        };
    }
    let on = is_on(id, entity);
    let temperature_unit = || {
        entity
            .text("temperature_unit")
            .map(str::to_string)
            .or_else(|| Some(data.config.temperature_unit.clone()).filter(|u| !u.is_empty()))
    };
    let unit = || entity.text("unit_of_measurement").map(str::to_string);
    let mut tile = Tile {
        value: state(),
        ..Default::default()
    };
    match domain(id) {
        "light" => {
            tile.on = Some(on);
            if on {
                tile.color = Some(light_color(entity));
                if let Some(brightness) = entity.number("brightness") {
                    let pct = brightness / 255.0 * 100.0;
                    tile.value = number(pct, Some(0));
                    tile.unit = Some("%".into());
                    tile.level = Some(pct / 100.0);
                    tile.detail = Some(state());
                }
                tile.extra = attribute("effect")
                    .filter(|effect| effect != "off")
                    .or_else(|| {
                        entity
                            .number("color_temp_kelvin")
                            .filter(|_| entity.text("color_mode") == Some("color_temp"))
                            .map(|k| format!("{} K", number(k, Some(0))))
                    });
            } else {
                tile.detail = since(entity, now);
            }
        }
        "switch" | "input_boolean" | "siren" | "remote" | "group" | "schedule" => {
            tile.on = Some(on);
            tile.color = on.then_some(AMBER);
            tile.detail = since(entity, now);
        }
        "automation" | "script" => {
            tile.on = Some(on);
            tile.color = on.then_some(AMBER);
            tile.detail = entity
                .text("last_triggered")
                .and_then(timestamp)
                .map(|at| trf!("hass.tiles.lastRun", when = ago(at, now)));
        }
        "scene" | "button" | "input_button" => {
            tile.value = if domain(id) == "scene" {
                tr!("hass.tiles.activate")
            } else {
                tr!("hass.tiles.press")
            }
            .into();
            tile.detail =
                timestamp(&entity.state).map(|at| trf!("hass.tiles.lastRun", when = ago(at, now)));
        }
        "fan" => {
            tile.on = Some(on);
            if on {
                tile.color = Some(CYAN);
                if let Some(pct) = entity.number("percentage") {
                    tile.value = number(pct, Some(0));
                    tile.unit = Some("%".into());
                    tile.level = Some(pct / 100.0);
                    tile.detail = Some(state());
                }
                tile.extra = attribute("preset_mode").or_else(|| {
                    (entity.flag("oscillating") == Some(true))
                        .then(|| tr!("hass.tiles.oscillating").into())
                });
            }
        }
        "climate" => {
            tile.on = Some(on);
            tile.color = if on { climate_color(entity) } else { None };
            let unit = temperature_unit();
            if let Some(current) = entity.number("current_temperature") {
                tile.value = number(current, Some(1));
                tile.unit = unit.clone();
                let target = match (
                    entity.number("temperature"),
                    entity.number("target_temp_low"),
                    entity.number("target_temp_high"),
                ) {
                    (Some(target), _, _) => {
                        Some(trf!("hass.tiles.target", value = target_degrees(target)))
                    }
                    (None, Some(low), Some(high)) => Some(trf!(
                        "hass.tiles.target",
                        value = format!("{}–{}", target_degrees(low), target_degrees(high))
                    )),
                    _ => None,
                };
                tile.detail = if on { target } else { Some(state()) };
            }
            let doing = attribute("hvac_action").unwrap_or_else(state);
            let humidity = entity
                .number("current_humidity")
                .map(|h| trf!("hass.tiles.humidity", value = percent(h)));
            tile.extra = Some(match humidity {
                Some(humidity) => format!("{doing} · {humidity}"),
                None => doing,
            });
        }
        "water_heater" => {
            tile.on = Some(on);
            tile.color = on.then_some(ORANGE);
            let unit = temperature_unit();
            if let Some(value) = entity
                .number("current_temperature")
                .or_else(|| entity.number("temperature"))
            {
                tile.value = number(value, Some(1));
                tile.unit = unit.clone();
            }
            tile.detail = entity
                .number("temperature")
                .map(|t| trf!("hass.tiles.target", value = target_degrees(t)));
            tile.extra = attribute("operation_mode");
        }
        "humidifier" => {
            tile.on = Some(on);
            tile.color = on.then_some(BLUE);
            if let Some(current) = entity.number("current_humidity") {
                tile.value = number(current, Some(0));
                tile.unit = Some("%".into());
                tile.level = Some(current / 100.0);
            }
            tile.detail = entity
                .number("humidity")
                .filter(|_| on)
                .map(|h| trf!("hass.tiles.target", value = percent(h)))
                .or_else(|| Some(state()));
            tile.extra = attribute("mode").or_else(|| attribute("action"));
        }
        "cover" | "valve" => {
            tile.on = Some(on);
            tile.color = on.then_some(PURPLE);
            if let Some(position) = entity.number("current_position") {
                tile.value = number(position, Some(0));
                tile.unit = Some("%".into());
                tile.level = Some(position / 100.0);
                tile.detail = Some(state());
            }
            tile.extra = entity
                .number("current_tilt_position")
                .map(|tilt| trf!("hass.tiles.tilt", value = percent(tilt)));
        }
        "lock" => {
            tile.on = Some(on);
            tile.color = Some(match entity.state.as_str() {
                "locked" => GREEN,
                "locking" | "unlocking" | "opening" => AMBER,
                _ => RED,
            });
            tile.detail = since(entity, now);
        }
        "alarm_control_panel" => {
            tile.on = Some(on);
            tile.color = Some(match entity.state.as_str() {
                "disarmed" => GREEN,
                "arming" | "pending" => AMBER,
                _ => RED,
            });
            tile.detail = entity
                .text("changed_by")
                .map(str::to_string)
                .or_else(|| since(entity, now));
        }
        "vacuum" | "lawn_mower" => {
            tile.on = Some(on);
            tile.color = on.then_some(if domain(id) == "vacuum" { TEAL } else { GREEN });
            tile.detail = entity
                .number("battery_level")
                .map(|b| trf!("hass.tiles.battery", value = percent(b)));
            tile.extra = attribute("fan_speed");
        }
        "media_player" => {
            tile.on = Some(on);
            tile.color = (entity.state == "playing").then_some(INDIGO);
            tile.detail = entity
                .text("source")
                .or_else(|| entity.text("app_name"))
                .map(str::to_string);
            tile.extra = entity
                .number("volume_level")
                .filter(|_| on)
                .map(|v| trf!("hass.tiles.volume", value = percent(v * 100.0)));
        }
        "binary_sensor" => {
            tile.on = Some(on);
            tile.color = on.then(|| binary_sensor_color(entity));
            tile.detail = since(entity, now);
        }
        "person" | "device_tracker" => {
            tile.on = Some(on);
            tile.color = Some(if on { GREEN } else { GREY });
            tile.detail = since(entity, now);
        }
        "sun" => {
            let next = if on { "next_setting" } else { "next_rising" };
            tile.detail = entity.text(next).and_then(|at| when(at, now)).map(|time| {
                if on {
                    trf!("hass.tiles.sets", time = time)
                } else {
                    trf!("hass.tiles.rises", time = time)
                }
            });
            tile.color = on.then_some(AMBER);
        }
        "timer" => {
            tile.on = Some(on);
            tile.color = on.then_some(AMBER);
            tile.value = clock(timer_left(entity, now));
            tile.detail = Some(state());
            tile.level = entity
                .text("duration")
                .and_then(duration_seconds)
                .filter(|d| *d > 0)
                .map(|d| 1.0 - timer_left(entity, now) as f64 / d as f64);
        }
        "number" | "input_number" | "counter" => {
            if let Some(value) = entity.value() {
                tile.value = number(value, precision(data, id));
                tile.unit = unit();
                let (min, max) = if domain(id) == "counter" {
                    (entity.number("minimum"), entity.number("maximum"))
                } else {
                    (entity.number("min"), entity.number("max"))
                };
                if let (Some(min), Some(max)) = (min, max)
                    && max > min
                {
                    tile.level = Some((value - min) / (max - min));
                    tile.detail = Some(format!(
                        "{} – {}{}",
                        number(min, None),
                        number(max, None),
                        unit().map(|u| format!(" {u}")).unwrap_or_default()
                    ));
                }
                tile.color = Some(BLUE);
            }
        }
        "select" | "input_select" => {
            let options = entity.list("options");
            if let Some(at) = options.iter().position(|o| *o == entity.state) {
                tile.detail = Some(trf!(
                    "hass.tiles.option",
                    index = at + 1,
                    count = options.len()
                ));
            }
        }
        "update" => {
            tile.on = Some(on);
            tile.color = on.then_some(AMBER);
            let installed = entity.text("installed_version");
            if on && let Some(latest) = entity.text("latest_version") {
                tile.value = latest.to_string();
                tile.detail = installed.map(|v| trf!("hass.tiles.installed", version = v));
            } else {
                tile.detail = installed.map(str::to_string);
            }
            tile.extra = entity.text("title").map(str::to_string);
        }
        "calendar" => {
            tile.on = Some(on);
            tile.color = on.then_some(AMBER);
            if let Some(message) = entity.text("message") {
                tile.value = message.to_string();
                tile.detail = entity.text("start_time").and_then(|start| when(start, now));
                tile.extra = entity
                    .text("location")
                    .filter(|l| !l.is_empty())
                    .map(str::to_string);
            }
        }
        "todo" => {
            if let Some(count) = entity.value() {
                tile.value = number(count, Some(0));
                tile.detail = Some(tr!("hass.tiles.pending").into());
            }
        }
        "zone" => {
            if let Some(count) = entity.value() {
                tile.value = number(count, Some(0));
                tile.detail = Some(tr!("hass.tiles.people").into());
                tile.color = (count > 0.0).then_some(GREEN);
            }
        }
        "weather" => {
            if let Some(temperature) = entity.number("temperature") {
                tile.value = number(temperature, Some(1));
                tile.unit = temperature_unit();
                tile.detail = Some(state());
            }
            let mut extra = Vec::new();
            if let Some(humidity) = entity.number("humidity") {
                extra.push(percent(humidity));
            }
            if let Some(wind) = entity.number("wind_speed") {
                extra.push(
                    format!(
                        "{} {}",
                        number(wind, Some(0)),
                        entity.text("wind_speed_unit").unwrap_or_default()
                    )
                    .trim()
                    .to_string(),
                );
            }
            tile.extra = (!extra.is_empty()).then(|| extra.join(" · "));
        }
        "date" | "time" | "datetime" | "input_datetime" => {
            if let Some(at) = when(&entity.state, now) {
                tile.value = at;
            }
        }
        "event" => {
            tile.value = attribute("event_type").unwrap_or_else(state);
            tile.detail = timestamp(&entity.state).map(|at| ago(at, now));
        }
        "sensor" => sensor(data, id, entity, now, &mut tile),
        _ => {
            if let Some(at) = timestamp(&entity.state) {
                tile.value = ago(at, now);
            } else if let Some(value) = entity.value() {
                tile.value = number(value, precision(data, id));
                tile.unit = unit();
            }
        }
    }
    tile
}

fn sensor(data: &HassData, id: &str, entity: &Entity, now: Timestamp, tile: &mut Tile) {
    let class = entity.text("device_class");
    if class == Some("timestamp") {
        if let Some(at) = timestamp(&entity.state) {
            tile.value = ago(at, now);
            tile.detail = when(&entity.state, now);
        }
        return;
    }
    let Some(value) = entity.value() else {
        return;
    };
    tile.value = number(value, precision(data, id));
    tile.unit = entity.text("unit_of_measurement").map(str::to_string);
    match class {
        Some("battery") => {
            tile.level = Some(value / 100.0);
            tile.color = Some(battery_color(value));
        }
        Some("humidity" | "moisture") => {
            tile.level = Some(value / 100.0);
            tile.color = Some(BLUE);
        }
        Some("temperature") => tile.color = Some(if value < 18.0 { BLUE } else { ORANGE }),
        Some("power" | "energy" | "current" | "voltage") => tile.color = Some(AMBER),
        Some("carbon_dioxide") => {
            tile.color = Some(if value > 1500.0 {
                RED
            } else if value > 1000.0 {
                AMBER
            } else {
                GREEN
            })
        }
        _ if tile.unit.as_deref() == Some("%") => tile.level = Some(value / 100.0),
        _ => {}
    }
}

/// The icon of the entity in the state it is in: the user's, or Home Assistant's for that state
/// or value, or its default.
pub fn icon(
    assets: &AssetStore,
    custom: Option<&str>,
    icons: &BTreeMap<String, String>,
    entity: Option<&Entity>,
) -> Option<ImageData> {
    let chosen = custom.or_else(|| {
        let entity = entity?;
        icons
            .get(&entity.state)
            .or_else(|| {
                let value = entity.value()?;
                icons
                    .iter()
                    .filter_map(|(key, asset)| {
                        Some((key.strip_prefix('#')?.parse::<f64>().ok()?, asset))
                    })
                    .filter(|(from, _)| *from <= value)
                    .max_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, asset)| asset)
            })
            .or_else(|| icons.get(""))
            .map(String::as_str)
    })?;
    assets.get(chosen).ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn entity(state: &str, attributes: serde_json::Value) -> Entity {
        Entity {
            state: state.into(),
            attributes: attributes.as_object().cloned().unwrap_or_default(),
            ..Default::default()
        }
    }

    #[test]
    fn a_climate_shows_its_temperature_target_and_action() {
        let data = HassData::default();
        let hvac = entity(
            "heat",
            json!({ "current_temperature": 21.3, "temperature": 22, "hvac_action": "heating", "current_humidity": 48 }),
        );
        let tile = tile(&data, "climate.salon", &hvac, Timestamp::now());
        assert_eq!(tile.value, "21,3");
        assert_eq!(tile.detail.as_deref(), Some("→ 22°"));
        assert_eq!(tile.extra.as_deref(), Some("heating · Humedad 48 %"));
        assert_eq!(tile.color, Some(ORANGE));
    }

    #[test]
    fn a_light_shows_its_brightness_in_its_color() {
        let data = HassData::default();
        let light = entity("on", json!({ "brightness": 128, "rgb_color": [255, 0, 0] }));
        let tile = tile(&data, "light.a", &light, Timestamp::now());
        assert_eq!(
            (tile.value.as_str(), tile.unit.as_deref()),
            ("50", Some("%"))
        );
        assert_eq!(tile.color, Some(Color::rgb(255, 0, 0)));
        assert!((tile.level.unwrap() - 0.502).abs() < 0.01);
    }

    #[test]
    fn numbers_show_where_they_sit_between_their_limits() {
        let data = HassData::default();
        let target = entity(
            "55.0",
            json!({ "min": 0, "max": 100, "unit_of_measurement": "%" }),
        );
        let tile = tile(&data, "input_number.volume", &target, Timestamp::now());
        assert_eq!(tile.value, "55");
        assert_eq!(tile.detail.as_deref(), Some("0 – 100 %"));
        assert_eq!(tile.level, Some(0.55));
    }

    #[test]
    fn icons_follow_the_state_or_the_value() {
        let dir = tempfile::tempdir().unwrap();
        let assets = AssetStore::open(dir.path().to_path_buf()).unwrap();
        let svg = |c: &str| {
            assets
                .put(
                    crate::render::ImageFormat::Svg,
                    format!("<svg id=\"{c}\"/>").as_bytes(),
                )
                .unwrap()
        };
        let (default, off, low) = (svg("a"), svg("b"), svg("c"));
        let icons = BTreeMap::from([
            (String::new(), default.clone()),
            ("off".to_string(), off.clone()),
            ("#0".to_string(), low.clone()),
            ("#50".to_string(), default.clone()),
        ]);
        let bytes = |image: Option<ImageData>| image.unwrap().bytes.to_vec();
        let state = |s: &str| entity(s, json!({}));
        assert_eq!(
            bytes(icon(&assets, None, &icons, Some(&state("off")))),
            b"<svg id=\"b\"/>"
        );
        assert_eq!(
            bytes(icon(&assets, None, &icons, Some(&state("on")))),
            b"<svg id=\"a\"/>"
        );
        assert_eq!(
            bytes(icon(&assets, None, &icons, Some(&state("12")))),
            b"<svg id=\"c\"/>"
        );
        assert_eq!(
            bytes(icon(&assets, Some(&off), &icons, Some(&state("on")))),
            b"<svg id=\"b\"/>"
        );
    }
}
