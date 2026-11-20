//! What the settings offer. A value saved before the options changed stays on offer, as the
//! settings show it, so a value equal to the current one always passes.

use super::appearance::{FONT_WEIGHTS, TEXT_SIZE};
use super::asset_lookup::Assets;
use super::problem::{Problem, fail, within_range};
use crate::config::integrations::Integration;
use crate::config::model::DeviceSettings;
use crate::config::preferences::{Preferences, ScreensaverImage};
use crate::hass::Address;

const SLEEP_MINUTES: [u32; 7] = [0, 1, 5, 10, 15, 30, 60];
const IDLE_MINUTES: [u32; 8] = [0, 1, 2, 5, 10, 15, 30, 60];
const OFF_AFTER_MINUTES: [u32; 7] = [0, 5, 10, 15, 30, 60, 120];
const BRIGHTNESS: (u8, u8) = (0, 100);
const SCREENSAVER_BRIGHTNESS: (u8, u8) = (5, 100);
const LONG_PRESS_MS: (u32, u32) = (250, 2000);
const DOUBLE_PRESS_MS: (u32, u32) = (150, 800);

pub fn check_device(new: &DeviceSettings, current: &DeviceSettings) -> Result<(), Problem> {
    in_range("brightness", new.brightness, current.brightness, BRIGHTNESS)?;
    one_of(
        "sleep_minutes",
        new.sleep_minutes,
        current.sleep_minutes,
        &SLEEP_MINUTES,
    )
}

pub fn check_preferences(
    new: &Preferences,
    current: &Preferences,
    assets: &impl Assets,
) -> Result<(), Problem> {
    let (gestures, before) = (&new.gestures, &current.gestures);
    in_range(
        "gestures.long_press_ms",
        gestures.long_press_ms,
        before.long_press_ms,
        LONG_PRESS_MS,
    )?;
    in_range(
        "gestures.double_press_ms",
        gestures.double_press_ms,
        before.double_press_ms,
        DOUBLE_PRESS_MS,
    )?;

    let (keys, before) = (&new.new_keys, &current.new_keys);
    in_range(
        "new_keys.text_size",
        keys.text_size,
        before.text_size,
        TEXT_SIZE,
    )?;
    one_of(
        "new_keys.font_weight",
        keys.font_weight,
        before.font_weight,
        &FONT_WEIGHTS,
    )?;
    if keys.font_family.trim().is_empty() {
        return fail("new_keys.font_family cannot be empty");
    }

    let (saver, before) = (&new.screensaver, &current.screensaver);
    one_of(
        "screensaver.idle_minutes",
        saver.idle_minutes,
        before.idle_minutes,
        &IDLE_MINUTES,
    )?;
    one_of(
        "screensaver.off_after_minutes",
        saver.off_after_minutes,
        before.off_after_minutes,
        &OFF_AFTER_MINUTES,
    )?;
    in_range(
        "screensaver.brightness",
        saver.brightness,
        before.brightness,
        SCREENSAVER_BRIGHTNESS,
    )?;
    if let ScreensaverImage::Custom { asset, source, .. } = &saver.image
        && saver.image != before.image
    {
        for (field, id) in [("asset", Some(asset)), ("source", source.as_ref())] {
            if let Some(id) = id
                && !assets.has_image(id)
            {
                return fail(format!(
                    "screensaver.image.{field} {id:?} is not an image of the asset store; \
                     upload it first"
                ));
            }
        }
    }
    Ok(())
}

/// A server of OBS takes any host and port, an empty host or port 0 meaning OBS's own; a Home
/// Assistant needs an address the app can reach it at.
pub fn check_integration(integration: &Integration) -> Result<(), Problem> {
    if integration.name().trim().is_empty() {
        return fail("name cannot be empty");
    }
    if let Integration::HomeAssistant { url, .. } = integration
        && Address::parse(url).is_none()
    {
        return fail(format!(
            "url must be the address of Home Assistant as a browser opens it, such as \
             http://homeassistant.local:8123, got {url:?}"
        ));
    }
    Ok(())
}

fn in_range<T: PartialOrd + Copy + std::fmt::Display>(
    field: &str,
    value: T,
    current: T,
    range: (T, T),
) -> Result<(), Problem> {
    if value == current {
        return Ok(());
    }
    within_range(field, value, range)
}

