//! How the app behaves, apart from the keys: gesture timing, the look of new keys and what the
//! dock shows while the Mac is locked or the dock is left alone.

use serde::{Deserialize, Serialize};

use super::linked_apps::ProfileSwitching;
use super::model::Crop;
use crate::i18n::LanguageChoice;
use crate::render::{Color, DEFAULT_FAMILY, VerticalPosition};

const LONG_PRESS_MS: (u32, u32) = (250, 2000);
const DOUBLE_PRESS_MS: (u32, u32) = (150, 800);
const TEXT_SIZE: (f32, f32) = (8.0, 96.0);
const MAX_MINUTES: u32 = 24 * 60;

/// Every field has a default, so configurations saved before a field existed still load.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct Preferences {
    /// The language of the app; the Mac's unless the user picks one.
    pub language: LanguageChoice,
    pub gestures: GestureTiming,
    pub new_keys: NewKeyStyle,
    pub screensaver: ScreensaverSettings,
    pub profiles: ProfileSwitching,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct GestureTiming {
    /// Held this long, a key with a long press does that instead of its press.
    pub long_press_ms: u32,
    /// A second press within this long is a double press; keys with one wait this long to act.
    pub double_press_ms: u32,
}

impl Default for GestureTiming {
    fn default() -> Self {
        Self {
            long_press_ms: 500,
            double_press_ms: 300,
        }
    }
}

/// How a key looks when it is created, from the inspector, a line on the canvas or a new page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct NewKeyStyle {
    pub background: Color,
    pub text_color: Color,
    pub font_family: String,
    pub font_weight: u16,
    #[specta(type = specta_typescript::Number)]
    pub text_size: f32,
    pub text_position: VerticalPosition,
}

impl Default for NewKeyStyle {
    fn default() -> Self {
        Self {
            background: Color::BLACK,
            text_color: Color::WHITE,
            font_family: DEFAULT_FAMILY.into(),
            font_weight: 700,
            text_size: 30.0,
            text_position: VerticalPosition::Middle,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct ScreensaverSettings {
    pub on_lock: LockAction,
    #[serde(deserialize_with = "or_default")]
    #[specta(type = ScreensaverImage)]
    pub image: ScreensaverImage,
    /// Minutes without touching the dock before the screensaver starts; `0` never.
    pub idle_minutes: u32,
    /// Minutes of screensaver before the screen goes dark; `0` never.
    pub off_after_minutes: u32,
    pub brightness: u8,
}

impl Default for ScreensaverSettings {
    fn default() -> Self {
        Self {
            on_lock: LockAction::Screensaver,
            image: ScreensaverImage::default(),
            idle_minutes: 0,
            off_after_minutes: 30,
            brightness: 50,
        }
    }
}

/// What the dock does while the Mac is locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LockAction {
    /// Keeps showing the keys, which go on working.
    Keep,
    /// Plays the screensaver; the keys only make it react.
    #[default]
    Screensaver,
    /// Turns the screen dark.
    Off,
}

/// The still image of the screensaver, over the whole panel. The dock keeps a copy and shows it
/// by itself while the app is away.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScreensaverImage {
    /// One of the pictures the app draws.
    Scene { scene: Scene },
    /// An image from the store, covering the panel.
    Custom {
        asset: String,
        /// Original upload when `asset` is an edited copy of it.
        #[serde(default)]
        source: Option<String>,
        /// How `source` was framed to produce `asset`, so the editor can reopen it.
        #[serde(default)]
        crop: Option<Crop>,
    },
}

/// A picture the app no longer has, or a shape it no longer reads, falls back to the default
/// instead of failing the whole configuration.
fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Lenient<T> {
        Known(T),
        Unknown(serde::de::IgnoredAny),
    }
    Ok(match Lenient::deserialize(deserializer)? {
        Lenient::Known(value) => value,
        Lenient::Unknown(_) => T::default(),
    })
}

impl Default for ScreensaverImage {
    fn default() -> Self {
        Self::Scene {
            scene: Scene::default(),
        }
    }
}

/// The app's own pictures for the screensaver, over the whole panel.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "snake_case")]
pub enum Scene {
    #[default]
    Sunset,
    Aurora,
    Ocean,
    Neon,
    Mountains,
    Graphite,
}

