//! Works out how a tile of Home Assistant draws its entity: the drawing that suits its domain
//! and device class, or the one the user chose, and where it stands on it.

use jiff::Timestamp;

use super::hass_tiles::{self, Tile};
use super::hass_widgets::{MISSING, graph, reading, weather};
use super::widgets::text;
use crate::config::TileDrawing;
use crate::hass::{Entity, HassData, domain, is_on};
use crate::render::{Color, CoverShape, Drawing, ImageData, LockState, WidgetContent};

const ORANGE: Color = Color::rgb(255, 129, 0);
const BLUE: Color = Color::rgb(43, 154, 249);
const GREEN: Color = Color::rgb(76, 175, 80);
const AMBER: Color = Color::rgb(255, 193, 7);
const RED: Color = Color::rgb(244, 67, 54);
const PURPLE: Color = Color::rgb(186, 104, 200);

/// What a tile is set to show.
pub struct Settings<'a> {
    pub entity: &'a str,
    pub label: &'a str,
    pub drawing: TileDrawing,
    pub extra: Option<&'a str>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub hours: u32,
}

/// The drawing that suits an entity when the user leaves it to the app. Those only one domain
/// has, such as a vacuum's or the sun's, come as `Auto`.
pub fn suited(id: &str, entity: &Entity) -> TileDrawing {
    let class = entity.text("device_class").unwrap_or_default();
    match domain(id) {
        "light" => TileDrawing::Bulb,
        "switch" if class == "outlet" => TileDrawing::Plug,
        "switch" | "input_boolean" | "automation" => TileDrawing::Switch,
        "fan" => TileDrawing::Fan,
        "cover" => match class {
            "curtain" => TileDrawing::Curtain,
            "blind" | "damper" => TileDrawing::Blind,
            "shade" => TileDrawing::Shade,
            "garage" => TileDrawing::Garage,
            "awning" => TileDrawing::Awning,
            "gate" => TileDrawing::Gate,
            "door" => TileDrawing::Door,
            "window" => TileDrawing::Window,
            _ => TileDrawing::Shutter,
        },
        "valve" => TileDrawing::Valve,
        "lock" => TileDrawing::Lock,
        "climate" | "water_heater" | "humidifier" | "number" | "input_number" => TileDrawing::Gauge,
        "counter" => TileDrawing::Value,
        "sensor" if entity.value().is_some() => match class {
            "temperature" => TileDrawing::Thermometer,
            "humidity" | "moisture" => TileDrawing::Droplet,
            "battery" => TileDrawing::Battery,
            "carbon_dioxide"
            | "carbon_monoxide"
            | "pm1"
            | "pm25"
            | "pm10"
            | "aqi"
            | "volatile_organic_compounds"
            | "volatile_organic_compounds_parts"
            | "nitrogen_dioxide"
            | "ozone" => TileDrawing::Gauge,
            _ if entity.text("unit_of_measurement") == Some("%") => TileDrawing::Gauge,
            _ => TileDrawing::Icon,
        },
        "binary_sensor" => match class {
            "door" | "opening" => TileDrawing::Door,
            "window" => TileDrawing::Window,
            "garage_door" => TileDrawing::Garage,
            "plug" | "power" => TileDrawing::Plug,
            "light" => TileDrawing::Bulb,
            "lock" => TileDrawing::Lock,
            "moisture" => TileDrawing::Droplet,
            "battery" => TileDrawing::Battery,
            _ => TileDrawing::Icon,
        },
        "vacuum" | "sun" | "timer" | "media_player" | "person" | "device_tracker" | "camera"
        | "image" | "weather" => TileDrawing::Auto,
        _ => TileDrawing::Icon,
    }
}

