mod apps;
mod brightness;
mod keyboard;
mod media;
mod scancodes;

pub use apps::{close_app, open};
pub use keyboard::{press_combo, type_text};
pub use media::media_key;

use crate::actions::error::ActionError;

/// Windows lets any app send keystrokes, except to apps running as administrator.
pub fn accessibility_granted() -> bool {
    true
}

pub fn request_accessibility() {}

pub fn open_accessibility_settings() -> Result<(), ActionError> {
    Ok(())
}
