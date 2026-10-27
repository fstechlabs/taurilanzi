//! The `keylist` of a `hotkey` command: macOS symbols (`⌘⇧4`) or names joined by `+`
//! (`Ctrl+Shift+S`, as Windows plugins write it).

use crate::config::KeyCombo;

pub fn parse(keylist: &str) -> Option<KeyCombo> {
    let mut combo = KeyCombo {
        key: String::new(),
        command: false,
        shift: false,
        option: false,
        control: false,
    };
    let mut rest = String::new();
    for c in keylist.trim().chars() {
        match c {
            '⌘' => combo.command = true,
            '⇧' => combo.shift = true,
            '⌥' => combo.option = true,
            '^' | '⌃' => combo.control = true,
            _ => rest.push(c),
        }
    }
    let parts: Vec<&str> = if rest.len() > 1 && rest.contains('+') {
        rest.split('+')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect()
    } else {
        vec![rest.trim()]
    };
    let (key, modifiers) = parts.split_last()?;
    for modifier in modifiers {
        match modifier.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "meta" | "win" | "super" => combo.command = true,
            "shift" => combo.shift = true,
            "alt" | "option" | "opt" => combo.option = true,
            "ctrl" | "control" => combo.control = true,
            _ => return None,
        }
    }
    combo.key = code(key)?;
    Some(combo)
}

/// A key as `KeyboardEvent.code` names it.
fn code(key: &str) -> Option<String> {
    let lower = key.to_ascii_lowercase();
    let named = match lower.as_str() {
        "enter" | "return" | "⏎" | "↩" => "Enter",
        "space" | " " => "Space",
        "tab" | "⇥" => "Tab",
        "esc" | "escape" | "⎋" => "Escape",
        "backspace" | "delete" | "⌫" => "Backspace",
        "del" | "forwarddelete" | "⌦" => "Delete",
        "up" | "arrowup" | "↑" => "ArrowUp",
        "down" | "arrowdown" | "↓" => "ArrowDown",
        "left" | "arrowleft" | "←" => "ArrowLeft",
        "right" | "arrowright" | "→" => "ArrowRight",
        "home" | "↖" => "Home",
        "end" | "↘" => "End",
        "pageup" | "⇞" => "PageUp",
        "pagedown" | "⇟" => "PageDown",
        "-" => "Minus",
        "=" => "Equal",
        "," => "Comma",
        "." => "Period",
        "/" => "Slash",
        ";" => "Semicolon",
        "'" => "Quote",
        "[" => "BracketLeft",
        "]" => "BracketRight",
        "\\" => "Backslash",
        "`" => "Backquote",
        _ => "",
    };
    if !named.is_empty() {
        return Some(named.into());
    }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => {
            Some(format!("Key{}", c.to_ascii_uppercase()))
        }
        (Some(c), None) if c.is_ascii_digit() => Some(format!("Digit{c}")),
        _ => {
            let number: u8 = lower.strip_prefix('f')?.parse().ok()?;
            (1..=20).contains(&number).then(|| format!("F{number}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(key: &str, command: bool, shift: bool, option: bool, control: bool) -> KeyCombo {
        KeyCombo {
            key: key.into(),
            command,
            shift,
            option,
            control,
        }
    }

    #[test]
    fn reads_macos_symbols() {
        assert_eq!(parse("⌘C"), Some(combo("KeyC", true, false, false, false)));
        assert_eq!(
            parse("⌘⇧4"),
            Some(combo("Digit4", true, true, false, false))
        );
        assert_eq!(parse("^⌥A"), Some(combo("KeyA", false, false, true, true)));
        assert_eq!(
            parse("⌃⎋"),
            Some(combo("Escape", false, false, false, true))
        );
    }

    #[test]
    fn reads_names_joined_by_plus() {
        assert_eq!(
            parse("Ctrl+Shift+S"),
            Some(combo("KeyS", false, true, false, true))
        );
        assert_eq!(
            parse("Cmd+Space"),
            Some(combo("Space", true, false, false, false))
        );
        assert_eq!(
            parse("alt + tab"),
            Some(combo("Tab", false, false, true, false))
        );
        assert_eq!(
            parse("Win+Enter"),
            Some(combo("Enter", true, false, false, false))
        );
    }

    #[test]
    fn reads_function_and_arrow_keys() {
        assert_eq!(parse("F5"), Some(combo("F5", false, false, false, false)));
        assert_eq!(parse("f12"), Some(combo("F12", false, false, false, false)));
        assert_eq!(
            parse("↑"),
            Some(combo("ArrowUp", false, false, false, false))
        );
        assert_eq!(
            parse("⌘←"),
            Some(combo("ArrowLeft", true, false, false, false))
        );
        assert_eq!(
            parse("Shift+Down"),
            Some(combo("ArrowDown", false, true, false, false))
        );
        assert_eq!(
            parse("ArrowRight"),
            Some(combo("ArrowRight", false, false, false, false))
        );
        assert_eq!(parse("⌘/"), Some(combo("Slash", true, false, false, false)));
    }

    #[test]
    fn unknown_keys_and_modifiers_are_refused() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("⌘"), None);
        assert_eq!(parse("F25"), None);
        assert_eq!(parse("Hyper+K"), None);
        assert_eq!(parse("Ctrl+Banana"), None);
        assert_eq!(parse("ñ"), None);
    }
}
