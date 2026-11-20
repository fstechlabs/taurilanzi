//! What the editor lets each widget hold, field by field.

use std::collections::BTreeMap;

use jiff::civil::DateTime;
use jiff::tz::TimeZone;

use super::asset_lookup::Assets;
use super::problem::{Problem, at_most, fail, within_range};
use crate::config::widgets::{HassItem, Metric, Period, Widget, WorldClock};

const CLOCK_LABEL_MAX: usize = 24;
const CITY_LABEL_MAX: usize = 16;
const HOST_MAX: usize = 253;
/// The look of a Home Assistant key names its card with up to 40; the widget's own field, 32.
const HASS_CARD_LABEL_MAX: usize = 40;
const HASS_LABEL_MAX: usize = 32;
const HASS_ITEM_LABEL_MAX: usize = 16;
const METRICS: (usize, usize) = (1, 4);
const PERIODS: (usize, usize) = (1, 4);
const WORLD_CLOCKS: (usize, usize) = (1, 3);
const HASS_ENTITIES: (usize, usize) = (1, 3);
const GRAPH_MINUTES: (u32, u32) = (1, 24 * 60);
const TIMER_SECONDS: (u32, u32) = (1, 10 * 3600);
const COUNTER_STEP: (i32, i32) = (-1000, 1000);
const PHRASE_SECONDS: (u32, u32) = (0, 24 * 3600);
const HASS_GRAPH_HOURS: (u32, u32) = (1, 24 * 7);
const HASS_PICTURE_SECONDS: (u32, u32) = (2, 3600);
const METRIC_OPTIONS: [Metric; 4] = [Metric::Cpu, Metric::Ram, Metric::Gpu, Metric::Disk];
const PERIOD_OPTIONS: [Period; 4] = [Period::Day, Period::Week, Period::Month, Period::Year];

/// Widgets that answer to presses of their key, which a key of several uses to move on.
pub(super) fn answers_presses(widget: &Widget) -> bool {
    matches!(
        widget,
        Widget::Timer { .. } | Widget::Stopwatch { .. } | Widget::Counter { .. }
    )
}

pub(super) fn check_widget(widget: &Widget, assets: &impl Assets) -> Result<(), Problem> {
    if widget.style().font_family.trim().is_empty() {
        return fail("style.font_family cannot be empty");
    }
    match widget {
        Widget::DigitalClock { zone, label, .. } | Widget::AnalogClock { zone, label, .. } => {
            if let Some(zone) = zone.as_deref().filter(|zone| !zone.trim().is_empty()) {
                check_zone("zone", zone)?;
            }
            at_most("label", label, CLOCK_LABEL_MAX)
        }
        Widget::SystemMonitor {
            metrics, minutes, ..
        } => {
            check_choices("metrics", metrics, METRICS, &METRIC_OPTIONS)?;
            within_range("minutes", *minutes, GRAPH_MINUTES)
        }
        Widget::Network { minutes, .. } => within_range("minutes", *minutes, GRAPH_MINUTES),
        Widget::Countdown {
            target,
            label,
            done,
            ..
        } => {
            if !target.trim().is_empty() && target.trim().parse::<DateTime>().is_err() {
                return fail(format!(
                    "target must be a local date and time as YYYY-MM-DDTHH:MM, got {target:?}"
                ));
            }
            at_most("label", label, CLOCK_LABEL_MAX)?;
            at_most("done", done, CLOCK_LABEL_MAX)
        }
        Widget::WorldClocks { clocks, .. } => check_world_clocks(clocks),
        Widget::Timer {
            seconds,
            label,
            done,
            ..
        } => {
            within_range("seconds", *seconds, TIMER_SECONDS)?;
            at_most("label", label, CLOCK_LABEL_MAX)?;
            at_most("done", done, CLOCK_LABEL_MAX)
        }
        Widget::Latency { host, .. } => at_most("host", host, HOST_MAX),
        Widget::Progress { periods, .. } => {
            check_choices("periods", periods, PERIODS, &PERIOD_OPTIONS)
        }
        Widget::Stopwatch { label, .. } => at_most("label", label, CLOCK_LABEL_MAX),
        Widget::Counter { label, step, .. } => {
            at_most("label", label, CLOCK_LABEL_MAX)?;
            within_range("step", *step, COUNTER_STEP)
        }
        Widget::Phrases {
            phrases, seconds, ..
        } => {
            if phrases.is_empty() {
                return fail("phrases needs at least one phrase");
            }
            within_range("seconds", *seconds, PHRASE_SECONDS)
        }
        Widget::HassEntity {
            label, icon, icons, ..
        } => {
            at_most("label", label, HASS_CARD_LABEL_MAX)?;
            check_icons(icon.as_deref(), icons, assets)
        }
        Widget::HassGraph { hours, label, .. } => {
            within_range("hours", *hours, HASS_GRAPH_HOURS)?;
            at_most("label", label, HASS_LABEL_MAX)
        }
        Widget::HassEntities { entities, .. } => check_hass_items(entities),
        Widget::HassPicture { seconds, .. } => {
            within_range("seconds", *seconds, HASS_PICTURE_SECONDS)
        }
        Widget::HassTemplate { label, .. } => at_most("label", label, HASS_LABEL_MAX),
        Widget::HassTile {
            label,
            min,
            max,
            hours,
            icon,
            icons,
            ..
        } => {
            at_most("label", label, HASS_CARD_LABEL_MAX)?;
            for (field, limit) in [("min", min), ("max", max)] {
                if limit.is_some_and(|value| !value.is_finite()) {
                    return fail(format!("{field} must be a number or null"));
                }
            }
            within_range("hours", *hours, HASS_GRAPH_HOURS)?;
            check_icons(icon.as_deref(), icons, assets)
        }
        Widget::Calendar { .. }
        | Widget::Battery { .. }
        | Widget::NowPlaying { .. }
        | Widget::Temperatures { .. }
        | Widget::TopProcesses { .. }
        | Widget::Volume { .. }
        | Widget::Moon { .. }
        | Widget::WordClock { .. }
        | Widget::HassWeather { .. } => Ok(()),
    }
}

