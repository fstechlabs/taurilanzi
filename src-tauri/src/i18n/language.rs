//! The language of everything the app shows: the system's unless the user picks one. The engine
//! sets it at start and when the preference changes; every text drawn on a key, in a menu or in
//! an error is chosen from it with `tr!` or `trf!`.

#[cfg(not(test))]
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// Overrides the system's language, so tests see the same texts on any Mac.
pub const LANGUAGE_ENV: &str = "TAURILANZI_LANGUAGE";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Es,
    En,
}

/// What the user chose in the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LanguageChoice {
    #[default]
    System,
    Es,
    En,
}

#[cfg(not(test))]
static CURRENT: AtomicU8 = AtomicU8::new(0);

// Tests run in parallel: each keeps its own language, Spanish unless it sets another.
#[cfg(test)]
thread_local! {
    static CURRENT: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

#[cfg(not(test))]
fn load() -> u8 {
    CURRENT.load(Ordering::Relaxed)
}

#[cfg(not(test))]
fn store(value: u8) {
    CURRENT.store(value, Ordering::Relaxed);
}

#[cfg(test)]
fn load() -> u8 {
    CURRENT.with(std::cell::Cell::get)
}

#[cfg(test)]
fn store(value: u8) {
    CURRENT.with(|cell| cell.set(value));
}

impl Language {
    /// `es-ES`, `en-US`: what plugins of Ulanzi Studio expect.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Es => "es-ES",
            Self::En => "en-US",
        }
    }

    /// Spanish for any Spanish locale, English for everything else.
    pub fn from_locale(locale: &str) -> Self {
        if locale.trim().to_ascii_lowercase().starts_with("es") {
            Self::Es
        } else {
            Self::En
        }
    }
}

pub fn current() -> Language {
    match load() {
        1 => Language::En,
        _ => Language::Es,
    }
}

pub fn set(language: Language) {
    store(match language {
        Language::Es => 0,
        Language::En => 1,
    });
}

/// The language of the Mac's first preferred language, or `TAURILANZI_LANGUAGE`.
pub fn system() -> Language {
    std::env::var(LANGUAGE_ENV)
        .ok()
        .or_else(sys_locale::get_locale)
        .map_or(Language::En, |locale| Language::from_locale(&locale))
}

pub fn resolve(choice: LanguageChoice) -> Language {
    match choice {
        LanguageChoice::System => system(),
        LanguageChoice::Es => Language::Es,
        LanguageChoice::En => Language::En,
    }
}

/// The text of a key of `locales/*.json` in the current language: `tr!("tray.quit")`.
#[macro_export]
macro_rules! tr {
    ($key:literal $(,)?) => {
        $crate::i18n::text($key)
    };
}

/// Like `tr!`, replacing `{name}` in the text: `trf!("tray.updates.ready", version = v)`.
#[macro_export]
macro_rules! trf {
    ($key:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::i18n::format($key, &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),*])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_map_to_spanish_or_english() {
        for spanish in ["es", "es-ES", "es_MX", "ES-419"] {
            assert_eq!(Language::from_locale(spanish), Language::Es, "{spanish}");
        }
        for other in ["en-US", "en", "fr-FR", "de", ""] {
            assert_eq!(Language::from_locale(other), Language::En, "{other}");
        }
        assert_eq!(resolve(LanguageChoice::Es), Language::Es);
        assert_eq!(resolve(LanguageChoice::En), Language::En);
        assert_eq!(Language::En.tag(), "en-US");
    }
}
