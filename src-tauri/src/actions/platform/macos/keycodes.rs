//! Physical keys by their `KeyboardEvent.code` name, which the UI records, to macOS virtual key
//! codes (`kVK_*` in Carbon's `Events.h`). Positions, not characters: `KeyZ` is the key where Z is
//! on a US keyboard, whatever the layout prints on it.

pub const RETURN: u16 = 0x24;
pub const V: u16 = 0x09;

const CODES: &[(&str, u16)] = &[
    ("KeyA", 0x00),
    ("KeyS", 0x01),
    ("KeyD", 0x02),
    ("KeyF", 0x03),
    ("KeyH", 0x04),
    ("KeyG", 0x05),
    ("KeyZ", 0x06),
    ("KeyX", 0x07),
    ("KeyC", 0x08),
    ("KeyV", V),
    ("IntlBackslash", 0x0A),
    ("KeyB", 0x0B),
    ("KeyQ", 0x0C),
    ("KeyW", 0x0D),
    ("KeyE", 0x0E),
    ("KeyR", 0x0F),
    ("KeyY", 0x10),
    ("KeyT", 0x11),
    ("Digit1", 0x12),
    ("Digit2", 0x13),
    ("Digit3", 0x14),
    ("Digit4", 0x15),
    ("Digit6", 0x16),
    ("Digit5", 0x17),
    ("Equal", 0x18),
    ("Digit9", 0x19),
    ("Digit7", 0x1A),
    ("Minus", 0x1B),
    ("Digit8", 0x1C),
    ("Digit0", 0x1D),
    ("BracketRight", 0x1E),
    ("KeyO", 0x1F),
    ("KeyU", 0x20),
    ("BracketLeft", 0x21),
    ("KeyI", 0x22),
    ("KeyP", 0x23),
    ("Enter", RETURN),
    ("KeyL", 0x25),
    ("KeyJ", 0x26),
    ("Quote", 0x27),
    ("KeyK", 0x28),
    ("Semicolon", 0x29),
    ("Backslash", 0x2A),
    ("Comma", 0x2B),
    ("Slash", 0x2C),
    ("KeyN", 0x2D),
    ("KeyM", 0x2E),
    ("Period", 0x2F),
    ("Tab", 0x30),
    ("Space", 0x31),
    ("Backquote", 0x32),
    ("Backspace", 0x33),
    ("Escape", 0x35),
    ("F17", 0x40),
    ("NumpadDecimal", 0x41),
    ("NumpadMultiply", 0x43),
    ("NumpadAdd", 0x45),
    ("NumLock", 0x47),
    ("NumpadDivide", 0x4B),
    ("NumpadEnter", 0x4C),
    ("NumpadSubtract", 0x4E),
    ("F18", 0x4F),
    ("F19", 0x50),
    ("NumpadEqual", 0x51),
    ("Numpad0", 0x52),
    ("Numpad1", 0x53),
    ("Numpad2", 0x54),
    ("Numpad3", 0x55),
    ("Numpad4", 0x56),
    ("Numpad5", 0x57),
    ("Numpad6", 0x58),
    ("Numpad7", 0x59),
    ("F20", 0x5A),
    ("Numpad8", 0x5B),
    ("Numpad9", 0x5C),
    ("F5", 0x60),
    ("F6", 0x61),
    ("F7", 0x62),
    ("F3", 0x63),
    ("F8", 0x64),
    ("F9", 0x65),
    ("F11", 0x67),
    ("F13", 0x69),
    ("F16", 0x6A),
    ("F14", 0x6B),
    ("F10", 0x6D),
    ("F12", 0x6F),
    ("F15", 0x71),
    ("Insert", 0x72),
    ("Home", 0x73),
    ("PageUp", 0x74),
    ("Delete", 0x75),
    ("F4", 0x76),
    ("End", 0x77),
    ("F2", 0x78),
    ("PageDown", 0x79),
    ("F1", 0x7A),
    ("ArrowLeft", 0x7B),
    ("ArrowRight", 0x7C),
    ("ArrowDown", 0x7D),
    ("ArrowUp", 0x7E),
];

pub fn code(key: &str) -> Option<u16> {
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
        assert_eq!(code("KeyC"), Some(0x08));
        assert_eq!(code("Digit4"), Some(0x15));
        assert_eq!(code("F12"), Some(0x6F));
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
