//! What a key may hold as the editor sets it up: its kind agrees with its contents, and its looks
//! and actions are ones the editor could have made.

use super::actions::{check_actions, draws_key};
use super::appearance::check_appearance;
use super::asset_lookup::Assets;
use super::problem::{Problem, fail, tag};
use crate::config::actions::Action;
use crate::config::model::{Appearance, Key, KeyKind, LayerConfig};

/// The text that makes a key saved without a kind a page indicator.
const PAGE_NUMBER: &str = "{page_number}";

/// Whatever the editor could save is accepted, keys saved before they had a kind included. What
/// the engine also needs to know about the rest of the configuration (pages, profiles, servers)
/// it checks itself.
pub fn check_key(key: &Key, assets: &impl Assets) -> Result<(), Problem> {
    check_kind(key)?;
    check_appearance(&key.appearance, assets).map_err(|problem| problem.within("appearance"))?;
    if let Some(state) = &key.second_state {
        check_appearance(&state.appearance, assets)
            .map_err(|problem| problem.within("second_state.appearance"))?;
    }
    for (place, list) in lists(key) {
        check_actions(place, list, assets)?;
    }
    Ok(())
}

fn lists(key: &Key) -> [(&'static str, &[Action]); 4] {
    [
        ("actions", &key.actions),
        ("double_press", &key.double_press),
        ("long_press", &key.long_press),
        (
            "second_state.actions",
            key.second_state
                .as_ref()
                .map_or(&[][..], |state| &state.actions),
        ),
    ]
}

/// The kind the editor reads the key as. It forgives two slips: an `action` key left without
/// actions is basic, and a `widget` key with several widgets is a carousel.
pub(super) fn kind_of(key: &Key) -> KeyKind {
    let widgets = key.appearance.widgets().count();
    match key.kind {
        Some(KeyKind::Action) if key.actions.is_empty() => KeyKind::Basic,
        Some(KeyKind::Widget) if widgets > 1 => KeyKind::Carousel,
        Some(kind) => kind,
        None => inferred(key, widgets),
    }
}

/// What a key saved before keys had a kind is, as the editor works it out. The editor reads two
/// states that follow their first action as an `action` key; no rule here tells them apart.
fn inferred(key: &Key, widgets: usize) -> KeyKind {
    if key.second_state.is_some() {
        return KeyKind::TwoStates;
    }
    if face_of(key).is_some() {
        return KeyKind::Action;
    }
    match widgets {
        0 if shows_page_number(&key.appearance) => KeyKind::PageIndicator,
        0 if key.actions.is_empty() => KeyKind::Basic,
        0 => KeyKind::Action,
        1 => KeyKind::Widget,
        _ => KeyKind::Carousel,
    }
}

/// Which of the actions of a key saved without a kind is what the key is: the first that draws
/// it, or a Home Assistant action about an entity one of its widgets shows.
fn face_of(key: &Key) -> Option<usize> {
    key.actions
        .iter()
        .position(|action| draws_key(action) || shows_entity_of(action, &key.appearance))
}

fn shows_entity_of(action: &Action, look: &Appearance) -> bool {
    let Action::HomeAssistant { command, .. } = action else {
        return false;
    };
    let Some(entity) = command.entity() else {
        return false;
    };
    look.layers.iter().any(|layer| match layer {
        LayerConfig::Widget { widget } => matches!(
            widget,
            crate::config::Widget::HassTile { entity: shown, .. }
                | crate::config::Widget::HassEntity { entity: shown, .. }
                if shown == entity
        ),
        _ => false,
    })
}

fn shows_page_number(look: &Appearance) -> bool {
    look.layers.iter().any(
        |layer| matches!(layer, LayerConfig::Text { content, .. } if content.contains(PAGE_NUMBER)),
    )
}

fn check_kind(key: &Key) -> Result<(), Problem> {
    let kind = kind_of(key);
    let name = kind_name(kind);
    let widgets = key.appearance.widgets().count();
    let second = key.second_state.is_some();
    match kind {
        KeyKind::Basic | KeyKind::PageIndicator if widgets > 0 => {
            return fail(format!(
                "a {name} key shows no widgets, got {widgets}; use kind widget for one and \
                 carousel for several"
            ));
        }
        KeyKind::Widget if widgets == 0 => {
            return fail("a widget key needs one widget layer, got none");
        }
        // The editor keeps a carousel left with one widget, ready to take another.
        KeyKind::Carousel if widgets == 0 => {
            return fail("a carousel key needs widget layers, got none");
        }
        KeyKind::TwoStates if !second => {
            return fail("a two_states key needs second_state");
        }
        _ => {}
    }
    if second && !matches!(kind, KeyKind::TwoStates | KeyKind::Action) {
        return fail(format!(
            "a {name} key has no second_state; only two_states keys and action keys that \
             follow their action have one"
        ));
    }
    check_faces(key, kind, name)
}

/// An action that draws the key can only be what an `action` key is: its first action, which a
/// key saved without a kind finds wherever it is.
fn check_faces(key: &Key, kind: KeyKind, name: &str) -> Result<(), Problem> {
    let allowed = match (kind, key.kind, &key.second_state) {
        (_, _, Some(_)) => None,
        (KeyKind::Action, Some(_), None) => Some(0),
        (KeyKind::Action, None, None) => face_of(key),
        _ => None,
    };
    for (place, list) in lists(key) {
        for (i, action) in list.iter().enumerate() {
            if draws_key(action) && !(place == "actions" && Some(i) == allowed) {
                let action = tag(action, "type");
                return fail(format!(
                    "{place}[{i}] ({action}) draws the key, so it can only be actions[0] of a \
                     key of kind action without second_state; this key is {name}"
                ));
            }
        }
    }
    Ok(())
}

fn kind_name(kind: KeyKind) -> &'static str {
    match kind {
        KeyKind::Basic => "basic",
        KeyKind::Action => "action",
        KeyKind::Widget => "widget",
        KeyKind::Carousel => "carousel",
        KeyKind::TwoStates => "two_states",
        KeyKind::PageIndicator => "page_indicator",
    }
}

