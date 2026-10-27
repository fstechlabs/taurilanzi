//! Other systems are planned; until then every action that needs the system fails clearly.

use crate::actions::error::ActionError;
use crate::config::{KeyCombo, MediaKey};

pub fn accessibility_granted() -> bool {
    true
}

pub fn request_accessibility() {}

pub fn open_accessibility_settings() -> Result<(), ActionError> {
    Err(ActionError::Unsupported)
}

pub fn open(_target: &str) -> Result<(), ActionError> {
    Err(ActionError::Unsupported)
}

pub fn close_app(_app: &str) -> Result<(), ActionError> {
    Err(ActionError::Unsupported)
}

pub fn press_combo(_keys: &KeyCombo) -> Result<(), ActionError> {
    Err(ActionError::Unsupported)
}

pub fn type_text(_text: &str, _enter: bool, _paste: bool) -> Result<(), ActionError> {
    Err(ActionError::Unsupported)
}

pub fn media_key(_key: MediaKey) -> Result<(), ActionError> {
    Err(ActionError::Unsupported)
}
