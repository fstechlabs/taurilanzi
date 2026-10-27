//! Works out what a widget of Home Assistant shows now, as Home Assistant would show it.

use std::collections::BTreeMap;

use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use serde_json::Value;
use uuid::Uuid;

use super::hass_tiles;
use super::widgets::{day_month, decimal, text, track_time, weekday};
use crate::config::{AssetStore, HassItem, Widget};
use crate::hass::{Entity, Forecast, HassData, HassReport, domain, is_on, labels};
use crate::render::{ImageData, Reading, WeatherDay, WidgetContent};

const MISSING: &str = "—";
const GRAPH_POINTS: usize = 90;
const READINGS_MAX: usize = 3;
const FORECAST_DAYS: usize = 3;

pub fn view(
    widget: &Widget,
    servers: &BTreeMap<Uuid, HassReport>,
    assets: &AssetStore,
    now: Timestamp,
) -> WidgetContent {
    let missing_server = HassReport::default();
    let hass = widget
        .hass_server()
        .and_then(|server| servers.get(&server))
        .unwrap_or(&missing_server);
    let data = &hass.data;
    let connected = hass.connected();
    match widget {
        Widget::HassEntity {
            entity,
            attribute,
            label,
            icon,
            icons,
            ..
        } => {
            let state = data.entity(entity).filter(|_| connected);
            let image = hass_tiles::icon(assets, icon.as_deref(), icons, state);
            if !connected {
                return offline(text(label).or_else(|| Some(entity.clone())), image);
            }
            let Some(state) = state else {
                return missing(text(label).or_else(|| Some(entity.clone())), image);
            };
            let name = text(label).unwrap_or_else(|| name(entity, state));
            if let Some(attribute) = attribute.as_deref().filter(|a| !a.trim().is_empty()) {
                let (value, unit) = attribute_value(data, entity, state, attribute);
                return WidgetContent::Entity {
                    label: Some(name),
                    value,
                    unit,
                    detail: None,
                    extra: None,
                    level: None,
                    on: None,
                    tint: None,
                    icon: image,
                };
            }
            match domain(entity) {
                "camera" | "image" => WidgetContent::Picture {
                    image: data.pictures.get(entity).cloned(),
                    missing: tr!("widgets.reading").into(),
                },
                "media_player" if state.text("media_title").is_some() && is_on(entity, state) => {
                    media(data, entity, state, &name, now)
                }
                _ => {
                    let tile = hass_tiles::tile(data, entity, state, now);
                    WidgetContent::Entity {
                        label: Some(name),
                        value: tile.value,
                        unit: tile.unit,
                        detail: tile.detail,
                        extra: tile.extra,
                        level: tile.level.map(|l| l.clamp(0.0, 1.0) as f32),
                        on: if state.available() {
                            tile.on
                        } else {
                            Some(false)
                        },
                        tint: tile.color,
                        icon: image,
                    }
                }
            }
        }
        Widget::HassGraph {
            entity,
            hours,
            label,
            ..
        } => graph(data, connected, entity, *hours, label, now),
        Widget::HassEntities { entities, .. } => WidgetContent::Readings {
            items: entities
                .iter()
                .take(READINGS_MAX)
                .map(|item| item_reading(data, connected, item, now))
                .collect(),
        },
        Widget::HassPicture { entity, .. } => WidgetContent::Picture {
            image: data.pictures.get(entity).cloned().filter(|_| connected),
            missing: if !connected {
                tr!("hass.widgets.offline").into()
            } else if data.entity(entity).is_none() {
                tr!("hass.widgets.missing").into()
            } else {
                tr!("widgets.reading").into()
            },
        },
        Widget::HassWeather {
            entity, forecast, ..
        } => weather(data, connected, entity, *forecast, now),
        Widget::HassTemplate {
            template, label, ..
        } => {
            let label = text(label);
            if !connected {
                return offline(label, None);
            }
            let (value, detail) = match data.templates.get(template) {
                Some(Ok(result)) if !result.trim().is_empty() => (result.trim().to_string(), None),
                Some(Ok(_)) => (MISSING.into(), None),
                Some(Err(_)) => (
                    MISSING.into(),
                    Some(tr!("hass.widgets.templateError").into()),
                ),
                None => (tr!("widgets.reading").into(), None),
            };
            WidgetContent::Entity {
                label,
                value,
                unit: None,
                detail,
                extra: None,
                level: None,
                on: None,
                tint: None,
                icon: None,
            }
        }
        _ => WidgetContent::Phrase {
            text: String::new(),
        },
    }
}