#[cfg(test)]
mod tests {
    use crate::config::assets::AssetStore;
    use serde_json::{Value, json};

    use super::*;
    use crate::config::preferences::NewKeyStyle;
    use crate::config::{Preferences, SoundFormat, back_key, first_run};
    use crate::render::ImageFormat;

    struct Setup {
        _dir: tempfile::TempDir,
        assets: AssetStore,
        icon: String,
        sound: String,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let assets = AssetStore::open(dir.path().to_path_buf()).unwrap();
        let icon = assets.put(ImageFormat::Svg, b"<svg/>").unwrap();
        let sound = assets.put_sound(SoundFormat::Wav, b"RIFF").unwrap();
        Setup {
            _dir: dir,
            assets,
            icon,
            sound,
        }
    }

    fn style() -> Value {
        json!({ "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 56, "g": 189, "b": 248 }, "font_family": "Inter" })
    }

    /// `newWidget` of the editor, field by field.
    fn new_widget(kind: &str) -> Value {
        let nil = uuid::Uuid::nil();
        let mut widget = match kind {
            "digital_clock" => {
                json!({ "seconds": false, "hour12": false, "second_line": "weekday_date", "zone": null, "label": "" })
            }
            "analog_clock" => {
                json!({ "seconds": true, "date": true, "zone": null, "label": "" })
            }
            "system_monitor" => {
                json!({ "metrics": ["cpu", "ram", "gpu"], "graph": false, "minutes": 5 })
            }
            "network" => json!({ "graph": false, "minutes": 5 }),
            "countdown" => json!({ "target": "2026-11-20T09:00", "label": "", "done": "" }),
            "world_clocks" => {
                json!({ "clocks": [{ "zone": "America/New_York", "label": "" }, { "zone": "Asia/Tokyo", "label": "" }], "hour12": false })
            }
            "timer" => json!({ "seconds": 25 * 60, "label": "", "done": "" }),
            "top_processes" => json!({ "by": "cpu" }),
            "latency" => json!({ "host": "" }),
            "progress" => json!({ "periods": ["day", "week", "month", "year"] }),
            "stopwatch" => json!({ "label": "" }),
            "counter" => json!({ "label": "", "step": 1 }),
            "phrases" => json!({ "phrases": ["Hola", "Adiós"], "seconds": 10 }),
            "hass_entity" => {
                json!({ "server": nil, "entity": "", "attribute": null, "label": "", "icon": null, "icons": {} })
            }
            "hass_graph" => json!({ "server": nil, "entity": "", "hours": 24, "label": "" }),
            "hass_entities" => {
                json!({ "server": nil, "entities": [{ "entity": "", "label": "" }, { "entity": "", "label": "" }] })
            }
            "hass_picture" => json!({ "server": nil, "entity": "", "seconds": 10 }),
            "hass_weather" => json!({ "server": nil, "entity": "", "forecast": true }),
            "hass_template" => json!({ "server": nil, "template": "", "label": "" }),
            "hass_tile" => {
                json!({ "server": nil, "entity": "", "label": "", "drawing": "auto", "extra": null, "min": null, "max": null, "hours": 24, "icon": null, "icons": {} })
            }
            _ => json!({}),
        };
        widget["kind"] = json!(kind);
        widget["style"] = style();
        widget
    }

    const WIDGET_KINDS: [&str; 27] = [
        "digital_clock",
        "analog_clock",
        "system_monitor",
        "network",
        "countdown",
        "world_clocks",
        "calendar",
        "timer",
        "battery",
        "now_playing",
        "temperatures",
        "top_processes",
        "latency",
        "volume",
        "moon",
        "progress",
        "stopwatch",
        "counter",
        "word_clock",
        "phrases",
        "hass_entity",
        "hass_graph",
        "hass_entities",
        "hass_picture",
        "hass_weather",
        "hass_template",
        "hass_tile",
    ];

    fn text(content: &str, size: f32) -> Value {
        json!({ "type": "text", "content": content, "font_family": "Inter", "font_weight": 700, "size": size, "color": { "r": 255, "g": 255, "b": 255 }, "position": "middle", "outline": null })
    }

    fn widget_layer(kind: &str) -> Value {
        json!({ "type": "widget", "widget": new_widget(kind) })
    }

    fn key(kind: Option<&str>, layers: Value, actions: Value) -> Value {
        json!({
            "kind": kind,
            "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": layers },
            "actions": actions,
            "double_press": [],
            "long_press": [],
            "second_state": null,
        })
    }

    fn check(setup: &Setup, key: &Value) -> Result<(), Problem> {
        let key: Key = serde_json::from_value(key.clone()).unwrap();
        check_key(&key, &setup.assets)
    }

    fn message(setup: &Setup, key: Value) -> String {
        check(setup, &key).unwrap_err().to_string()
    }

    fn tile(entity: &str) -> Value {
        json!({ "type": "home_assistant", "server": uuid::Uuid::nil(), "command": { "kind": "tile", "entity": entity, "press": "auto" } })
    }

    fn plugin() -> Value {
        json!({ "type": "plugin", "action": "com.a.b.c.act", "id": uuid::Uuid::new_v4(), "settings": {} })
    }

    fn with_second_state(mut key: Value, actions: Value) -> Value {
        key["second_state"] = json!({ "appearance": key["appearance"], "actions": actions });
        key
    }

    #[test]
    fn keys_as_the_editor_sets_them_up_pass() {
        let setup = setup();
        let ok = |key: Value| {
            if let Err(problem) = check(&setup, &key) {
                panic!("{problem}: {key}");
            }
        };
        ok(key(Some("basic"), json!([text("Nueva", 30.0)]), json!([])));
        ok(key(
            Some("page_indicator"),
            json!([text("{page_number}/{page_count}", 40.0)]),
            json!([]),
        ));
        for kind in WIDGET_KINDS {
            ok(key(Some("widget"), json!([widget_layer(kind)]), json!([])));
        }
        ok(key(
            Some("carousel"),
            json!([widget_layer("digital_clock"), widget_layer("calendar")]),
            json!([]),
        ));
        let icon = json!({ "type": "image", "asset": setup.icon, "fit": "contain", "inset": 0.22, "tint": { "r": 255, "g": 255, "b": 255 }, "icon": "mdi:home", "source": null, "crop": null });
        let mains = [
            json!({ "type": "open_url", "url": "https://example.com" }),
            json!({ "type": "hotkey", "keys": { "key": "KeyC", "command": true, "shift": false, "option": false, "control": false } }),
            json!({ "type": "type_text", "text": "hola", "enter": false, "paste": false }),
            json!({ "type": "play_sound", "sound": setup.sound, "name": "clic.wav", "volume": 80, "mode": "restart", "device": null }),
            json!({ "type": "random", "actions": [{ "type": "stop_sounds" }, { "type": "media", "key": "next" }] }),
            json!({ "type": "back" }),
            plugin(),
            tile("light.salon"),
        ];
        for main in mains {
            ok(key(
                Some("action"),
                json!([icon, text("Abrir", 30.0)]),
                json!([main, { "type": "delay", "ms": 500 }, { "type": "stop_sounds" }]),
            ));
        }

        let mut dressed = new_widget("hass_tile");
        dressed["entity"] = json!("light.salon");
        dressed["icons"] = json!({ "": setup.icon, "off": setup.icon });
        dressed["label"] = json!("Lámpara del salón de la planta de arriba");
        ok(key(
            Some("action"),
            json!([{ "type": "widget", "widget": dressed }]),
            json!([tile("light.salon")]),
        ));

        let switch = json!({ "type": "home_assistant", "server": uuid::Uuid::nil(), "command": { "kind": "switch", "entity": "light.salon", "mode": "toggle" } });
        ok(with_second_state(
            key(Some("action"), json!([icon]), json!([switch])),
            json!([switch]),
        ));

        let hotkey = json!({ "type": "hotkey", "keys": { "key": "KeyM", "command": true, "shift": true, "option": false, "control": false } });
        ok(with_second_state(
            key(
                Some("two_states"),
                json!([text("Mic", 30.0)]),
                json!([hotkey]),
            ),
            json!([hotkey]),
        ));

        ok(key(
            Some("action"),
            json!([text("Página 2", 30.0)]),
            json!([{ "type": "open_page", "page": uuid::Uuid::new_v4() }]),
        ));
    }

    #[test]
    fn the_keys_the_app_creates_pass() {
        let setup = setup();
        let config = first_run(&setup.assets).unwrap();
        for key in config.profiles[0].pages[0].keys.values() {
            check_key(key, &setup.assets).unwrap();
        }
        let back = back_key(&setup.assets, &NewKeyStyle::default()).unwrap();
        check_key(&back, &setup.assets).unwrap();
        let style = Preferences::default().new_keys;
        assert!((10.0..=80.0).contains(&style.text_size));
    }

    #[test]
    fn keys_saved_before_kinds_are_read_as_the_editor_reads_them() {
        let setup = setup();
        let ok = |key: Value| check(&setup, &key).unwrap();
        ok(key(None, json!([]), json!([])));
        ok(key(
            None,
            json!([widget_layer("timer")]),
            json!([{ "type": "stop_sounds" }]),
        ));
        ok(key(
            None,
            json!([widget_layer("calendar"), widget_layer("moon")]),
            json!([]),
        ));
        ok(key(
            None,
            json!([]),
            json!([{ "type": "stop_sounds" }, plugin(), { "type": "back" }]),
        ));
        assert!(
            check(
                &setup,
                &key(None, json!([]), json!([tile("light.a"), plugin()]))
            )
            .is_err(),
            "only one action can be what the key is"
        );
    }

    #[test]
    fn the_editor_forgives_an_empty_action_key_and_a_widget_key_with_several() {
        let setup = setup();
        check(&setup, &key(Some("action"), json!([]), json!([]))).unwrap();
        let several = |first: &str| {
            key(
                Some("widget"),
                json!([widget_layer(first), widget_layer("moon")]),
                json!([]),
            )
        };
        check(&setup, &several("calendar")).unwrap();
        let problem = message(&setup, several("timer"));
        assert!(
            problem.contains("a timer widget answers presses"),
            "{problem}"
        );
    }

    #[test]
    fn a_kind_agrees_with_what_the_key_holds() {
        let setup = setup();
        assert_eq!(
            message(
                &setup,
                key(Some("widget"), json!([text("x", 30.0)]), json!([]))
            ),
            "a widget key needs one widget layer, got none"
        );
        assert_eq!(
            message(&setup, key(Some("carousel"), json!([]), json!([]))),
            "a carousel key needs widget layers, got none"
        );
        assert!(
            check(
                &setup,
                &key(Some("carousel"), json!([widget_layer("moon")]), json!([]))
            )
            .is_ok(),
            "the editor leaves a carousel with one widget when the others are removed"
        );
        assert!(
            message(
                &setup,
                key(Some("basic"), json!([widget_layer("moon")]), json!([]))
            )
            .starts_with("a basic key shows no widgets, got 1")
        );
        assert!(
            message(
                &setup,
                key(
                    Some("page_indicator"),
                    json!([widget_layer("moon")]),
                    json!([])
                )
            )
            .starts_with("a page_indicator key shows no widgets")
        );
        assert_eq!(
            message(&setup, key(Some("two_states"), json!([]), json!([]))),
            "a two_states key needs second_state"
        );
        let basic = with_second_state(key(Some("basic"), json!([]), json!([])), json!([]));
        assert!(message(&setup, basic).starts_with("a basic key has no second_state"));
    }

    #[test]
    fn what_draws_the_key_is_only_the_first_action_of_an_action_key() {
        let setup = setup();
        assert_eq!(
            message(
                &setup,
                key(
                    Some("action"),
                    json!([]),
                    json!([{ "type": "back" }, tile("light.a")])
                )
            ),
            "actions[1] (home_assistant) draws the key, so it can only be actions[0] of a key \
             of kind action without second_state; this key is action"
        );
        assert!(
            message(&setup, key(Some("basic"), json!([]), json!([plugin()])))
                .ends_with("this key is basic")
        );
        let mut long = key(Some("action"), json!([]), json!([{ "type": "back" }]));
        long["long_press"] = json!([tile("light.a")]);
        assert!(message(&setup, long).starts_with("long_press[0] (home_assistant) draws the key"));
        let show = json!({ "type": "home_assistant", "server": uuid::Uuid::nil(), "command": { "kind": "show", "entity": "sensor.t" } });
        let two = with_second_state(key(Some("two_states"), json!([]), json!([show])), json!([]));
        assert!(message(&setup, two).starts_with("actions[0] (home_assistant) draws the key"));
    }

    #[test]
    fn looks_and_actions_say_where_the_problem_is() {
        let setup = setup();
        let mut monitor = new_widget("system_monitor");
        monitor["metrics"] = json!(["cpu", "ram", "gpu", "disk", "cpu"]);
        assert_eq!(
            message(
                &setup,
                key(
                    Some("widget"),
                    json!([{ "type": "widget", "widget": monitor }]),
                    json!([])
                )
            ),
            "appearance: layers[0].widget (system_monitor): metrics must be 1 to 4 different \
             values of \"cpu\", \"ram\", \"gpu\", \"disk\", got \
             [\"cpu\",\"ram\",\"gpu\",\"disk\",\"cpu\"]"
        );
        let mut two = with_second_state(
            key(Some("two_states"), json!([]), json!([])),
            json!([{ "type": "open_url", "url": "x" }]),
        );
        two["second_state"]["appearance"]["layers"] = json!([text("x", 90.0)]);
        assert_eq!(
            message(&setup, two.clone()),
            "second_state.appearance: layers[0] (text): size must be 10 to 80, got 90"
        );
        two["second_state"]["appearance"]["layers"] = json!([]);
        assert!(message(&setup, two).starts_with("second_state.actions[0] (open_url): url must"));
        let missing = format!("{}.png", "0".repeat(64));
        assert_eq!(
            message(
                &setup,
                key(
                    Some("basic"),
                    json!([{ "type": "image", "asset": missing, "fit": "cover", "inset": 0, "tint": null, "icon": null, "source": null, "crop": null }]),
                    json!([])
                )
            ),
            format!(
                "appearance: layers[0] (image): asset \"{missing}\" is not an image of the asset \
                 store; upload it first"
            )
        );
    }
}
