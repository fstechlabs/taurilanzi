use std::thread;
use std::time::Duration;

use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventTapLocation};
use objc2_foundation::NSString;

use super::accessibility;
use super::keycodes::{self, RETURN, V};
use crate::actions::error::ActionError;
use crate::config::KeyCombo;

/// macOS takes at most this many UTF-16 units per synthetic keystroke.
const CHUNK: usize = 20;
const BETWEEN_CHUNKS: Duration = Duration::from_millis(8);
/// Long enough for the target app to read the clipboard before it is put back.
const BEFORE_RESTORING_CLIPBOARD: Duration = Duration::from_millis(400);

pub fn press_combo(keys: &KeyCombo) -> Result<(), ActionError> {
    accessibility::require()?;
    let code =
        keycodes::code(&keys.key).ok_or_else(|| ActionError::UnknownKey(keys.key.clone()))?;
    let mut flags = CGEventFlags::empty();
    for (on, flag) in [
        (keys.command, CGEventFlags::MaskCommand),
        (keys.shift, CGEventFlags::MaskShift),
        (keys.option, CGEventFlags::MaskAlternate),
        (keys.control, CGEventFlags::MaskControl),
    ] {
        if on {
            flags |= flag;
        }
    }
    tap(code, flags);
    Ok(())
}

pub fn type_text(text: &str, enter: bool, paste: bool) -> Result<(), ActionError> {
    accessibility::require()?;
    if paste {
        paste_text(text);
    } else {
        let units: Vec<u16> = text.encode_utf16().collect();
        for chunk in units.chunks(CHUNK) {
            for down in [true, false] {
                if let Some(event) = CGEvent::new_keyboard_event(None, 0, down) {
                    // SAFETY: the pointer and length describe `chunk`, which outlives the call.
                    unsafe {
                        CGEvent::keyboard_set_unicode_string(
                            Some(&event),
                            chunk.len() as _,
                            chunk.as_ptr(),
                        )
                    };
                    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
                }
            }
            thread::sleep(BETWEEN_CHUNKS);
        }
    }
    if enter {
        tap(RETURN, CGEventFlags::empty());
    }
    Ok(())
}

/// Puts the text on the clipboard, presses ⌘V and then puts back the text that was there.
fn paste_text(text: &str) {
    let pasteboard = NSPasteboard::generalPasteboard();
    // SAFETY: NSPasteboardTypeString is a constant provided by AppKit.
    let kind = unsafe { NSPasteboardTypeString };
    let previous = pasteboard.stringForType(kind);
    pasteboard.clearContents();
    pasteboard.setString_forType(&NSString::from_str(text), kind);
    tap(V, CGEventFlags::MaskCommand);
    thread::sleep(BEFORE_RESTORING_CLIPBOARD);
    if let Some(previous) = previous {
        pasteboard.clearContents();
        pasteboard.setString_forType(&previous, kind);
    }
}

fn tap(code: u16, flags: CGEventFlags) {
    for down in [true, false] {
        if let Some(event) = CGEvent::new_keyboard_event(None, code, down) {
            CGEvent::set_flags(Some(&event), flags);
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        }
    }
}