/// The least and the most a gauge or a thermometer of the entity goes, from what it says of
/// itself or, for sensors, from what is usual for their kind.
fn range(id: &str, entity: &Entity, settings: &Settings) -> (f64, f64) {
    let class = entity.text("device_class").unwrap_or_default();
    let fahrenheit = entity
        .text("unit_of_measurement")
        .or_else(|| entity.text("temperature_unit"))
        == Some("°F");
    let (low, high) = match domain(id) {
        "climate" | "water_heater" => (
            entity
                .number("min_temp")
                .unwrap_or(if fahrenheit { 45.0 } else { 7.0 }),
            entity
                .number("max_temp")
                .unwrap_or(if fahrenheit { 95.0 } else { 35.0 }),
        ),
        "humidifier" => (
            entity.number("min_humidity").unwrap_or(0.0),
            entity.number("max_humidity").unwrap_or(100.0),
        ),
        "number" | "input_number" => (
            entity.number("min").unwrap_or(0.0),
            entity.number("max").unwrap_or(100.0),
        ),
        "counter" => (
            entity.number("minimum").unwrap_or(0.0),
            entity.number("maximum").unwrap_or(100.0),
        ),
        _ => match class {
            "temperature" if fahrenheit => (14.0, 104.0),
            "temperature" => (-10.0, 40.0),
            "carbon_dioxide" => (400.0, 2000.0),
            "carbon_monoxide" => (0.0, 100.0),
            "pm1" | "pm25" => (0.0, 75.0),
            "pm10" => (0.0, 150.0),
            "aqi" => (0.0, 300.0),
            "volatile_organic_compounds" | "volatile_organic_compounds_parts" => (0.0, 1000.0),
            "illuminance" => (0.0, 1000.0),
            "power" => (0.0, 3000.0),
            "pressure" | "atmospheric_pressure" => (950.0, 1050.0),
            _ => (0.0, 100.0),
        },
    };
    let (low, high) = (settings.min.unwrap_or(low), settings.max.unwrap_or(high));
    if high > low {
        (low, high)
    } else {
        (low, low + 1.0)
    }
}

fn fraction(value: f64, (low, high): (f64, f64)) -> f32 {
    ((value - low) / (high - low)).clamp(0.0, 1.0) as f32
}

/// How open a cover, a valve or a door is, from its position or else from its state.
fn openness(id: &str, entity: &Entity) -> f32 {
    if let Some(position) = entity.number("current_position") {
        return (position / 100.0).clamp(0.0, 1.0) as f32;
    }
    match entity.state.as_str() {
        "opening" | "closing" => 0.5,
        _ if is_on(id, entity) => 1.0,
        _ => 0.0,
    }
}

fn lock_state(id: &str, entity: &Entity) -> LockState {
    if domain(id) == "binary_sensor" {
        // Their `on` means unlocked.
        return if entity.state == "on" {
            LockState::Unlocked
        } else {
            LockState::Locked
        };
    }
    match entity.state.as_str() {
        "locked" => LockState::Locked,
        "locking" | "unlocking" | "opening" => LockState::Moving,
        "jammed" => LockState::Jammed,
        _ => LockState::Unlocked,
    }
}

/// A sensor's own color, or the one of its kind of drawing.
fn level_color(tile: &Tile, fallback: Color) -> Color {
    tile.color.unwrap_or(fallback)
}

fn cover_shape(drawing: TileDrawing) -> Option<CoverShape> {
    Some(match drawing {
        TileDrawing::Curtain => CoverShape::Curtain,
        TileDrawing::Blind => CoverShape::Blind,
        TileDrawing::Shutter => CoverShape::Shutter,
        TileDrawing::Shade => CoverShape::Shade,
        TileDrawing::Garage => CoverShape::Garage,
        TileDrawing::Awning => CoverShape::Awning,
        TileDrawing::Gate => CoverShape::Gate,
        TileDrawing::Door => CoverShape::Door,
        TileDrawing::Window => CoverShape::Window,
        _ => return None,
    })
}