/// The editor offers each option of `all` once, and keeps at least one.
fn check_choices<T: PartialEq + serde::Serialize>(
    field: &str,
    chosen: &[T],
    (least, most): (usize, usize),
    all: &[T],
) -> Result<(), Problem> {
    let names = || {
        all.iter()
            .map(|option| serde_json::to_string(option).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let repeated = chosen
        .iter()
        .enumerate()
        .any(|(i, option)| chosen[..i].contains(option));
    if chosen.len() < least || chosen.len() > most || repeated {
        return fail(format!(
            "{field} must be {least} to {most} different values of {}, got {}",
            names(),
            serde_json::to_string(chosen).unwrap_or_default()
        ));
    }
    Ok(())
}

fn check_zone(field: &str, zone: &str) -> Result<(), Problem> {
    if TimeZone::get(zone.trim()).is_err() {
        return fail(format!(
            "{field} must be an IANA time zone such as Europe/Madrid, got {zone:?}"
        ));
    }
    Ok(())
}

fn check_world_clocks(clocks: &[WorldClock]) -> Result<(), Problem> {
    let (least, most) = WORLD_CLOCKS;
    if !(least..=most).contains(&clocks.len()) {
        return fail(format!(
            "clocks must hold {least} to {most} places, got {}",
            clocks.len()
        ));
    }
    for (i, clock) in clocks.iter().enumerate() {
        check_zone("zone", &clock.zone)
            .and_then(|()| at_most("label", &clock.label, CITY_LABEL_MAX))
            .map_err(|problem| problem.within(format!("clocks[{i}]")))?;
    }
    Ok(())
}

fn check_hass_items(entities: &[HassItem]) -> Result<(), Problem> {
    let (least, most) = HASS_ENTITIES;
    if !(least..=most).contains(&entities.len()) {
        return fail(format!(
            "entities must hold {least} to {most} entities, got {}",
            entities.len()
        ));
    }
    for (i, item) in entities.iter().enumerate() {
        at_most("label", &item.label, HASS_ITEM_LABEL_MAX)
            .map_err(|problem| problem.within(format!("entities[{i}]")))?;
    }
    Ok(())
}

fn check_icons(
    icon: Option<&str>,
    icons: &BTreeMap<String, String>,
    assets: &impl Assets,
) -> Result<(), Problem> {
    if let Some(icon) = icon
        && !assets.has_image(icon)
    {
        return fail(format!("icon {icon:?} is not an image of the asset store"));
    }
    for (state, asset) in icons {
        if !assets.has_image(asset) {
            return fail(format!(
                "icons[{state:?}] {asset:?} is not an image of the asset store"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::assets::AssetStore;
    use serde_json::{Value, json};

    use super::*;
    use crate::render::ImageFormat;

    fn assets() -> (tempfile::TempDir, AssetStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = AssetStore::open(dir.path().to_path_buf()).unwrap();
        (dir, store)
    }

    fn widget(kind: &str, fields: Value) -> Widget {
        let mut json = fields;
        json["kind"] = json!(kind);
        json["style"] = json!({ "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 0, "g": 0, "b": 0 }, "font_family": "Inter" });
        serde_json::from_value(json).unwrap()
    }

    fn message(widget: &Widget, assets: &AssetStore) -> String {
        check_widget(widget, assets).unwrap_err().to_string()
    }

    #[test]
    fn lists_hold_as_many_as_the_editor_offers() {
        let (_dir, assets) = assets();
        let ok = |widget: Widget| check_widget(&widget, &assets).is_ok();
        let monitor = |metrics: Value| {
            widget(
                "system_monitor",
                json!({ "metrics": metrics, "graph": true, "minutes": 60 }),
            )
        };
        assert!(ok(monitor(json!(["disk", "cpu"]))));
        assert!(!ok(monitor(json!([]))) && !ok(monitor(json!(["cpu", "cpu"]))));
        let progress = |periods: Value| widget("progress", json!({ "periods": periods }));
        assert!(ok(progress(json!(["year"]))) && !ok(progress(json!(["day", "day"]))));

        let city = |zone: &str| json!({ "zone": zone, "label": "" });
        let clocks =
            |clocks: Value| widget("world_clocks", json!({ "clocks": clocks, "hour12": true }));
        assert!(ok(clocks(json!([city("Europe/Madrid")]))));
        assert_eq!(
            message(
                &clocks(json!([city("UTC"), city("UTC"), city("UTC"), city("UTC")])),
                &assets
            ),
            "clocks must hold 1 to 3 places, got 4"
        );
        assert_eq!(
            message(&clocks(json!([city("Madrid")])), &assets),
            "clocks[0]: zone must be an IANA time zone such as Europe/Madrid, got \"Madrid\""
        );
        let mut named = city("Asia/Tokyo");
        named["label"] = json!("Tokio, la capital");
        assert!(!ok(clocks(json!([named]))), "16 characters");

        let items = |count: usize| {
            widget(
                "hass_entities",
                json!({ "entities": vec![json!({ "entity": "sensor.t", "label": "" }); count] }),
            )
        };
        assert!(ok(items(3)) && !ok(items(0)));
        assert_eq!(
            message(&items(4), &assets),
            "entities must hold 1 to 3 entities, got 4"
        );
    }

    #[test]
    fn numbers_stay_within_the_fields_of_the_editor() {
        let (_dir, assets) = assets();
        let ok = |widget: Widget| check_widget(&widget, &assets).is_ok();
        let timer = |seconds: u32| {
            widget(
                "timer",
                json!({ "seconds": seconds, "label": "", "done": "" }),
            )
        };
        assert!(ok(timer(1)) && ok(timer(36_000)));
        assert!(!ok(timer(0)) && !ok(timer(36_001)));
        let counter = |step: i32| widget("counter", json!({ "label": "", "step": step }));
        assert!(ok(counter(-1000)) && ok(counter(0)) && ok(counter(1000)));
        assert!(!ok(counter(1001)));
        let phrases = |phrases: Value, seconds: u32| {
            widget("phrases", json!({ "phrases": phrases, "seconds": seconds }))
        };
        assert!(ok(phrases(json!([""]), 0)) && ok(phrases(json!(["a"]), 86_400)));
        assert!(!ok(phrases(json!([]), 10)) && !ok(phrases(json!(["a"]), 86_401)));
        let network =
            |minutes: u32| widget("network", json!({ "graph": true, "minutes": minutes }));
        assert!(ok(network(1)) && ok(network(1440)));
        assert!(!ok(network(0)) && !ok(network(1441)));
        let graph = |hours: u32| {
            widget(
                "hass_graph",
                json!({ "entity": "sensor.t", "hours": hours, "label": "" }),
            )
        };
        assert!(ok(graph(168)) && !ok(graph(169)) && !ok(graph(0)));
        let picture = |seconds: u32| {
            widget(
                "hass_picture",
                json!({ "entity": "camera.a", "seconds": seconds }),
            )
        };
        assert!(ok(picture(2)) && ok(picture(3600)));
        assert!(!ok(picture(1)) && !ok(picture(3601)));
        let tile = |hours: u32| widget("hass_tile", json!({ "entity": "light.a", "hours": hours }));
        assert!(ok(tile(24)) && !ok(tile(200)));
    }

    #[test]
    fn texts_are_as_long_as_their_fields_allow() {
        let (_dir, assets) = assets();
        let ok = |widget: Widget| check_widget(&widget, &assets).is_ok();
        let clock = |label: &str| {
            widget(
                "digital_clock",
                json!({ "seconds": false, "hour12": false, "second_line": "date", "zone": null, "label": label }),
            )
        };
        assert!(ok(clock(&"ñ".repeat(24))));
        assert_eq!(
            message(&clock(&"a".repeat(25)), &assets),
            "label can be at most 24 characters, got 25"
        );
        let entity = |label: &str| {
            widget(
                "hass_entity",
                json!({ "entity": "light.a", "label": label }),
            )
        };
        assert!(ok(entity(&"a".repeat(40))) && !ok(entity(&"a".repeat(41))));
        let template = |label: &str| {
            widget(
                "hass_template",
                json!({ "template": "{{ 1 }}", "label": label }),
            )
        };
        assert!(ok(template(&"a".repeat(32))) && !ok(template(&"a".repeat(33))));
        let latency = |host: &str| widget("latency", json!({ "host": host }));
        assert!(ok(latency("")) && !ok(latency(&"a".repeat(254))));
    }

    #[test]
    fn dates_and_zones_are_ones_the_engine_reads() {
        let (_dir, assets) = assets();
        let ok = |widget: Widget| check_widget(&widget, &assets).is_ok();
        let countdown = |target: &str| {
            widget(
                "countdown",
                json!({ "target": target, "label": "", "done": "" }),
            )
        };
        assert!(ok(countdown("2026-12-31T23:59")) && ok(countdown("")));
        assert!(!ok(countdown("31/12/2026")));
        let clock = |zone: Value| {
            widget(
                "analog_clock",
                json!({ "seconds": true, "date": true, "zone": zone, "label": "" }),
            )
        };
        assert!(ok(clock(Value::Null)) && ok(clock(json!(""))) && ok(clock(json!("Asia/Tokyo"))));
        assert!(!ok(clock(json!("Tokyo"))));
    }

    #[test]
    fn icons_of_home_assistant_are_images_of_the_store() {
        let (_dir, assets) = assets();
        let icon = assets.put(ImageFormat::Svg, b"<svg/>").unwrap();
        let missing = format!("{}.svg", "0".repeat(64));
        let card = |own: Value, icons: Value| {
            widget(
                "hass_entity",
                json!({ "entity": "light.a", "icon": own, "icons": icons }),
            )
        };
        assert!(check_widget(&card(json!(icon), json!({ "": icon })), &assets).is_ok());
        assert_eq!(
            message(&card(Value::Null, json!({ "off": missing })), &assets),
            format!("icons[\"off\"] \"{missing}\" is not an image of the asset store")
        );
        assert!(check_widget(&card(json!(missing), json!({})), &assets).is_err());
        let tile = widget(
            "hass_tile",
            json!({ "entity": "light.a", "min": 10.0, "max": 30.0, "icons": { "": missing } }),
        );
        assert!(check_widget(&tile, &assets).is_err());
    }
}
