//! Physical keys by their `KeyboardEvent.code` name, which the UI records, to Linux input key
//! codes. Positions, not characters: `KeyZ` is the key where Z is on a US keyboard, whatever the
//! layout prints on it.

use evdev::KeyCode;

pub const CODES: &[(&str, KeyCode)] = &[
    ("Escape", KeyCode::KEY_ESC),
    ("Digit1", KeyCode::KEY_1),
    ("Digit2", KeyCode::KEY_2),
    ("Digit3", KeyCode::KEY_3),
    ("Digit4", KeyCode::KEY_4),
    ("Digit5", KeyCode::KEY_5),
    ("Digit6", KeyCode::KEY_6),
    ("Digit7", KeyCode::KEY_7),
    ("Digit8", KeyCode::KEY_8),
    ("Digit9", KeyCode::KEY_9),
    ("Digit0", KeyCode::KEY_0),
    ("Minus", KeyCode::KEY_MINUS),
    ("Equal", KeyCode::KEY_EQUAL),
    ("Backspace", KeyCode::KEY_BACKSPACE),
    ("Tab", KeyCode::KEY_TAB),
    ("KeyQ", KeyCode::KEY_Q),
    ("KeyW", KeyCode::KEY_W),
    ("KeyE", KeyCode::KEY_E),
    ("KeyR", KeyCode::KEY_R),
    ("KeyT", KeyCode::KEY_T),
    ("KeyY", KeyCode::KEY_Y),
    ("KeyU", KeyCode::KEY_U),
    ("KeyI", KeyCode::KEY_I),
    ("KeyO", KeyCode::KEY_O),
    ("KeyP", KeyCode::KEY_P),
    ("BracketLeft", KeyCode::KEY_LEFTBRACE),
    ("BracketRight", KeyCode::KEY_RIGHTBRACE),
    ("Enter", KeyCode::KEY_ENTER),
    ("KeyA", KeyCode::KEY_A),
    ("KeyS", KeyCode::KEY_S),
    ("KeyD", KeyCode::KEY_D),
    ("KeyF", KeyCode::KEY_F),
    ("KeyG", KeyCode::KEY_G),
    ("KeyH", KeyCode::KEY_H),
    ("KeyJ", KeyCode::KEY_J),
    ("KeyK", KeyCode::KEY_K),
    ("KeyL", KeyCode::KEY_L),
    ("Semicolon", KeyCode::KEY_SEMICOLON),
    ("Quote", KeyCode::KEY_APOSTROPHE),
    ("Backquote", KeyCode::KEY_GRAVE),
    ("Backslash", KeyCode::KEY_BACKSLASH),
    ("KeyZ", KeyCode::KEY_Z),
    ("KeyX", KeyCode::KEY_X),
    ("KeyC", KeyCode::KEY_C),
    ("KeyV", KeyCode::KEY_V),
    ("KeyB", KeyCode::KEY_B),
    ("KeyN", KeyCode::KEY_N),
    ("KeyM", KeyCode::KEY_M),
    ("Comma", KeyCode::KEY_COMMA),
    ("Period", KeyCode::KEY_DOT),
    ("Slash", KeyCode::KEY_SLASH),
    ("NumpadMultiply", KeyCode::KEY_KPASTERISK),
    ("Space", KeyCode::KEY_SPACE),
    ("F1", KeyCode::KEY_F1),
    ("F2", KeyCode::KEY_F2),
    ("F3", KeyCode::KEY_F3),
    ("F4", KeyCode::KEY_F4),
    ("F5", KeyCode::KEY_F5),
    ("F6", KeyCode::KEY_F6),
    ("F7", KeyCode::KEY_F7),
    ("F8", KeyCode::KEY_F8),
    ("F9", KeyCode::KEY_F9),
    ("F10", KeyCode::KEY_F10),
    ("NumLock", KeyCode::KEY_NUMLOCK),
    ("Numpad7", KeyCode::KEY_KP7),
    ("Numpad8", KeyCode::KEY_KP8),
    ("Numpad9", KeyCode::KEY_KP9),
    ("NumpadSubtract", KeyCode::KEY_KPMINUS),
    ("Numpad4", KeyCode::KEY_KP4),
    ("Numpad5", KeyCode::KEY_KP5),
    ("Numpad6", KeyCode::KEY_KP6),
    ("NumpadAdd", KeyCode::KEY_KPPLUS),
    ("Numpad1", KeyCode::KEY_KP1),
    ("Numpad2", KeyCode::KEY_KP2),
    ("Numpad3", KeyCode::KEY_KP3),
    ("Numpad0", KeyCode::KEY_KP0),
    ("NumpadDecimal", KeyCode::KEY_KPDOT),
    ("IntlBackslash", KeyCode::KEY_102ND),
    ("F11", KeyCode::KEY_F11),
    ("F12", KeyCode::KEY_F12),
    ("NumpadEnter", KeyCode::KEY_KPENTER),
    ("NumpadDivide", KeyCode::KEY_KPSLASH),
    ("Home", KeyCode::KEY_HOME),
    ("ArrowUp", KeyCode::KEY_UP),
    ("PageUp", KeyCode::KEY_PAGEUP),
    ("ArrowLeft", KeyCode::KEY_LEFT),
    ("ArrowRight", KeyCode::KEY_RIGHT),
    ("End", KeyCode::KEY_END),
    ("ArrowDown", KeyCode::KEY_DOWN),
    ("PageDown", KeyCode::KEY_PAGEDOWN),
    ("Insert", KeyCode::KEY_INSERT),
    ("Delete", KeyCode::KEY_DELETE),
    ("NumpadEqual", KeyCode::KEY_KPEQUAL),
    ("F13", KeyCode::KEY_F13),
    ("F14", KeyCode::KEY_F14),
    ("F15", KeyCode::KEY_F15),
    ("F16", KeyCode::KEY_F16),
    ("F17", KeyCode::KEY_F17),
    ("F18", KeyCode::KEY_F18),
    ("F19", KeyCode::KEY_F19),
    ("F20", KeyCode::KEY_F20),
];

pub fn code(key: &str) -> Option<KeyCode> {
    CODES
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, code)| *code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_physical_keys_and_rejects_unknown_ones() {
        assert_eq!(code("KeyC"), Some(KeyCode::KEY_C));
        assert_eq!(code("ArrowUp"), Some(KeyCode::KEY_UP));
        assert_eq!(code("Ñ"), None);
    }

    #[test]
    fn every_name_and_code_appears_once() {
        for (i, (name, value)) in CODES.iter().enumerate() {
            assert!(
                CODES[i + 1..].iter().all(|(n, v)| n != name && v != value),
                "{name}"
            );
        }
    }
}