fn offline(label: Option<String>, icon: Option<ImageData>) -> WidgetContent {
    WidgetContent::Entity {
        label,
        value: MISSING.into(),
        unit: None,
        detail: Some(tr!("hass.widgets.offline").into()),
        extra: None,
        level: None,
        on: Some(false),
        tint: None,
        icon,
    }
}

fn missing(label: Option<String>, icon: Option<ImageData>) -> WidgetContent {
    WidgetContent::Entity {
        label,
        value: MISSING.into(),
        unit: None,
        detail: Some(tr!("hass.widgets.missing").into()),
        extra: None,
        level: None,
        on: Some(false),
        tint: None,
        icon,
    }
}

fn name(id: &str, entity: &Entity) -> String {
    entity.name().unwrap_or(id).to_string()
}

/// `21,5`: with the decimals Home Assistant was told to show, or up to two.
pub fn number(value: f64, precision: Option<u8>) -> String {
    let decimals = precision
        .map(usize::from)
        .unwrap_or(if value.fract().abs() < 1e-9 {
            0
        } else if value.abs() >= 100.0 {
            1
        } else {
            2
        });
    let mut text = format!("{value:.decimals$}");
    if precision.is_none() && text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if text == "-0" {
        text = "0".into();
    }
    decimal(text)
}

pub(super) fn precision(data: &HassData, id: &str) -> Option<u8> {
    data.registry.get(id).and_then(|r| r.precision)
}

fn local(time: Timestamp) -> Zoned {
    time.to_zoned(TimeZone::system())
}

/// A moment Home Assistant gives, as `18:30` today or `27 de octubre, 18:30` another day.
pub(super) fn when(text: &str, now: Timestamp) -> Option<String> {
    let text = text.trim();
    let time = match text.parse::<Timestamp>() {
        Ok(at) => local(at),
        Err(_) => text
            .replace(' ', "T")
            .parse::<DateTime>()
            .ok()?
            .to_zoned(TimeZone::system())
            .ok()?,
    };
    let clock = format!("{:02}:{:02}", time.hour(), time.minute());
    if time.date() == local(now).date() {
        Some(clock)
    } else {
        Some(trf!(
            "hass.widgets.dayTime",
            day = day_month(&time),
            time = clock
        ))
    }
}

