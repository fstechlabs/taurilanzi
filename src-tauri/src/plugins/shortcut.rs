//! The keyboard shortcuts of Stream Dock's shortcut plugins (Photoshop, Premiere, DaVinci…),
//! which run no code: each action carries in its settings the key to press, as a Windows virtual
//! key (`VKeyCode`) and a macOS one (`VKeyCode_Mac`), with its modifiers, and the host presses it.

use serde_json::Value;

use crate::config::KeyCombo;

/// The shortcut in an action's settings, for this system.
pub fn from_settings(settings: &Value) -> Option<KeyCombo> {
    combo(settings, cfg!(target_os = "macos"))
}

fn combo(settings: &Value, mac: bool) -> Option<KeyCombo> {
    let number = |field: &str| {
        settings
            .get(field)
            .and_then(Value::as_i64)
            .filter(|n| *n >= 0)
    };
    let key = if mac {
        number("VKeyCode_Mac")
            .and_then(mac_key)
            .or_else(|| number("VKeyCode").and_then(windows_key))
    } else {
        number("VKeyCode").and_then(windows_key)
    }?;
    let flag = |field: &str| settings.get(field).and_then(Value::as_bool) == Some(true);
    // Mac shortcuts name their modifiers apart when they differ from Windows'.
    let own = mac
        && settings.as_object().is_some_and(|o| {
            ["KeyCmd_Mac", "KeyCtrl_Mac", "KeyOption_Mac", "KeyShift_Mac"]
                .iter()
                .any(|k| o.contains_key(*k))
        });
    let suffix = if own { "_Mac" } else { "" };
    let held = |name: &str| flag(&format!("Key{name}{suffix}"));
    Some(KeyCombo {
        key: key.into(),
        command: held("Cmd"),
        shift: held("Shift"),
        option: held("Option"),
        control: held("Ctrl") || held("ctrl"),
    })
}

/// Windows virtual keys as `KeyboardEvent.code` names them.
fn windows_key(code: i64) -> Option<&'static str> {
    const LETTERS: [&str; 26] = [
        "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK",
        "KeyL", "KeyM", "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV",
        "KeyW", "KeyX", "KeyY", "KeyZ",
    ];
    const DIGITS: [&str; 10] = [
        "Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8",
        "Digit9",
    ];
    const NUMPAD: [&str; 10] = [
        "Numpad0", "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6", "Numpad7",
        "Numpad8", "Numpad9",
    ];
    const FUNCTIONS: [&str; 20] = [
        "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13", "F14",
        "F15", "F16", "F17", "F18", "F19", "F20",
    ];
    let at = |list: &'static [&'static str], first: i64| {
        usize::try_from(code - first)
            .ok()
            .and_then(|i| list.get(i).copied())
    };
    match code {
        0x41..=0x5A => at(&LETTERS, 0x41),
        0x30..=0x39 => at(&DIGITS, 0x30),
        0x60..=0x69 => at(&NUMPAD, 0x60),
        0x70..=0x83 => at(&FUNCTIONS, 0x70),
        0x08 => Some("Backspace"),
        0x09 => Some("Tab"),
        0x0D => Some("Enter"),
        0x1B => Some("Escape"),
        0x20 => Some("Space"),
        0x21 => Some("PageUp"),
        0x22 => Some("PageDown"),
        0x23 => Some("End"),
        0x24 => Some("Home"),
        0x25 => Some("ArrowLeft"),
        0x26 => Some("ArrowUp"),
        0x27 => Some("ArrowRight"),
        0x28 => Some("ArrowDown"),
        0x2D => Some("Insert"),
        0x2E => Some("Delete"),
        0x6A => Some("NumpadMultiply"),
        0x6B => Some("NumpadAdd"),
        0x6D => Some("NumpadSubtract"),
        0x6E => Some("NumpadDecimal"),
        0x6F => Some("NumpadDivide"),
        0xBA => Some("Semicolon"),
        0xBB => Some("Equal"),
        0xBC => Some("Comma"),
        0xBD => Some("Minus"),
        0xBE => Some("Period"),
        0xBF => Some("Slash"),
        0xC0 => Some("Backquote"),
        0xDB => Some("BracketLeft"),
        0xDC => Some("Backslash"),
        0xDD => Some("BracketRight"),
        0xDE => Some("Quote"),
        _ => None,
    }
}

