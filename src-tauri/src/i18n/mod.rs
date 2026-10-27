#[macro_use]
mod language;
mod messages;

pub use language::{Language, LanguageChoice, current, resolve, set};
pub use messages::{format, text};
