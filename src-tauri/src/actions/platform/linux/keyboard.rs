use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use evdev::KeyCode;

use super::keycodes;
use super::keymap::Layout;
use super::virtual_keyboard::with_keyboard;
use crate::actions::error::ActionError;
use crate::config::KeyCombo;

/// Long enough for the target app to read the clipboard before it is put back.
const BEFORE_RESTORING_CLIPBOARD: Duration = Duration::from_millis(400);

/// Under X11 the clipboard lives in the app that set it, so it is kept for the app's lifetime.
static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

/// Key positions, so the shortcut is the same keys whatever the keyboard layout.
pub fn press_combo(keys: &KeyCombo) -> Result<(), ActionError> {
    let key = keycodes::code(&keys.key).ok_or_else(|| ActionError::UnknownKey(keys.key.clone()))?;
    let modifiers: Vec<KeyCode> = [
        (keys.control, KeyCode::KEY_LEFTCTRL),
        (keys.shift, KeyCode::KEY_LEFTSHIFT),
        (keys.option, KeyCode::KEY_LEFTALT),
        (keys.command, KeyCode::KEY_LEFTMETA),
    ]
    .into_iter()
    .filter_map(|(on, code)| on.then_some(code))
    .collect();
    with_keyboard(|keyboard| keyboard.chord(&modifiers, key))
}

/// Text with characters on no key of the layout (emoji, other alphabets) is pasted instead.
pub fn type_text(text: &str, enter: bool, paste: bool) -> Result<(), ActionError> {
    let plan = if paste {
        None
    } else {
        Layout::current().and_then(|layout| layout.plan(text))
    };
    match plan {
        Some(strokes) => with_keyboard(|keyboard| {
            for stroke in strokes {
                let mut held = Vec::new();
                if stroke.shift {
                    held.push(KeyCode::KEY_LEFTSHIFT);
                }
                if stroke.alt_gr {
                    held.push(KeyCode::KEY_RIGHTALT);
                }
                keyboard.chord(&held, stroke.key)?;
            }
            Ok(())
        })?,
        None => paste_text(text)?,
    }
    if enter {
        with_keyboard(|keyboard| keyboard.chord(&[], KeyCode::KEY_ENTER))?;
    }
    Ok(())
}

/// Puts the text on the clipboard, presses Ctrl+V and then puts back the text that was there.
fn paste_text(text: &str) -> Result<(), ActionError> {
    let mut clipboard = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
    if clipboard.is_none() {
        *clipboard =
            Some(arboard::Clipboard::new().map_err(|err| ActionError::Clipboard(err.to_string()))?);
    }
    let clipboard = clipboard.as_mut().expect("created above");
    let previous = clipboard.get_text().ok();
    clipboard
        .set_text(text)
        .map_err(|err| ActionError::Clipboard(err.to_string()))?;
    with_keyboard(|keyboard| keyboard.chord(&[KeyCode::KEY_LEFTCTRL], KeyCode::KEY_V))?;
    thread::sleep(BEFORE_RESTORING_CLIPBOARD);
    if let Some(previous) = previous {
        let _ = clipboard.set_text(previous);
    }
    Ok(())
}