impl Scene {
    pub const ALL: [Self; 6] = [
        Self::Sunset,
        Self::Aurora,
        Self::Ocean,
        Self::Neon,
        Self::Mountains,
        Self::Graphite,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Sunset => "sunset",
            Self::Aurora => "aurora",
            Self::Ocean => "ocean",
            Self::Neon => "neon",
            Self::Mountains => "mountains",
            Self::Graphite => "graphite",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scene| scene.id() == id)
    }
}

impl Preferences {
    /// Keeps every value within what the engine and the dock can do.
    pub fn clamped(mut self) -> Self {
        let gestures = &mut self.gestures;
        gestures.long_press_ms = gestures
            .long_press_ms
            .clamp(LONG_PRESS_MS.0, LONG_PRESS_MS.1);
        gestures.double_press_ms = gestures
            .double_press_ms
            .clamp(DOUBLE_PRESS_MS.0, DOUBLE_PRESS_MS.1);
        let keys = &mut self.new_keys;
        keys.text_size = keys.text_size.clamp(TEXT_SIZE.0, TEXT_SIZE.1);
        keys.font_weight = keys.font_weight.clamp(100, 900);
        if keys.font_family.trim().is_empty() {
            keys.font_family = DEFAULT_FAMILY.into();
        }
        let saver = &mut self.screensaver;
        saver.idle_minutes = saver.idle_minutes.min(MAX_MINUTES);
        saver.off_after_minutes = saver.off_after_minutes.min(MAX_MINUTES);
        saver.brightness = saver.brightness.min(100);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_their_defaults() {
        let prefs: Preferences =
            serde_json::from_str(r#"{"gestures":{"long_press_ms":800}}"#).unwrap();
        assert_eq!(prefs.gestures.long_press_ms, 800);
        assert_eq!(prefs.gestures.double_press_ms, 300);
        assert_eq!(prefs.screensaver, ScreensaverSettings::default());
    }

    #[test]
    fn screensavers_saved_as_animations_still_load() {
        let prefs: Preferences =
            serde_json::from_str(r#"{"screensaver":{"scene":"clock","idle_minutes":5}}"#).unwrap();
        assert_eq!(prefs.screensaver.idle_minutes, 5);
        assert_eq!(prefs.screensaver.image, ScreensaverImage::default());
    }

    #[test]
    fn a_picture_no_longer_offered_falls_back_to_the_default() {
        let prefs: Preferences = serde_json::from_str(
            r#"{"screensaver":{"image":{"type":"scene","scene":"wave"},"brightness":40}}"#,
        )
        .unwrap();
        assert_eq!(prefs.screensaver.image, ScreensaverImage::default());
        assert_eq!(prefs.screensaver.brightness, 40);
    }

    #[test]
    fn an_image_of_the_users_keeps_its_original_and_crop() {
        let prefs: Preferences = serde_json::from_str(
            r#"{"screensaver":{"image":{"type":"custom","asset":"a","source":"s","crop":{"zoom":1.5,"x":0.1,"y":0,"rotation":90,"flip_x":false,"flip_y":true}}}}"#,
        )
        .unwrap();
        let ScreensaverImage::Custom {
            asset,
            source,
            crop,
        } = prefs.screensaver.image
        else {
            panic!("a custom image");
        };
        assert_eq!((asset.as_str(), source.as_deref()), ("a", Some("s")));
        assert_eq!(crop.map(|c| c.rotation), Some(90.0));
    }

    #[test]
    fn clamps_what_the_engine_cannot_do() {
        let mut prefs = Preferences::default();
        prefs.gestures.long_press_ms = 10;
        prefs.gestures.double_press_ms = 5000;
        prefs.new_keys.font_family = " ".into();
        prefs.screensaver.brightness = 200;
        let prefs = prefs.clamped();
        assert_eq!(prefs.gestures.long_press_ms, 250);
        assert_eq!(prefs.gestures.double_press_ms, 800);
        assert_eq!(prefs.new_keys.font_family, DEFAULT_FAMILY);
        assert_eq!(prefs.screensaver.brightness, 100);
    }

    #[test]
    fn scene_ids_round_trip() {
        for scene in Scene::ALL {
            assert_eq!(Scene::from_id(scene.id()), Some(scene));
            assert_eq!(
                serde_json::to_string(&scene).unwrap(),
                format!("\"{}\"", scene.id())
            );
        }
    }
}
