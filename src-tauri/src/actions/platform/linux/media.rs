use evdev::KeyCode;

use super::virtual_keyboard::with_keyboard;
use crate::actions::error::ActionError;
use crate::config::MediaKey;

/// The desktop answers these keys as it does those of a laptop keyboard.
pub fn media_key(key: MediaKey) -> Result<(), ActionError> {
    let code = match key {
        MediaKey::VolumeUp => KeyCode::KEY_VOLUMEUP,
        MediaKey::VolumeDown => KeyCode::KEY_VOLUMEDOWN,
        MediaKey::Mute => KeyCode::KEY_MUTE,
        MediaKey::PlayPause => KeyCode::KEY_PLAYPAUSE,
        MediaKey::Next => KeyCode::KEY_NEXTSONG,
        MediaKey::Previous => KeyCode::KEY_PREVIOUSSONG,
        MediaKey::BrightnessUp => KeyCode::KEY_BRIGHTNESSUP,
        MediaKey::BrightnessDown => KeyCode::KEY_BRIGHTNESSDOWN,
    };
    with_keyboard(|keyboard| keyboard.chord(&[], code))
}
