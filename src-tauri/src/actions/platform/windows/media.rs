use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_VOLUME_DOWN, VK_VOLUME_MUTE,
    VK_VOLUME_UP,
};

use super::brightness;
use super::keyboard::{send, virtual_key};
use crate::actions::error::ActionError;
use crate::config::MediaKey;

pub fn media_key(key: MediaKey) -> Result<(), ActionError> {
    let key = match key {
        MediaKey::VolumeUp => VK_VOLUME_UP,
        MediaKey::VolumeDown => VK_VOLUME_DOWN,
        MediaKey::Mute => VK_VOLUME_MUTE,
        MediaKey::PlayPause => VK_MEDIA_PLAY_PAUSE,
        MediaKey::Next => VK_MEDIA_NEXT_TRACK,
        MediaKey::Previous => VK_MEDIA_PREV_TRACK,
        MediaKey::BrightnessUp => return brightness::step(true),
        MediaKey::BrightnessDown => return brightness::step(false),
    };
    send(&[virtual_key(key, false), virtual_key(key, true)])
}
