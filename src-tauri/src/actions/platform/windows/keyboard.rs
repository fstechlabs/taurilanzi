use std::thread;
use std::time::Duration;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY,
};

use super::scancodes::{self, ALT, CONTROL, ENTER, SHIFT, V, WIN};
use crate::actions::error::ActionError;
use crate::config::KeyCombo;

/// Characters sent in one go; apps reading the queue slowly drop keys from longer bursts.
const CHUNK: usize = 32;
const BETWEEN_CHUNKS: Duration = Duration::from_millis(8);
/// Long enough for the target app to read the clipboard before it is put back.
const BEFORE_RESTORING_CLIPBOARD: Duration = Duration::from_millis(400);

/// Scan codes, so the shortcut is the same keys whatever the keyboard layout.
pub fn press_combo(keys: &KeyCombo) -> Result<(), ActionError> {
    let key =
        scancodes::code(&keys.key).ok_or_else(|| ActionError::UnknownKey(keys.key.clone()))?;
    let modifiers: Vec<u16> = [
        (keys.control, CONTROL),
        (keys.shift, SHIFT),
        (keys.option, ALT),
        (keys.command, WIN),
    ]
    .into_iter()
    .filter_map(|(on, code)| on.then_some(code))
    .collect();
    send(&chord(&modifiers, key))
}

pub fn type_text(text: &str, enter: bool, paste: bool) -> Result<(), ActionError> {
    if paste {
        paste_text(text)?;
    } else {
        let mut inputs = Vec::new();
        for c in text.chars() {
            match c {
                '\r' => {}
                '\n' => inputs.extend(tap(ENTER)),
                c => {
                    let mut units = [0u16; 2];
                    for &unit in c.encode_utf16(&mut units).iter() {
                        inputs.push(unicode(unit, false));
                        inputs.push(unicode(unit, true));
                    }
                }
            }
        }
        for chunk in inputs.chunks(CHUNK * 2) {
            send(chunk)?;
            thread::sleep(BETWEEN_CHUNKS);
        }
    }
    if enter {
        send(&tap(ENTER))?;
    }
    Ok(())
}

/// Puts the text on the clipboard, presses Ctrl+V and then puts back the text that was there.
fn paste_text(text: &str) -> Result<(), ActionError> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|err| ActionError::Clipboard(err.to_string()))?;
    let previous = clipboard.get_text().ok();
    clipboard
        .set_text(text)
        .map_err(|err| ActionError::Clipboard(err.to_string()))?;
    send(&chord(&[CONTROL], V))?;
    thread::sleep(BEFORE_RESTORING_CLIPBOARD);
    if let Some(previous) = previous {
        let _ = clipboard.set_text(previous);
    }
    Ok(())
}

fn chord(modifiers: &[u16], key: u16) -> Vec<INPUT> {
    let mut inputs: Vec<INPUT> = modifiers.iter().map(|&m| scan(m, false)).collect();
    inputs.extend(tap(key));
    inputs.extend(modifiers.iter().rev().map(|&m| scan(m, true)));
    inputs
}

fn tap(code: u16) -> [INPUT; 2] {
    [scan(code, false), scan(code, true)]
}

fn scan(code: u16, up: bool) -> INPUT {
    let mut flags = KEYEVENTF_SCANCODE;
    if scancodes::is_extended(code) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    keyboard(VIRTUAL_KEY(0), code & 0xFF, flags)
}

fn unicode(unit: u16, up: bool) -> INPUT {
    let flags = if up {
        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
    } else {
        KEYEVENTF_UNICODE
    };
    keyboard(VIRTUAL_KEY(0), unit, flags)
}

pub(super) fn virtual_key(key: VIRTUAL_KEY, up: bool) -> INPUT {
    let flags = if up {
        KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP
    } else {
        KEYEVENTF_EXTENDEDKEY
    };
    keyboard(key, 0, flags)
}

fn keyboard(key: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Windows refuses the keys while an app running as administrator, or the lock screen, is in
/// front.
pub(super) fn send(inputs: &[INPUT]) -> Result<(), ActionError> {
    // SAFETY: the slice is valid for the call and the size is that of its elements.
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err(ActionError::KeysRefused)
    }
}
