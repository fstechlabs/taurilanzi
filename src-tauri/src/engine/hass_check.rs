//! Manual check against a real Home Assistant: connects with a token, runs commands on
//! entities of many domains, waits to see their states change, and draws every widget of Home
//! Assistant with what it reads into `target/hass-check`.
//!
//! Made for Home Assistant's `demo` integration, whose entities it uses; on another Home
//! Assistant the commands on missing entities just fail. From `src-tauri`:
//!
//! ```text
//! TAURILANZI_HASS_URL=http://127.0.0.1:8123 TAURILANZI_HASS_TOKEN=… \
//!   cargo test --lib hass_check -- --ignored --nocapture
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use serde_json::json;

use super::hass_widgets;
use crate::config::{
    AssetStore, HassAlarm, HassChange, HassChoice, HassCommand, HassItem, HassLevel, HassMedia,
    HassOption, HassSwitch, HassTarget, Widget, WidgetStyle,
};
use crate::hass::{Hass, HassReport, Wants, is_on};
use crate::i18n::Language;
use crate::render::{Color, ImageFormat, KeyDesign, Layer, Renderer, WidgetView};
use crate::secrets::Secrets;

const WAIT: Duration = Duration::from_secs(15);

struct Check {
    hass: Hass,
    reports: Receiver<HassReport>,
    last: HassReport,
    failures: Vec<String>,
}

impl Check {
    fn pump(&mut self, until: impl Fn(&HassReport) -> bool) -> bool {
        let deadline = Instant::now() + WAIT;
        while !until(&self.last) {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.reports.recv_timeout(left) {
                Ok(report) => self.last = report,
                Err(_) => return false,
            }
        }
        true
    }

    fn state(&self, entity: &str) -> String {
        self.last
            .data
            .entity(entity)
            .map(|e| e.state.clone())
            .unwrap_or_default()
    }

    /// Runs a command and waits until `expect` holds for the entity it is about.
    fn run(&mut self, name: &str, command: HassCommand, expect: impl Fn(&HassReport) -> bool) {
        let result = self.hass.run(&command);
        let ok = match &result {
            Ok(()) => self.pump(&expect),
            Err(_) => false,
        };
        let entity = command.entity().unwrap_or_default().to_string();
        println!(
            "{} {name}: {} → {}",
            if ok { "✔" } else { "✘" },
            result.map_or_else(|e| e.to_string(), |()| "done".into()),
            self.state(&entity),
        );
        if !ok {
            self.failures.push(name.into());
        }
    }
}

/// Material Design Icons by state, as the editor uploads them from Iconify.
fn icons(assets: &AssetStore, pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(state, name)| {
            let svg = ureq::get(format!("https://api.iconify.design/mdi/{name}.svg"))
                .call()
                .unwrap()
                .into_body()
                .read_to_string()
                .unwrap()
                .replace("#000", "currentColor");
            let asset = assets.put(ImageFormat::Svg, svg.as_bytes()).unwrap();
            ((*state).to_string(), asset)
        })
        .collect()
}

fn style() -> WidgetStyle {
    WidgetStyle {
        color: Color::WHITE,
        accent: Color::rgb(56, 189, 248),
        font_family: "Inter".into(),
    }
}