/// `1:05:00` for timers.
pub(super) fn clock(seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!(
        "{}:{:02}:{:02}",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}

/// `0:45:00`, or `1 day, 2:00:00`, as Home Assistant writes durations.
pub(super) fn duration_seconds(text: &str) -> Option<i64> {
    let text = text.trim();
    let (days, clock) = match text.split_once(',') {
        Some((days, clock)) => (days.split_whitespace().next()?.parse::<i64>().ok()?, clock),
        None => (0, text),
    };
    let mut parts = clock.trim().split(':').rev();
    let seconds: f64 = parts.next()?.parse().ok()?;
    let minutes: i64 = parts.next().map_or(Ok(0), str::parse).ok()?;
    let hours: i64 = parts.next().map_or(Ok(0), str::parse).ok()?;
    Some(days * 86_400 + hours * 3600 + minutes * 60 + seconds as i64)
}

/// The value of an entity in a few characters, and its unit.
fn reading(data: &HassData, id: &str, entity: &Entity, now: Timestamp) -> (String, Option<String>) {
    if !entity.available() {
        return (labels::state(data, id, entity), None);
    }
    let unit = || entity.text("unit_of_measurement").map(str::to_string);
    let temperature_unit = || Some(data.config.temperature_unit.clone()).filter(|u| !u.is_empty());
    match domain(id) {
        "sensor" | "number" | "input_number" | "counter" | "todo" => {
            if entity.text("device_class") == Some("timestamp") {
                return (
                    when(&entity.state, now).unwrap_or_else(|| entity.state.clone()),
                    None,
                );
            }
            match entity.value() {
                Some(value) => (number(value, precision(data, id)), unit()),
                None => (labels::state(data, id, entity), unit()),
            }
        }
        "date" | "time" | "datetime" | "input_datetime" => (
            when(&entity.state, now).unwrap_or_else(|| entity.state.clone()),
            None,
        ),
        "climate" | "water_heater" => {
            match entity
                .number("current_temperature")
                .or_else(|| entity.number("temperature"))
            {
                Some(value) => (number(value, Some(1)), temperature_unit()),
                None => (labels::state(data, id, entity), None),
            }
        }
        "weather" => match entity.number("temperature") {
            Some(value) => (
                number(value, Some(1)),
                entity
                    .text("temperature_unit")
                    .map(str::to_string)
                    .or_else(temperature_unit),
            ),
            None => (labels::state(data, id, entity), None),
        },
        "humidifier" => match entity
            .number("current_humidity")
            .or_else(|| entity.number("humidity"))
        {
            Some(value) if is_on(id, entity) => (number(value, Some(0)), Some("%".into())),
            _ => (labels::state(data, id, entity), None),
        },
        "timer" => (clock(timer_left(entity, now)), None),
        "calendar" => (
            entity
                .text("message")
                .map(str::to_string)
                .unwrap_or_else(|| labels::state(data, id, entity)),
            None,
        ),
        "event" => match entity.text("event_type") {
            Some(event) => (
                labels::attribute(data, id, entity, "event_type", event),
                None,
            ),
            None => (labels::state(data, id, entity), None),
        },
        "media_player" => (
            entity
                .text("media_title")
                .filter(|_| is_on(id, entity))
                .map(str::to_string)
                .unwrap_or_else(|| labels::state(data, id, entity)),
            None,
        ),
        _ => (labels::state(data, id, entity), None),
    }
}

/// Seconds a timer has left: counted down from when it finishes while it runs.
pub(super) fn timer_left(entity: &Entity, now: Timestamp) -> i64 {
    if entity.state == "active"
        && let Some(finishes) = entity
            .text("finishes_at")
            .and_then(|t| t.parse::<Timestamp>().ok())
    {
        return (finishes.as_second() - now.as_second()).max(0);
    }
    entity
        .text("remaining")
        .or_else(|| entity.text("duration"))
        .and_then(duration_seconds)
        .unwrap_or(0)
}

/// Entities that are either on or off, whose value is drawn in the accent while on.
fn binary(id: &str) -> bool {
    matches!(
        domain(id),
        "light"
            | "switch"
            | "fan"
            | "input_boolean"
            | "binary_sensor"
            | "cover"
            | "valve"
            | "lock"
            | "climate"
            | "water_heater"
            | "automation"
            | "script"
            | "siren"
            | "humidifier"
            | "alarm_control_panel"
            | "vacuum"
            | "lawn_mower"
            | "person"
            | "device_tracker"
            | "timer"
            | "remote"
            | "group"
            | "update"
            | "schedule"
            | "calendar"
            | "sun"
            | "camera"
    )
}

fn media(data: &HassData, id: &str, entity: &Entity, name: &str, now: Timestamp) -> WidgetContent {
    let playing = entity.state == "playing";
    let title = entity
        .text("media_title")
        .filter(|_| is_on(id, entity))
        .map(str::to_string);
    let artist = entity
        .text("media_artist")
        .or_else(|| entity.text("media_album_name"))
        .or_else(|| entity.text("app_name"))
        .map(str::to_string)
        .unwrap_or_else(|| name.to_string());
    if title.is_none() {
        return WidgetContent::NowPlaying {
            title: None,
            artist: format!("{name} · {}", labels::state(data, id, entity)),
            playing: false,
            progress: None,
            time: None,
        };
    }
    let duration = entity.number("media_duration").filter(|d| *d > 0.0);
    let position = entity.number("media_position").map(|position| {
        let updated = entity
            .text("media_position_updated_at")
            .and_then(|t| t.parse::<Timestamp>().ok());
        match updated {
            Some(at) if playing => {
                position + (now.as_millisecond() - at.as_millisecond()) as f64 / 1000.0
            }
            _ => position,
        }
    });
    let position = match (position, duration) {
        (Some(p), Some(d)) => Some(p.min(d)),
        (p, _) => p,
    };
    WidgetContent::NowPlaying {
        title,
        artist,
        playing,
        progress: position
            .zip(duration)
            .map(|(p, d)| (p / d).clamp(0.0, 1.0) as f32),
        time: position.map(|p| match duration {
            Some(d) => format!("{} / {}", track_time(p as f32), track_time(d as f32)),
            None => track_time(p as f32),
        }),
    }
}

/// An attribute of an entity, in Home Assistant's words, with a unit where it is known.
fn attribute_value(
    data: &HassData,
    id: &str,
    entity: &Entity,
    attribute: &str,
) -> (String, Option<String>) {
    let Some(value) = entity.attribute(attribute) else {
        return (MISSING.into(), None);
    };
    let unit = if attribute.contains("temperature") && !attribute.contains("kelvin") {
        Some(data.config.temperature_unit.clone()).filter(|u| !u.is_empty())
    } else if attribute.contains("humidity")
        || matches!(
            attribute,
            "percentage" | "current_position" | "current_tilt_position" | "battery_level"
        )
    {
        Some("%".into())
    } else if attribute.contains("kelvin") {
        Some("K".into())
    } else {
        None
    };
    match value {
        Value::Number(n) => {
            let n = n.as_f64().unwrap_or_default();
            match attribute {
                "brightness" => (number(n / 255.0 * 100.0, Some(0)), Some("%".into())),
                "volume_level" => (number(n * 100.0, Some(0)), Some("%".into())),
                _ => (number(n, None), unit),
            }
        }
        Value::String(text) => (labels::attribute(data, id, entity, attribute, text), None),
        Value::Bool(true) => (tr!("hass.widgets.yes").into(), None),
        Value::Bool(false) => (tr!("hass.widgets.no").into(), None),
        Value::Array(items) => (
            items
                .iter()
                .map(|item| match item {
                    Value::String(text) => labels::attribute(data, id, entity, attribute, text),
                    other => other.to_string(),
                })
                .collect::<Vec<_>>()
                .join(", "),
            None,
        ),
        other => (other.to_string(), None),
    }
}

fn item_reading(data: &HassData, connected: bool, item: &HassItem, now: Timestamp) -> Reading {
    let entity = data.entity(&item.entity).filter(|_| connected);
    let label = text(&item.label)
        .or_else(|| entity.and_then(Entity::name).map(str::to_string))
        .unwrap_or_else(|| item.entity.clone());
    let Some(entity) = entity else {
        return Reading {
            label,
            value: MISSING.into(),
            unit: None,
            on: None,
        };
    };
    let (value, unit) = reading(data, &item.entity, entity, now);
    Reading {
        label,
        value,
        unit,
        on: (binary(&item.entity) && entity.available()).then(|| is_on(&item.entity, entity)),
    }
}

fn graph(
    data: &HassData,
    connected: bool,
    id: &str,
    hours: u32,
    label: &str,
    now: Timestamp,
) -> WidgetContent {
    let entity = data.entity(id).filter(|_| connected);
    let label = text(label).or_else(|| entity.map(|e| name(id, e)));
    let Some(entity) = entity else {
        return WidgetContent::History {
            label,
            value: MISSING.into(),
            unit: None,
            points: Vec::new(),
            low: String::new(),
            high: String::new(),
        };
    };
    let (value, unit) = reading(data, id, entity, now);
    let end = now.as_millisecond() as f64 / 1000.0;
    let start = end - f64::from(hours.max(1)) * 3600.0;
    let history = data
        .history
        .get(id)
        .map(|h| h.as_slice())
        .unwrap_or_default();
    let step = (end - start) / GRAPH_POINTS as f64;
    let mut values: Vec<Option<f64>> = Vec::with_capacity(GRAPH_POINTS);
    let mut next = 0;
    let mut last = None;
    for i in 0..GRAPH_POINTS {
        let until = start + step * (i + 1) as f64;
        while next < history.len() && history[next].0 <= until {
            last = Some(history[next].1);
            next += 1;
        }
        values.push(last);
    }
    let known = values.iter().flatten();
    let (mut low, mut high) =
        known.fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    if low > high {
        return WidgetContent::History {
            label,
            value,
            unit,
            points: Vec::new(),
            low: String::new(),
            high: String::new(),
        };
    }
    if (high - low).abs() < 1e-9 {
        low -= 1.0;
        high += 1.0;
    }
    let digits = precision(data, id);
    let with_unit = |v: f64| match &unit {
        Some(unit) => format!("{} {unit}", number(v, digits)),
        None => number(v, digits),
    };
    WidgetContent::History {
        label,
        value,
        unit: unit.clone(),
        points: values
            .iter()
            .map(|v| v.map(|v| ((v - low) / (high - low)) as f32))
            .collect(),
        low: with_unit(low),
        high: with_unit(high),
    }
}

fn weather(
    data: &HassData,
    connected: bool,
    id: &str,
    forecast: bool,
    now: Timestamp,
) -> WidgetContent {
    let Some(entity) = data.entity(id).filter(|_| connected) else {
        return WidgetContent::Weather {
            condition: String::new(),
            text: if connected {
                tr!("hass.widgets.missing")
            } else {
                tr!("hass.widgets.offline")
            }
            .into(),
            temperature: MISSING.into(),
            detail: None,
            days: Vec::new(),
        };
    };
    let (temperature, unit) = reading(data, id, entity, now);
    let mut detail = Vec::new();
    if let Some(humidity) = entity.number("humidity") {
        detail.push(format!("{} %", number(humidity, Some(0))));
    }
    if let Some(wind) = entity.number("wind_speed") {
        let unit = entity.text("wind_speed_unit").unwrap_or_default();
        detail.push(
            format!("{} {unit}", number(wind, Some(0)))
                .trim()
                .to_string(),
        );
    }
    let today = local(now).date();
    let days = if forecast {
        data.forecasts
            .get(id)
            .map(|days| {
                days.iter()
                    .filter(|day| forecast_date(day) >= today)
                    .take(FORECAST_DAYS)
                    .map(|day| forecast_day(day, today))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    WidgetContent::Weather {
        condition: entity.state.clone(),
        text: labels::state(data, id, entity),
        temperature: match unit {
            Some(unit) => format!("{temperature}{unit}"),
            None => temperature,
        },
        detail: (!detail.is_empty()).then(|| detail.join(" · ")),
        days,
    }
}

fn forecast_date(day: &Forecast) -> jiff::civil::Date {
    let at = Timestamp::from_millisecond((day.at * 1000.0) as i64).unwrap_or_default();
    local(at).date()
}

fn forecast_day(day: &Forecast, today: jiff::civil::Date) -> WeatherDay {
    let at = Timestamp::from_millisecond((day.at * 1000.0) as i64).unwrap_or_default();
    let time = local(at);
    let name = if time.date() == today {
        tr!("hass.widgets.today").to_string()
    } else {
        weekday(&time).chars().take(3).collect()
    };
    let degrees = |v: f64| format!("{}°", number(v, Some(0)));
    WeatherDay {
        name,
        condition: day.condition.clone(),
        high: day.temperature.map(degrees),
        low: day.low.map(degrees),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_as_home_assistant_shows_them() {
        assert_eq!(number(21.0, None), "21");
        assert_eq!(number(21.5, None), "21,5");
        assert_eq!(number(21.333_33, None), "21,33");
        assert_eq!(number(1234.56, None), "1234,6");
        assert_eq!(number(21.0, Some(1)), "21,0");
        assert_eq!(number(-0.001, Some(0)), "0");
    }

    #[test]
    fn reads_durations_and_counts_down() {
        assert_eq!(duration_seconds("0:45:00"), Some(2700));
        assert_eq!(duration_seconds("1 day, 2:00:00"), Some(93_600));
        assert_eq!(duration_seconds("30"), Some(30));
        assert_eq!(clock(2700), "0:45:00");
        assert_eq!(clock(-5), "0:00:00");
    }
}
