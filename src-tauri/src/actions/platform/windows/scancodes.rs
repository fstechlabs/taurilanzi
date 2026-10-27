//! Physical keys by their `KeyboardEvent.code` name, which the UI records, to the scan codes of a
//! PC keyboard (set 1). Positions, not characters: `KeyZ` is the key where Z is on a US keyboard,
//! whatever the layout prints on it. Codes from 0xE000 up are extended keys.

pub const ENTER: u16 = 0x1C;
pub const V: u16 = 0x2F;
pub const CONTROL: u16 = 0x1D;
pub const SHIFT: u16 = 0x2A;
pub const ALT: u16 = 0x38;
pub const WIN: u16 = 0xE05B;

const CODES: &[(&str, u16)] = &[
    ("Escape", 0x01),
    ("Digit1", 0x02),
    ("Digit2", 0x03),
    ("Digit3", 0x04),
    ("Digit4", 0x05),
    ("Digit5", 0x06),
    ("Digit6", 0x07),
    ("Digit7", 0x08),
    ("Digit8", 0x09),
    ("Digit9", 0x0A),
    ("Digit0", 0x0B),
    ("Minus", 0x0C),
    ("Equal", 0x0D),
    ("Backspace", 0x0E),
    ("Tab", 0x0F),
    ("KeyQ", 0x10),
    ("KeyW", 0x11),
    ("KeyE", 0x12),
    ("KeyR", 0x13),
    ("KeyT", 0x14),
    ("KeyY", 0x15),
    ("KeyU", 0x16),
    ("KeyI", 0x17),
    ("KeyO", 0x18),
    ("KeyP", 0x19),
    ("BracketLeft", 0x1A),
    ("BracketRight", 0x1B),
    ("Enter", ENTER),
    ("KeyA", 0x1E),
    ("KeyS", 0x1F),
    ("KeyD", 0x20),
    ("KeyF", 0x21),
    ("KeyG", 0x22),
    ("KeyH", 0x23),
    ("KeyJ", 0x24),
    ("KeyK", 0x25),
    ("KeyL", 0x26),
    ("Semicolon", 0x27),
    ("Quote", 0x28),
    ("Backquote", 0x29),
    ("Backslash", 0x2B),
    ("KeyZ", 0x2C),
    ("KeyX", 0x2D),
    ("KeyC", 0x2E),
    ("KeyV", V),
    ("KeyB", 0x30),
    ("KeyN", 0x31),
    ("KeyM", 0x32),
    ("Comma", 0x33),
    ("Period", 0x34),
    ("Slash", 0x35),
    ("NumpadMultiply", 0x37),
    ("Space", 0x39),
    ("F1", 0x3B),
    ("F2", 0x3C),
    ("F3", 0x3D),
    ("F4", 0x3E),
    ("F5", 0x3F),
    ("F6", 0x40),
    ("F7", 0x41),
    ("F8", 0x42),
    ("F9", 0x43),
    ("F10", 0x44),
    ("Numpad7", 0x47),
    ("Numpad8", 0x48),
    ("Numpad9", 0x49),
    ("NumpadSubtract", 0x4A),
    ("Numpad4", 0x4B),
    ("Numpad5", 0x4C),
    ("Numpad6", 0x4D),
    ("NumpadAdd", 0x4E),
    ("Numpad1", 0x4F),
    ("Numpad2", 0x50),
    ("Numpad3", 0x51),
    ("Numpad0", 0x52),
    ("NumpadDecimal", 0x53),
    ("IntlBackslash", 0x56),
    ("F11", 0x57),
    ("F12", 0x58),
    ("NumpadEqual", 0x59),
    ("F13", 0x64),
    ("F14", 0x65),
    ("F15", 0x66),
    ("F16", 0x67),
    ("F17", 0x68),
    ("F18", 0x69),
    ("F19", 0x6A),
    ("F20", 0x6B),
    ("NumpadEnter", 0xE01C),
    ("NumpadDivide", 0xE035),
    ("NumLock", 0xE045),
    ("Home", 0xE047),
    ("ArrowUp", 0xE048),
    ("PageUp", 0xE049),
    ("ArrowLeft", 0xE04B),
    ("ArrowRight", 0xE04D),
    ("End", 0xE04F),
    ("ArrowDown", 0xE050),
    ("PageDown", 0xE051),
    ("Insert", 0xE052),
    ("Delete", 0xE053),
];

pub fn code(key: &str) -> Option<u16> {
    CODES
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, code)| *code)
}

pub fn is_extended(code: u16) -> bool {
    code & 0xFF00 == 0xE000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_physical_keys_and_rejects_unknown_ones() {
        assert_eq!(code("KeyC"), Some(0x2E));
        assert_eq!(code("ArrowUp"), Some(0xE048));
        assert!(is_extended(code("Delete").unwrap()));
        assert!(!is_extended(code("KeyA").unwrap()));
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