#[test]
#[ignore = "needs a Home Assistant; see the module docs"]
fn hass_check() {
    let url =
        std::env::var("TAURILANZI_HASS_URL").unwrap_or_else(|_| "http://127.0.0.1:8123".into());
    let token = std::env::var("TAURILANZI_HASS_TOKEN").expect("set TAURILANZI_HASS_TOKEN");
    let dir = tempfile::tempdir().unwrap();
    let (tx, reports) = mpsc::channel();
    let hass = Hass::start(
        Secrets::new(dir.path(), "com.taurilanzi.test"),
        uuid::Uuid::nil(),
        move |report| tx.send(report).is_ok(),
        |data| {
            println!("taurilanzi event: {data}");
            true
        },
    );
    tauri::async_runtime::block_on(hass.set_token(Some(token))).unwrap();
    tauri::async_runtime::block_on(
        hass.set_code("alarm_control_panel.security".into(), Some("1234".into())),
    )
    .unwrap();

    let entities: BTreeSet<String> = [
        "light.bed_light",
        "light.ceiling_lights",
        "light.living_room_rgbww_lights",
        "cover.kitchen_window",
        "cover.hall_window",
        "lock.kitchen_door",
        "fan.living_room_fan",
        "climate.hvac",
        "climate.ecobee",
        "media_player.living_room",
        "media_player.walkman",
        "input_select.house_mode",
        "input_number.volume_target",
        "input_boolean.guests",
        "counter.coffees",
        "script.hello",
        "alarm_control_panel.security",
        "vacuum.demo_vacuum_0_ground_floor",
        "valve.front_garden",
        "sensor.outside_temperature",
        "sensor.outside_humidity",
        "sensor.power_consumption",
        "weather.demo_weather_south",
        "timer.laundry",
        "camera.demo_camera",
        "humidifier.humidifier",
        "water_heater.demo_water_heater",
        "binary_sensor.basement_floor_wet",
        "calendar.calendar_2",
        "update.demo_update_no_install",
        "person.demo",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let template = "{{ states('sensor.outside_temperature') }} °C fuera".to_string();
    hass.want(Wants {
        connect: true,
        url,
        insecure: false,
        language: Some(Language::Es),
        entities: entities.clone(),
        histories: BTreeMap::from([("sensor.outside_temperature".into(), 24)]),
        templates: BTreeSet::from([template.clone()]),
        forecasts: BTreeSet::from(["weather.demo_weather_south".into()]),
        pictures: BTreeMap::from([
            ("camera.demo_camera".into(), 5),
            ("media_player.living_room".into(), 10),
        ]),
    });

    let mut check = Check {
        hass: hass.clone(),
        reports,
        last: HassReport::default(),
        failures: Vec::new(),
    };
    assert!(
        check.pump(|r| r.connected() && r.data.entities.len() >= 10),
        "did not connect: {:?}",
        check.last.info
    );
    println!("{:?}", check.last.info.status);
    println!("{} entities followed", check.last.data.entities.len());

    let switch = |entity: &str, mode| HassCommand::Switch {
        entity: entity.into(),
        mode,
    };
    let state_is = |entity: &'static str, wanted: &'static [&'static str]| {
        move |r: &HassReport| {
            r.data
                .entity(entity)
                .is_some_and(|e| wanted.contains(&e.state.as_str()))
        }
    };
    let attr = |entity: &'static str, name: &'static str, test: fn(f64) -> bool| {
        move |r: &HassReport| {
            r.data
                .entity(entity)
                .and_then(|e| e.number(name))
                .is_some_and(test)
        }
    };

    check.run(
        "light on",
        switch("light.bed_light", HassSwitch::On),
        state_is("light.bed_light", &["on"]),
    );
    check.run(
        "light toggle",
        switch("light.bed_light", HassSwitch::Toggle),
        state_is("light.bed_light", &["off"]),
    );
    check.run(
        "brightness 30 %",
        HassCommand::Level {
            entity: "light.ceiling_lights".into(),
            level: HassLevel::Brightness,
            change: HassChange::Set { to: 30.0 },
        },
        attr("light.ceiling_lights", "brightness", |b| {
            (75.0..=78.0).contains(&b)
        }),
    );
    check.run(
        "brightness +20",
        HassCommand::Level {
            entity: "light.ceiling_lights".into(),
            level: HassLevel::Brightness,
            change: HassChange::Step { by: 20.0 },
        },
        attr("light.ceiling_lights", "brightness", |b| {
            (126.0..=129.0).contains(&b)
        }),
    );
    check.run(
        "color temperature",
        HassCommand::Level {
            entity: "light.ceiling_lights".into(),
            level: HassLevel::ColorTemp,
            change: HassChange::Set { to: 4000.0 },
        },
        attr("light.ceiling_lights", "color_temp_kelvin", |k| {
            (3900.0..=4100.0).contains(&k)
        }),
    );
    check.run(
        "light color",
        HassCommand::Color {
            entity: "light.living_room_rgbww_lights".into(),
            color: Color::rgb(255, 0, 0),
        },
        |r: &HassReport| {
            r.data
                .entity("light.living_room_rgbww_lights")
                .and_then(|e| e.attribute("rgb_color").cloned())
                .is_some_and(|c| c[0] == 255)
        },
    );
    check.run(
        "light effect",
        HassCommand::Choose {
            entity: "light.bed_light".into(),
            option: HassOption::Effect,
            choice: HassChoice::Next,
        },
        |r: &HassReport| {
            r.data
                .entity("light.bed_light")
                .is_some_and(|e| e.text("effect").is_some())
        },
    );
    check.run(
        "close cover",
        switch("cover.kitchen_window", HassSwitch::Off),
        state_is("cover.kitchen_window", &["closing", "closed"]),
    );
    check.run(
        "cover position",
        HassCommand::Level {
            entity: "cover.hall_window".into(),
            level: HassLevel::Position,
            change: HassChange::Set { to: 40.0 },
        },
        state_is("cover.hall_window", &["opening", "closing", "open"]),
    );
    check.run(
        "lock",
        switch("lock.kitchen_door", HassSwitch::Toggle),
        state_is("lock.kitchen_door", &["locked", "locking"]),
    );
    check.run(
        "fan speed",
        HassCommand::Level {
            entity: "fan.living_room_fan".into(),
            level: HassLevel::Speed,
            change: HassChange::Set { to: 66.0 },
        },
        attr("fan.living_room_fan", "percentage", |p| p >= 60.0),
    );
    check.run(
        "climate +1",
        HassCommand::Level {
            entity: "climate.hvac".into(),
            level: HassLevel::Temperature,
            change: HassChange::Step { by: 1.0 },
        },
        attr("climate.hvac", "temperature", |t| t > 7.0),
    );
    check.run(
        "climate mode",
        HassCommand::Choose {
            entity: "climate.hvac".into(),
            option: HassOption::HvacMode,
            choice: HassChoice::Set {
                option: "heat".into(),
            },
        },
        state_is("climate.hvac", &["heat"]),
    );
    check.run(
        "climate range",
        HassCommand::Level {
            entity: "climate.ecobee".into(),
            level: HassLevel::Temperature,
            change: HassChange::Step { by: -0.5 },
        },
        |r: &HassReport| r.data.entity("climate.ecobee").is_some(),
    );
    check.run(
        "pause player",
        HassCommand::Media {
            entity: "media_player.living_room".into(),
            control: HassMedia::PlayPause,
        },
        state_is("media_player.living_room", &["paused"]),
    );
    check.run(
        "player volume",
        HassCommand::Level {
            entity: "media_player.walkman".into(),
            level: HassLevel::Volume,
            change: HassChange::Set { to: 30.0 },
        },
        attr("media_player.walkman", "volume_level", |v| {
            (v - 0.3).abs() < 0.01
        }),
    );
    check.run(
        "mute",
        HassCommand::Media {
            entity: "media_player.walkman".into(),
            control: HassMedia::Mute,
        },
        |r: &HassReport| {
            r.data
                .entity("media_player.walkman")
                .and_then(|e| e.flag("is_volume_muted"))
                == Some(true)
        },
    );
    check.run(
        "sound mode",
        HassCommand::Choose {
            entity: "media_player.walkman".into(),
            option: HassOption::SoundMode,
            choice: HassChoice::Next,
        },
        |r: &HassReport| r.data.entity("media_player.walkman").is_some(),
    );
    check.run(
        "select next",
        HassCommand::Choose {
            entity: "input_select.house_mode".into(),
            option: HassOption::Option,
            choice: HassChoice::Next,
        },
        state_is(
            "input_select.house_mode",
            &["Casa", "Fuera", "Noche", "Vacaciones"],
        ),
    );
    check.run(
        "number",
        HassCommand::Level {
            entity: "input_number.volume_target".into(),
            level: HassLevel::Value,
            change: HassChange::Set { to: 55.0 },
        },
        state_is("input_number.volume_target", &["55.0"]),
    );
    check.run(
        "counter",
        HassCommand::Run {
            entity: "counter.coffees".into(),
        },
        |r: &HassReport| {
            r.data
                .entity("counter.coffees")
                .and_then(|e| e.value())
                .is_some_and(|v| v >= 1.0)
        },
    );
    check.run(
        "input boolean",
        switch("input_boolean.guests", HassSwitch::Toggle),
        state_is("input_boolean.guests", &["on"]),
    );
    check.run(
        "script",
        HassCommand::Run {
            entity: "script.hello".into(),
        },
        state_is("script.hello", &["on", "off"]),
    );
    check.run(
        "alarm",
        HassCommand::Alarm {
            entity: "alarm_control_panel.security".into(),
            mode: HassAlarm::ArmAway,
        },
        state_is("alarm_control_panel.security", &["arming", "armed_away"]),
    );
    check.run(
        "vacuum",
        switch("vacuum.demo_vacuum_0_ground_floor", HassSwitch::Toggle),
        state_is("vacuum.demo_vacuum_0_ground_floor", &["cleaning"]),
    );
    check.run(
        "valve",
        switch("valve.front_garden", HassSwitch::Toggle),
        |r: &HassReport| r.data.entity("valve.front_garden").is_some(),
    );
    check.run(
        "timer",
        switch("timer.laundry", HassSwitch::On),
        state_is("timer.laundry", &["active"]),
    );
    check.run(
        "humidifier",
        HassCommand::Level {
            entity: "humidifier.humidifier".into(),
            level: HassLevel::Humidity,
            change: HassChange::Set { to: 50.0 },
        },
        attr("humidifier.humidifier", "humidity", |h| {
            (h - 50.0).abs() < 0.5
        }),
    );
    check.run(
        "action",
        HassCommand::Call {
            action: "persistent_notification.create".into(),
            target: HassTarget::default(),
            data: json!({ "message": "Desde la comprobación", "title": "Taurilanzi" }),
        },
        |_: &HassReport| true,
    );
    check.run(
        "action with response",
        HassCommand::Call {
            action: "weather.get_forecasts".into(),
            target: HassTarget {
                entities: vec!["weather.demo_weather_south".into()],
                ..Default::default()
            },
            data: json!({ "type": "daily" }),
        },
        |_: &HassReport| true,
    );
    check.run(
        "event",
        HassCommand::Event {
            event: "taurilanzi_check".into(),
            data: json!({ "key": 1 }),
        },
        |_: &HassReport| true,
    );
    check.run(
        "event to the dock",
        HassCommand::Event {
            event: "taurilanzi".into(),
            data: json!({ "page": "Inicio", "brightness": 40 }),
        },
        |_: &HassReport| true,
    );
    check.run(
        "assist",
        HassCommand::Assist {
            text: "turn on the ceiling lights".into(),
            agent: String::new(),
        },
        |_: &HassReport| true,
    );

    check.pump(|r| {
        r.data.pictures.contains_key("camera.demo_camera")
            && r.data.templates.contains_key(&template)
            && r.data.forecasts.contains_key("weather.demo_weather_south")
    });
    println!(
        "pictures: {:?}",
        check.last.data.pictures.keys().collect::<Vec<_>>()
    );
    println!("template: {:?}", check.last.data.templates.get(&template));
    println!(
        "forecast days: {}",
        check
            .last
            .data
            .forecasts
            .get("weather.demo_weather_south")
            .map_or(0, |d| d.len())
    );
    println!(
        "history points: {}",
        check
            .last
            .data
            .history
            .get("sensor.outside_temperature")
            .map_or(0, |h| h.len())
    );
    println!("texts: {}", check.last.data.labels.len());
    let catalog = tauri::async_runtime::block_on(hass.catalog()).unwrap();
    println!(
        "catalog: {} entities, {} areas, {} devices, {} texts",
        catalog.entities.len(),
        catalog.areas.len(),
        catalog.devices.len(),
        catalog.texts.len()
    );
    for id in &entities {
        if let Some(entity) = check.last.data.entity(id) {
            println!("  {id}: {} (on: {})", entity.state, is_on(id, entity));
        }
    }

    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/hass-check");
    std::fs::create_dir_all(&out).unwrap();
    let assets = AssetStore::open(dir.path().join("assets")).unwrap();
    let widgets: Vec<(&str, Widget)> = vec![
        (
            "temperature",
            Widget::HassEntity {
                server: uuid::Uuid::nil(),
                entity: "sensor.outside_temperature".into(),
                attribute: None,
                label: String::new(),
                icon: None,
                icons: icons(&assets, &[("", "thermometer")]),
                style: style(),
            },
        ),
        (
            "light",
            Widget::HassEntity {
                server: uuid::Uuid::nil(),
                entity: "light.ceiling_lights".into(),
                attribute: None,
                label: String::new(),
                icon: None,
                icons: icons(&assets, &[("", "lightbulb"), ("off", "lightbulb-off")]),
                style: style(),
            },
        ),
        (
            "climate",
            Widget::HassEntity {
                server: uuid::Uuid::nil(),
                entity: "climate.hvac".into(),
                attribute: None,
                label: String::new(),
                icon: None,
                icons: icons(&assets, &[("", "thermostat")]),
                style: style(),
            },
        ),
        (
            "cover",
            Widget::HassEntity {
                server: uuid::Uuid::nil(),
                entity: "cover.hall_window".into(),
                attribute: None,
                label: String::new(),
                icon: None,
                icons: icons(&assets, &[("", "window-open"), ("closed", "window-closed")]),
                style: style(),
            },
        ),
        (
            "player",
            Widget::HassEntity {
                server: uuid::Uuid::nil(),
                entity: "media_player.walkman".into(),
                attribute: None,
                label: String::new(),
                icon: None,
                icons: icons(&assets, &[("", "speaker"), ("off", "speaker-off")]),
                style: style(),
            },
        ),
        (
            "timer",
            Widget::HassEntity {
                server: uuid::Uuid::nil(),
                entity: "timer.laundry".into(),
                attribute: None,
                label: String::new(),
                icon: None,
                icons: icons(&assets, &[("", "timer-outline")]),
                style: style(),
            },
        ),
        (
            "binary",
            Widget::HassEntity {
                server: uuid::Uuid::nil(),
                entity: "binary_sensor.basement_floor_wet".into(),
                attribute: None,
                label: String::new(),
                icon: None,
                icons: icons(&assets, &[("", "water-off"), ("on", "water")]),
                style: style(),
            },
        ),
        (
            "calendar",
            Widget::HassEntity {
                server: uuid::Uuid::nil(),
                entity: "calendar.calendar_2".into(),
                attribute: None,
                label: String::new(),
                icon: None,
                icons: icons(&assets, &[("", "calendar")]),
                style: style(),
            },
        ),
        (
            "update",
            Widget::HassEntity {
                server: uuid::Uuid::nil(),
                entity: "update.demo_update_no_install".into(),
                attribute: None,
                label: String::new(),
                icon: None,
                icons: icons(&assets, &[("", "package"), ("on", "package-up")]),
                style: style(),
            },
        ),
        (
            "alarm",
            Widget::HassEntity {
                server: uuid::Uuid::nil(),
                entity: "alarm_control_panel.security".into(),
                attribute: None,
                label: String::new(),
                icon: None,
                icons: icons(
                    &assets,
                    &[
                        ("", "shield"),
                        ("armed_away", "shield-lock"),
                        ("disarmed", "shield-off"),
                    ],
                ),
                style: style(),
            },
        ),
        (
            "hvac action",
            Widget::HassEntity {
                server: uuid::Uuid::nil(),
                entity: "climate.hvac".into(),
                attribute: Some("hvac_action".into()),
                label: String::new(),
                icon: None,
                icons: icons(&assets, &[("", "thermostat")]),
                style: style(),
            },
        ),
        (
            "graph",
            Widget::HassGraph {
                server: uuid::Uuid::nil(),
                entity: "sensor.outside_temperature".into(),
                hours: 24,
                label: String::new(),
                style: style(),
            },
        ),
        (
            "readings",
            Widget::HassEntities {
                server: uuid::Uuid::nil(),
                entities: vec![
                    HassItem {
                        entity: "sensor.outside_temperature".into(),
                        label: "Fuera".into(),
                    },
                    HassItem {
                        entity: "sensor.outside_humidity".into(),
                        label: String::new(),
                    },
                    HassItem {
                        entity: "lock.kitchen_door".into(),
                        label: "Cocina".into(),
                    },
                ],
                style: style(),
            },
        ),
        (
            "camera",
            Widget::HassPicture {
                server: uuid::Uuid::nil(),
                entity: "camera.demo_camera".into(),
                seconds: 5,
                style: style(),
            },
        ),
        (
            "weather",
            Widget::HassWeather {
                server: uuid::Uuid::nil(),
                entity: "weather.demo_weather_south".into(),
                forecast: true,
                style: style(),
            },
        ),
        (
            "template",
            Widget::HassTemplate {
                server: uuid::Uuid::nil(),
                template: template.clone(),
                label: "Plantilla".into(),
                style: style(),
            },
        ),
    ];
    let mut renderer = Renderer::new();
    for (i, (name, widget)) in widgets.iter().enumerate() {
        let content = hass_widgets::view(
            widget,
            &BTreeMap::from([(uuid::Uuid::nil(), check.last.clone())]),
            &assets,
            Timestamp::now(),
        );
        let design = KeyDesign {
            layers: vec![Layer::Widget {
                view: WidgetView {
                    color: Color::WHITE,
                    accent: Color::rgb(56, 189, 248),
                    font_family: "Inter".into(),
                    content,
                },
            }],
            ..KeyDesign::solid(Color::rgb(24, 24, 27))
        };
        for (size_name, size) in [("wide", (452, 196)), ("square", (196, 196))] {
            let png = renderer.render(&design, size).unwrap();
            std::fs::write(
                out.join(format!("{i:02}-{name}-{size_name}.png")),
                png.as_ref(),
            )
            .unwrap();
        }
    }
    println!("widgets drawn in {}", out.display());
    assert!(check.failures.is_empty(), "failed: {:?}", check.failures);
}