/// macOS virtual keys (`kVK_*` in Carbon's `Events.h`) as `KeyboardEvent.code` names them.
fn mac_key(code: i64) -> Option<&'static str> {
    Some(match code {
        0x00 => "KeyA",
        0x01 => "KeyS",
        0x02 => "KeyD",
        0x03 => "KeyF",
        0x04 => "KeyH",
        0x05 => "KeyG",
        0x06 => "KeyZ",
        0x07 => "KeyX",
        0x08 => "KeyC",
        0x09 => "KeyV",
        0x0B => "KeyB",
        0x0C => "KeyQ",
        0x0D => "KeyW",
        0x0E => "KeyE",
        0x0F => "KeyR",
        0x10 => "KeyY",
        0x11 => "KeyT",
        0x12 => "Digit1",
        0x13 => "Digit2",
        0x14 => "Digit3",
        0x15 => "Digit4",
        0x16 => "Digit6",
        0x17 => "Digit5",
        0x18 => "Equal",
        0x19 => "Digit9",
        0x1A => "Digit7",
        0x1B => "Minus",
        0x1C => "Digit8",
        0x1D => "Digit0",
        0x1E => "BracketRight",
        0x1F => "KeyO",
        0x20 => "KeyU",
        0x21 => "BracketLeft",
        0x22 => "KeyI",
        0x23 => "KeyP",
        0x24 => "Enter",
        0x25 => "KeyL",
        0x26 => "KeyJ",
        0x27 => "Quote",
        0x28 => "KeyK",
        0x29 => "Semicolon",
        0x2A => "Backslash",
        0x2B => "Comma",
        0x2C => "Slash",
        0x2D => "KeyN",
        0x2E => "KeyM",
        0x2F => "Period",
        0x30 => "Tab",
        0x31 => "Space",
        0x32 => "Backquote",
        0x33 => "Backspace",
        0x35 => "Escape",
        0x41 => "NumpadDecimal",
        0x43 => "NumpadMultiply",
        0x45 => "NumpadAdd",
        0x4B => "NumpadDivide",
        0x4C => "NumpadEnter",
        0x4E => "NumpadSubtract",
        0x51 => "NumpadEqual",
        0x52 => "Numpad0",
        0x53 => "Numpad1",
        0x54 => "Numpad2",
        0x55 => "Numpad3",
        0x56 => "Numpad4",
        0x57 => "Numpad5",
        0x58 => "Numpad6",
        0x59 => "Numpad7",
        0x5B => "Numpad8",
        0x5C => "Numpad9",
        0x60 => "F5",
        0x61 => "F6",
        0x62 => "F7",
        0x63 => "F3",
        0x64 => "F8",
        0x65 => "F9",
        0x67 => "F11",
        0x69 => "F13",
        0x6A => "F16",
        0x6B => "F14",
        0x6D => "F10",
        0x6F => "F12",
        0x71 => "F15",
        0x72 => "Insert",
        0x73 => "Home",
        0x74 => "PageUp",
        0x75 => "Delete",
        0x76 => "F4",
        0x77 => "End",
        0x78 => "F2",
        0x79 => "PageDown",
        0x7A => "F1",
        0x7B => "ArrowLeft",
        0x7C => "ArrowRight",
        0x7D => "ArrowDown",
        0x7E => "ArrowUp",
        0x40 => "F17",
        0x4F => "F18",
        0x50 => "F19",
        0x5A => "F20",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn keys(combo: &KeyCombo) -> (&str, bool, bool, bool, bool) {
        (
            combo.key.as_str(),
            combo.command,
            combo.shift,
            combo.option,
            combo.control,
        )
    }

    #[test]
    fn each_system_presses_its_own_key_and_modifiers() {
        let save = json!({
            "KeyCtrl": true, "KeyCmd": false, "KeyModifiers": 65536, "VKeyCode": 83,
            "KeyCmd_Mac": true, "VKeyCode_Mac": 1, "NativeCode": -1,
        });
        assert_eq!(
            keys(&combo(&save, false).unwrap()),
            ("KeyS", false, false, false, true)
        );
        assert_eq!(
            keys(&combo(&save, true).unwrap()),
            ("KeyS", true, false, false, false)
        );
    }

    #[test]
    fn macs_use_the_shared_modifiers_when_they_name_none_of_their_own() {
        let canvas =
            json!({ "KeyCtrl": true, "KeyOption": true, "VKeyCode": 67, "VKeyCode_Mac": 8 });
        assert_eq!(
            keys(&combo(&canvas, true).unwrap()),
            ("KeyC", false, false, true, true)
        );
        let only_windows = json!({ "KeyShift": true, "VKeyCode": 0x70 });
        assert_eq!(
            keys(&combo(&only_windows, true).unwrap()),
            ("F1", false, true, false, false)
        );
        let lower = json!({ "Keyctrl": true, "VKeyCode": 0xBD });
        assert_eq!(
            keys(&combo(&lower, false).unwrap()),
            ("Minus", false, false, false, true)
        );
    }

    #[test]
    fn settings_without_a_key_are_no_shortcut() {
        assert!(combo(&json!({}), true).is_none());
        assert!(combo(&json!({ "VKeyCode": -1, "VKeyCode_Mac": -1 }), true).is_none());
        assert!(combo(&json!({ "VKeyCode": 0xFF }), false).is_none());
        assert!(combo(&json!("text"), false).is_none());
    }
}