fn one_of<T: PartialEq + std::fmt::Debug + std::fmt::Display>(
    field: &str,
    value: T,
    current: T,
    options: &[T],
) -> Result<(), Problem> {
    if value != current && !options.contains(&value) {
        return fail(format!("{field} must be one of {options:?}, got {value}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::assets::AssetStore;
    use uuid::Uuid;

    use super::*;
    use crate::render::ImageFormat;

    fn assets() -> (tempfile::TempDir, AssetStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = AssetStore::open(dir.path().to_path_buf()).unwrap();
        (dir, store)
    }

    fn device(brightness: u8, sleep_minutes: u32) -> DeviceSettings {
        DeviceSettings {
            brightness,
            sleep_minutes,
        }
    }

    #[test]
    fn the_dock_sleeps_after_one_of_the_times_offered() {
        let current = device(70, 0);
        assert!(check_device(&device(0, 60), &current).is_ok());
        assert!(check_device(&device(100, 15), &current).is_ok());
        assert_eq!(
            check_device(&device(70, 7), &current)
                .unwrap_err()
                .to_string(),
            "sleep_minutes must be one of [0, 1, 5, 10, 15, 30, 60], got 7"
        );
        assert!(check_device(&device(101, 0), &current).is_err());
        assert!(
            check_device(&device(40, 7), &device(70, 7)).is_ok(),
            "a time saved before the options changed stays"
        );
    }

    #[test]
    fn preferences_take_what_the_settings_offer() {
        let (_dir, assets) = assets();
        let current = Preferences::default();
        let check = |change: &dyn Fn(&mut Preferences)| {
            let mut new = current.clone();
            change(&mut new);
            check_preferences(&new, &current, &assets)
        };
        assert!(check(&|_| {}).is_ok());
        assert!(check(&|p| p.gestures.long_press_ms = 2000).is_ok());
        assert!(check(&|p| p.gestures.long_press_ms = 200).is_err());
        assert!(check(&|p| p.gestures.double_press_ms = 900).is_err());
        assert!(check(&|p| p.new_keys.text_size = 80.0).is_ok());
        assert!(check(&|p| p.new_keys.text_size = 96.0).is_err());
        assert!(check(&|p| p.new_keys.font_weight = 100).is_err());
        assert!(check(&|p| p.new_keys.font_family = String::new()).is_err());
        assert!(check(&|p| p.screensaver.idle_minutes = 2).is_ok());
        assert_eq!(
            check(&|p| p.screensaver.idle_minutes = 3)
                .unwrap_err()
                .to_string(),
            "screensaver.idle_minutes must be one of [0, 1, 2, 5, 10, 15, 30, 60], got 3"
        );
        assert!(check(&|p| p.screensaver.off_after_minutes = 120).is_ok());
        assert!(check(&|p| p.screensaver.off_after_minutes = 1).is_err());
        assert!(check(&|p| p.screensaver.brightness = 5).is_ok());
        assert!(check(&|p| p.screensaver.brightness = 0).is_err());

        let mut legacy = current.clone();
        legacy.screensaver.idle_minutes = 3;
        legacy.screensaver.brightness = 0;
        let mut language = legacy.clone();
        language.language = crate::i18n::LanguageChoice::En;
        assert!(
            check_preferences(&language, &legacy, &assets).is_ok(),
            "values saved before stay while others change"
        );
    }

    #[test]
    fn a_screensaver_image_of_the_users_is_in_the_store() {
        let (_dir, assets) = assets();
        let id = assets.put(ImageFormat::Jpeg, b"jpg").unwrap();
        let current = Preferences::default();
        let with = |asset: &str| {
            let mut new = current.clone();
            new.screensaver.image = ScreensaverImage::Custom {
                asset: asset.into(),
                source: None,
                crop: None,
            };
            new
        };
        assert!(check_preferences(&with(&id), &current, &assets).is_ok());
        assert!(check_preferences(&with("nope.jpg"), &current, &assets).is_err());
    }

    #[test]
    fn servers_have_a_name_and_home_assistant_an_address() {
        let hass = |name: &str, url: &str| Integration::HomeAssistant {
            id: Uuid::new_v4(),
            name: name.into(),
            url: url.into(),
            insecure: false,
        };
        for url in [
            "http://homeassistant.local:8123",
            "https://abc.ui.nabu.casa/",
            "192.168.1.10:8123",
        ] {
            assert!(check_integration(&hass("Casa", url)).is_ok(), "{url}");
        }
        for url in ["", "ftp://ha.local", "http://ha:x", "casa de campo"] {
            assert!(check_integration(&hass("Casa", url)).is_err(), "{url}");
        }
        assert_eq!(
            check_integration(&hass(" ", "ha.local"))
                .unwrap_err()
                .to_string(),
            "name cannot be empty"
        );
        let obs = Integration::Obs {
            id: Uuid::new_v4(),
            name: "OBS".into(),
            host: String::new(),
            port: 0,
        };
        assert!(
            check_integration(&obs).is_ok(),
            "OBS's own host and port when left empty"
        );
    }
}