pub fn view(
    data: &HassData,
    connected: bool,
    settings: &Settings,
    icon: Option<ImageData>,
    now: Timestamp,
) -> WidgetContent {
    let id = settings.entity;
    let entity = data.entity(id).filter(|_| connected);
    let label = text(settings.label)
        .or_else(|| entity.and_then(Entity::name).map(str::to_string))
        .or_else(|| Some(id.to_string()));
    let Some(entity) = entity else {
        return WidgetContent::Tile {
            label,
            value: MISSING.into(),
            unit: None,
            detail: Some(
                if connected {
                    tr!("hass.widgets.missing")
                } else {
                    tr!("hass.widgets.offline")
                }
                .into(),
            ),
            extra: None,
            badge: None,
            on: Some(false),
            tint: None,
            drawing: Drawing::Icon { icon },
        };
    };
    let chosen = match settings.drawing {
        TileDrawing::Auto => suited(id, entity),
        chosen => chosen,
    };
    match (chosen, domain(id)) {
        (TileDrawing::Graph, _) => {
            return graph(data, connected, id, settings.hours, settings.label, now);
        }
        (TileDrawing::Auto, "camera" | "image") => {
            return WidgetContent::Picture {
                image: data.pictures.get(id).cloned(),
                missing: tr!("widgets.reading").into(),
            };
        }
        (TileDrawing::Auto, "weather") => return weather(data, connected, id, true, now),
        _ => {}
    }
    let mut tile = hass_tiles::tile(data, id, entity, now);
    let on = if entity.available() {
        tile.on
    } else {
        Some(false)
    };
    let badge = settings
        .extra
        .filter(|extra| !extra.trim().is_empty())
        .map(|extra| match data.entity(extra) {
            Some(other) => {
                let (value, unit) = reading(data, extra, other, now);
                match unit {
                    Some(unit) => format!("{value} {unit}"),
                    None => value,
                }
            }
            None => MISSING.into(),
        });
    let drawing = drawing(chosen, id, entity, &mut tile, settings, data, icon);
    WidgetContent::Tile {
        label,
        value: tile.value,
        unit: tile.unit,
        detail: tile.detail,
        extra: tile.extra,
        badge,
        on,
        tint: tile.color,
        drawing,
    }
}

fn drawing(
    chosen: TileDrawing,
    id: &str,
    entity: &Entity,
    tile: &mut Tile,
    settings: &Settings,
    data: &HassData,
    icon: Option<ImageData>,
) -> Drawing {
    let level = || tile.level.map(|l| l.clamp(0.0, 1.0) as f32);
    if let Some(shape) = cover_shape(chosen) {
        if tile.color.is_none() && openness(id, entity) > 0.0 {
            tile.color = Some(PURPLE);
        }
        return Drawing::Cover {
            shape,
            open: openness(id, entity),
            tilt: entity
                .number("current_tilt_position")
                .map(|t| (t / 100.0).clamp(0.0, 1.0) as f32),
        };
    }
    match chosen {
        TileDrawing::Icon => Drawing::Icon { icon },
        TileDrawing::Value => Drawing::Value,
        TileDrawing::Bulb => {
            if tile.on == Some(true) && tile.color.is_none() {
                tile.color = Some(AMBER);
            }
            Drawing::Bulb { level: level() }
        }
        TileDrawing::Plug => Drawing::Plug,
        TileDrawing::Switch => Drawing::Switch,
        TileDrawing::Fan => Drawing::Fan { level: level() },
        TileDrawing::Valve => Drawing::Valve {
            open: openness(id, entity),
        },
        TileDrawing::Lock => {
            let state = lock_state(id, entity);
            tile.color = Some(match state {
                LockState::Locked => GREEN,
                LockState::Moving => AMBER,
                LockState::Unlocked | LockState::Jammed => RED,
            });
            Drawing::Lock { state }
        }
        TileDrawing::Gauge => gauge(id, entity, tile, settings),
        TileDrawing::Thermometer => {
            let value = entity
                .number("current_temperature")
                .or_else(|| entity.value())
                .unwrap_or_default();
            tile.color = Some(level_color(tile, if value < 18.0 { BLUE } else { ORANGE }));
            Drawing::Thermometer {
                level: fraction(value, range(id, entity, settings)),
            }
        }
        TileDrawing::Droplet => {
            let level = match domain(id) {
                "binary_sensor" => {
                    if entity.state == "on" {
                        0.9
                    } else {
                        0.15
                    }
                }
                _ => entity
                    .number("current_humidity")
                    .or_else(|| entity.value())
                    .map_or(0.0, |v| fraction(v, range(id, entity, settings))),
            };
            tile.color = Some(level_color(tile, BLUE));
            Drawing::Droplet { level }
        }
        TileDrawing::Battery => {
            let level = match domain(id) {
                // Their `on` means low.
                "binary_sensor" => {
                    if entity.state == "on" {
                        0.1
                    } else {
                        0.9
                    }
                }
                _ => entity
                    .number("battery_level")
                    .or_else(|| entity.value())
                    .map_or(0.0, |v| (v / 100.0).clamp(0.0, 1.0) as f32),
            };
            tile.color = Some(if level < 0.2 {
                RED
            } else if level < 0.5 {
                AMBER
            } else {
                GREEN
            });
            Drawing::Battery { level }
        }
        _ => special(id, entity, tile, data, icon),
    }
}

