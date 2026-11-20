//! What the editor lets an action hold, beyond what the engine needs to run it.

use super::asset_lookup::Assets;
use super::problem::{Problem, fail, tag};
use crate::config::actions::{Action, KeyCombo};
use crate::config::hass::HassCommand;

/// Keys that only change others (`KeyboardEvent.code` names): a shortcut ends in another one.
const MODIFIERS: [&str; 10] = [
    "MetaLeft",
    "MetaRight",
    "ShiftLeft",
    "ShiftRight",
    "AltLeft",
    "AltRight",
    "ControlLeft",
    "ControlRight",
    "CapsLock",
    "Fn",
];

/// Actions that draw the key they are on, so they can only be what the key is.
pub(super) fn draws_key(action: &Action) -> bool {
    matches!(
        action,
        Action::Plugin { .. }
            | Action::HomeAssistant {
                command: HassCommand::Tile { .. } | HassCommand::Show { .. },
                ..
            }
    )
}

/// `place` names the list in the key, such as `actions` or `second_state.actions`.
pub(super) fn check_actions(
    place: &str,
    list: &[Action],
    assets: &impl Assets,
) -> Result<(), Problem> {
    for (i, action) in list.iter().enumerate() {
        check_action(action, assets)
            .map_err(|problem| problem.within(format!("{place}[{i}] ({})", tag(action, "type"))))?;
    }
    Ok(())
}

fn check_action(action: &Action, assets: &impl Assets) -> Result<(), Problem> {
    match action {
        Action::OpenUrl { url } if !is_web_url(url) => fail(format!(
            "url must start with http:// or https:// and have no spaces, got {url:?}"
        )),
        Action::PlaySound { sound, .. } if !assets.has_sound(sound) => fail(format!(
            "sound {sound:?} is not a sound of the asset store; upload it first"
        )),
        Action::Hotkey { keys } => check_hotkey(keys),
        Action::Random { actions } => {
            if let Some(i) = actions.iter().position(draws_key) {
                return fail(format!(
                    "actions[{i}] ({}) draws the key and cannot be one of the picks",
                    tag(&actions[i], "type")
                ));
            }
            check_actions("actions", actions, assets)
        }
        _ => Ok(()),
    }
}

fn is_web_url(url: &str) -> bool {
    let url = url.trim();
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .is_some_and(|rest| !rest.is_empty() && !rest.contains(char::is_whitespace))
}

fn check_hotkey(keys: &KeyCombo) -> Result<(), Problem> {
    if MODIFIERS.contains(&keys.key.as_str()) {
        return fail(format!(
            "keys.key must be the key the shortcut ends in, not a modifier, got {:?}; set \
             command, shift, option and control for the modifiers",
            keys.key
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::assets::AssetStore;
    use crate::config::{HassPress, SoundFormat};

    fn assets() -> (tempfile::TempDir, AssetStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = AssetStore::open(dir.path().to_path_buf()).unwrap();
        (dir, store)
    }

    fn check(action: Action, assets: &AssetStore) -> Result<(), Problem> {
        check_actions("actions", &[action], assets)
    }

    #[test]
    fn links_are_web_addresses() {
        let (_dir, assets) = assets();
        let url = |url: &str| Action::OpenUrl { url: url.into() };
        assert!(check(url("https://example.com/a?b=1"), &assets).is_ok());
        assert!(check(url(" http://192.168.1.5:8123 "), &assets).is_ok());
        for wrong in [
            "",
            "example.com",
            "ftp://example.com",
            "https://",
            "https://a b",
        ] {
            assert!(check(url(wrong), &assets).is_err(), "{wrong}");
        }
        assert_eq!(
            check(url("example.com"), &assets).unwrap_err().to_string(),
            "actions[0] (open_url): url must start with http:// or https:// and have no \
             spaces, got \"example.com\""
        );
    }

    #[test]
    fn sounds_are_in_the_store() {
        let (_dir, assets) = assets();
        let sound = |sound: &str| Action::PlaySound {
            sound: sound.into(),
            name: "clic.wav".into(),
            volume: 80,
            mode: crate::config::SoundMode::Restart,
            device: None,
        };
        let stored = assets.put_sound(SoundFormat::Wav, b"RIFF").unwrap();
        let image = assets
            .put(crate::render::ImageFormat::Svg, b"<svg/>")
            .unwrap();
        assert!(check(sound(&stored), &assets).is_ok());
        assert!(check(sound(""), &assets).is_err());
        assert!(check(sound(&image), &assets).is_err(), "an image");
        assert!(check(sound(&format!("{}.wav", "0".repeat(64))), &assets).is_err());
    }

    #[test]
    fn shortcuts_end_in_a_key_that_is_not_a_modifier() {
        let (_dir, assets) = assets();
        let hotkey = |key: &str| Action::Hotkey {
            keys: KeyCombo {
                key: key.into(),
                command: true,
                shift: false,
                option: false,
                control: false,
            },
        };
        assert!(check(hotkey("KeyC"), &assets).is_ok());
        assert!(check(hotkey("IntlRo"), &assets).is_ok(), "any key recorded");
        assert!(check(hotkey("ShiftLeft"), &assets).is_err());
        assert!(check(hotkey("Fn"), &assets).is_err());
    }

    #[test]
    fn random_picks_never_draw_the_key_and_follow_the_same_rules() {
        let (_dir, assets) = assets();
        let random = |actions| Action::Random { actions };
        let tile = Action::HomeAssistant {
            server: uuid::Uuid::nil(),
            command: HassCommand::Tile {
                entity: "light.a".into(),
                press: HassPress::Auto,
            },
        };
        assert!(check(random(vec![tile]), &assets).is_err());
        let problem = check(
            random(vec![
                Action::StopSounds,
                Action::OpenUrl { url: "x".into() },
            ]),
            &assets,
        )
        .unwrap_err();
        assert!(
            problem
                .to_string()
                .starts_with("actions[0] (random): actions[1] (open_url): url must"),
            "{problem}"
        );
    }
}
