use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_core_graphics::{CGEvent, CGEventTapLocation};
use objc2_foundation::NSPoint;

use super::accessibility;
use crate::actions::error::ActionError;
use crate::config::MediaKey;

/// Media keys reach macOS as "system defined" events of this subtype, with the key in `data1`.
const AUX_CONTROL_BUTTON: i16 = 8;
const KEY_DOWN: isize = 0xa;
const KEY_UP: isize = 0xb;

pub fn media_key(key: MediaKey) -> Result<(), ActionError> {
    accessibility::require()?;
    let code = match key {
        MediaKey::VolumeUp => 0,
        MediaKey::VolumeDown => 1,
        MediaKey::BrightnessUp => 2,
        MediaKey::BrightnessDown => 3,
        MediaKey::Mute => 7,
        MediaKey::PlayPause => 16,
        MediaKey::Next => 17,
        MediaKey::Previous => 18,
    };
    for state in [KEY_DOWN, KEY_UP] {
        let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
            NSEventType::SystemDefined,
            NSPoint::new(0.0, 0.0),
            NSEventModifierFlags((state << 8) as usize),
            0.0,
            0,
            None,
            AUX_CONTROL_BUTTON,
            (code << 16) | (state << 8),
            -1,
        );
        if let Some(event) = event.and_then(|e| e.CGEvent()) {
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        }
    }
    Ok(())
}