/// An arc from the least to the most the entity goes, with its target on it.
fn gauge(id: &str, entity: &Entity, tile: &Tile, settings: &Settings) -> Drawing {
    let range = range(id, entity, settings);
    let at = |name: &str| entity.number(name).map(|v| fraction(v, range));
    let (value, marks) = match domain(id) {
        "climate" | "water_heater" => {
            let marks = match (
                at("temperature"),
                at("target_temp_low"),
                at("target_temp_high"),
            ) {
                (Some(target), _, _) => vec![target],
                (None, Some(low), Some(high)) => vec![low, high],
                _ => Vec::new(),
            };
            (
                entity
                    .number("current_temperature")
                    .or_else(|| entity.number("temperature")),
                marks,
            )
        }
        "humidifier" => (
            entity
                .number("current_humidity")
                .or_else(|| entity.number("humidity")),
            at("humidity").into_iter().collect(),
        ),
        _ => (
            entity.value().or_else(|| tile.level.map(|l| l * 100.0)),
            Vec::new(),
        ),
    };
    Drawing::Gauge {
        level: value.map(|v| fraction(v, range)),
        marks: marks.into_iter().filter(|_| is_on(id, entity)).collect(),
        ring: false,
    }
}

/// The drawings only one kind of entity has.
fn special(
    id: &str,
    entity: &Entity,
    tile: &mut Tile,
    data: &HassData,
    icon: Option<ImageData>,
) -> Drawing {
    match domain(id) {
        "vacuum" => Drawing::Vacuum,
        "timer" => Drawing::Gauge {
            level: tile.level.map(|l| l.clamp(0.0, 1.0) as f32),
            marks: Vec::new(),
            ring: true,
        },
        "sun" => {
            let azimuth = entity.number("azimuth").unwrap_or(180.0);
            let elevation = entity.number("elevation").unwrap_or(0.0);
            Drawing::Sun {
                x: ((azimuth - 60.0) / 240.0).clamp(0.0, 1.0) as f32,
                height: (elevation / 70.0).clamp(-1.0, 1.0) as f32,
            }
        }
        "media_player" => {
            if let Some(title) = entity.text("media_title").filter(|_| is_on(id, entity)) {
                tile.extra = tile.detail.take().or(tile.extra.take());
                tile.detail = entity
                    .text("media_artist")
                    .or_else(|| entity.text("media_album_name"))
                    .map(str::to_string);
                tile.value = title.to_string();
                tile.unit = None;
            }
            Drawing::Picture {
                image: data.pictures.get(id).cloned().filter(|_| is_on(id, entity)),
                round: false,
                icon,
            }
        }
        "person" | "device_tracker" => Drawing::Picture {
            image: data.pictures.get(id).cloned(),
            round: true,
            icon,
        },
        _ => Drawing::Icon { icon },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use serde_json::json;

    use super::*;

    fn entity(state: &str, attributes: serde_json::Value) -> Entity {
        Entity {
            state: state.into(),
            attributes: attributes.as_object().cloned().unwrap_or_default(),
            ..Default::default()
        }
    }

    fn data(entities: &[(&str, Entity)]) -> HassData {
        HassData {
            entities: Arc::new(
                entities
                    .iter()
                    .map(|(id, e)| ((*id).to_string(), e.clone()))
                    .collect::<BTreeMap<_, _>>(),
            ),
            ..Default::default()
        }
    }

    fn settings(entity: &str) -> Settings<'_> {
        Settings {
            entity,
            label: "",
            drawing: TileDrawing::Auto,
            extra: None,
            min: None,
            max: None,
            hours: 24,
        }
    }

    fn show(data: &HassData, settings: &Settings) -> WidgetContent {
        view(data, true, settings, None, Timestamp::now())
    }

    fn drawing(content: &WidgetContent) -> &Drawing {
        match content {
            WidgetContent::Tile { drawing, .. } => drawing,
            other => panic!("not a tile: {other:?}"),
        }
    }

    #[test]
    fn each_kind_of_entity_gets_its_drawing() {
        let cases = [
            ("light.a", "on", json!({})),
            ("switch.plug", "on", json!({ "device_class": "outlet" })),
            ("switch.relay", "on", json!({})),
            ("cover.salon", "open", json!({ "device_class": "curtain" })),
            ("cover.garage", "open", json!({ "device_class": "garage" })),
            ("cover.plain", "open", json!({})),
            ("climate.salon", "heat", json!({})),
            (
                "sensor.temp",
                "21.5",
                json!({ "device_class": "temperature" }),
            ),
            (
                "sensor.humidity",
                "48",
                json!({ "device_class": "humidity" }),
            ),
            (
                "sensor.co2",
                "800",
                json!({ "device_class": "carbon_dioxide" }),
            ),
            ("sensor.power", "35", json!({ "device_class": "power" })),
            ("sensor.words", "abc", json!({})),
            (
                "binary_sensor.door",
                "on",
                json!({ "device_class": "door" }),
            ),
            (
                "binary_sensor.motion",
                "on",
                json!({ "device_class": "motion" }),
            ),
            ("vacuum.robot", "cleaning", json!({})),
            ("counter.coffee", "3", json!({})),
        ];
        let expected = [
            TileDrawing::Bulb,
            TileDrawing::Plug,
            TileDrawing::Switch,
            TileDrawing::Curtain,
            TileDrawing::Garage,
            TileDrawing::Shutter,
            TileDrawing::Gauge,
            TileDrawing::Thermometer,
            TileDrawing::Droplet,
            TileDrawing::Gauge,
            TileDrawing::Icon,
            TileDrawing::Icon,
            TileDrawing::Door,
            TileDrawing::Icon,
            TileDrawing::Auto,
            TileDrawing::Value,
        ];
        for ((id, state, attributes), drawing) in cases.into_iter().zip(expected) {
            assert_eq!(suited(id, &entity(state, attributes)), drawing, "{id}");
        }
    }

    #[test]
    fn a_cover_is_drawn_as_open_as_it_is() {
        let blind = entity(
            "open",
            json!({ "device_class": "blind", "current_position": 40, "current_tilt_position": 50 }),
        );
        let data = data(&[("cover.salon", blind)]);
        let content = show(&data, &settings("cover.salon"));
        assert_eq!(
            drawing(&content),
            &Drawing::Cover {
                shape: CoverShape::Blind,
                open: 0.4,
                tilt: Some(0.5),
            }
        );
        let chosen = Settings {
            drawing: TileDrawing::Curtain,
            ..settings("cover.salon")
        };
        assert!(matches!(
            drawing(&show(&data, &chosen)),
            Drawing::Cover {
                shape: CoverShape::Curtain,
                ..
            }
        ));
        let closed = self::data(&[("cover.door", entity("closed", json!({})))]);
        assert!(matches!(
            drawing(&show(&closed, &settings("cover.door"))),
            Drawing::Cover { open: 0.0, .. }
        ));
    }

    #[test]
    fn a_thermostat_shows_where_it_is_and_where_it_goes() {
        let hvac = entity(
            "heat",
            json!({ "current_temperature": 20, "temperature": 22, "min_temp": 10, "max_temp": 30, "hvac_action": "heating" }),
        );
        let data = data(&[("climate.salon", hvac)]);
        let content = show(&data, &settings("climate.salon"));
        assert_eq!(
            drawing(&content),
            &Drawing::Gauge {
                level: Some(0.5),
                marks: vec![0.6],
                ring: false,
            }
        );
        let WidgetContent::Tile { value, detail, .. } = &content else {
            unreachable!()
        };
        assert_eq!(value, "20,0");
        assert_eq!(detail.as_deref(), Some("→ 22°"));
        let off = self::data(&[(
            "climate.salon",
            entity(
                "off",
                json!({ "current_temperature": 20, "temperature": 22 }),
            ),
        )]);
        assert!(matches!(
            drawing(&show(&off, &settings("climate.salon"))),
            Drawing::Gauge { marks, .. } if marks.is_empty()
        ));
    }

    #[test]
    fn a_sensor_follows_the_range_it_is_given() {
        let temp = entity("25", json!({ "device_class": "temperature" }));
        let data = data(&[("sensor.temp", temp)]);
        let ranged = Settings {
            drawing: TileDrawing::Gauge,
            min: Some(20.0),
            max: Some(30.0),
            ..settings("sensor.temp")
        };
        assert!(matches!(
            drawing(&show(&data, &ranged)),
            Drawing::Gauge { level: Some(level), .. } if (level - 0.5).abs() < 1e-6
        ));
        assert!(matches!(
            drawing(&show(&data, &settings("sensor.temp"))),
            Drawing::Thermometer { level } if (level - 0.7).abs() < 1e-6
        ));
    }

    #[test]
    fn a_plug_shows_what_another_entity_measures() {
        let data = data(&[
            (
                "switch.plug",
                entity("on", json!({ "device_class": "outlet" })),
            ),
            (
                "sensor.plug_power",
                entity("35.2", json!({ "unit_of_measurement": "W" })),
            ),
        ]);
        let with_power = Settings {
            extra: Some("sensor.plug_power"),
            ..settings("switch.plug")
        };
        let content = show(&data, &with_power);
        assert_eq!(drawing(&content), &Drawing::Plug);
        let WidgetContent::Tile { badge, on, .. } = content else {
            unreachable!()
        };
        assert_eq!(badge.as_deref(), Some("35,2 W"));
        assert_eq!(on, Some(true));
        let gone = Settings {
            extra: Some("sensor.gone"),
            ..settings("switch.plug")
        };
        let WidgetContent::Tile { badge, .. } = show(&data, &gone) else {
            unreachable!()
        };
        assert_eq!(badge.as_deref(), Some(MISSING));
    }

    #[test]
    fn a_lock_is_drawn_in_the_color_of_its_state() {
        for (state, expected, color) in [
            ("locked", LockState::Locked, GREEN),
            ("unlocked", LockState::Unlocked, RED),
            ("locking", LockState::Moving, AMBER),
            ("jammed", LockState::Jammed, RED),
        ] {
            let data = data(&[("lock.door", entity(state, json!({})))]);
            let content = show(&data, &settings("lock.door"));
            let WidgetContent::Tile { tint, drawing, .. } = content else {
                unreachable!()
            };
            assert_eq!(drawing, Drawing::Lock { state: expected }, "{state}");
            assert_eq!(tint, Some(color), "{state}");
        }
    }

    #[test]
    fn graphs_pictures_and_the_weather_take_the_whole_key() {
        let data = data(&[
            ("sensor.temp", entity("21", json!({}))),
            ("camera.door", entity("idle", json!({}))),
            (
                "weather.home",
                entity("sunny", json!({ "temperature": 20 })),
            ),
        ]);
        let graph = Settings {
            drawing: TileDrawing::Graph,
            ..settings("sensor.temp")
        };
        assert!(matches!(show(&data, &graph), WidgetContent::History { .. }));
        assert!(matches!(
            show(&data, &settings("camera.door")),
            WidgetContent::Picture { .. }
        ));
        assert!(matches!(
            show(&data, &settings("weather.home")),
            WidgetContent::Weather { .. }
        ));
    }

    #[test]
    fn without_home_assistant_the_tile_says_so() {
        let data = data(&[(
            "light.a",
            entity("on", json!({ "friendly_name": "Lámpara" })),
        )]);
        let WidgetContent::Tile {
            label,
            value,
            detail,
            on,
            ..
        } = view(&data, false, &settings("light.a"), None, Timestamp::now())
        else {
            unreachable!()
        };
        assert_eq!(label.as_deref(), Some("light.a"));
        assert_eq!(value, MISSING);
        assert_eq!(detail.as_deref(), Some("Sin conexión"));
        assert_eq!(on, Some(false));
        let WidgetContent::Tile { label, detail, .. } = show(&data, &settings("light.b")) else {
            unreachable!()
        };
        assert_eq!(label.as_deref(), Some("light.b"));
        assert_eq!(detail.as_deref(), Some("No encontrado"));
        let WidgetContent::Tile { label, .. } = show(&data, &settings("light.a")) else {
            unreachable!()
        };
        assert_eq!(label.as_deref(), Some("Lámpara"));
    }
}
