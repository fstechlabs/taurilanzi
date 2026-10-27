//! Not known yet on this system: no app is ever in front, so profiles never switch by themselves.

use std::path::Path;

use super::AppDetection;
use crate::config::AppRef;

/// The app in front and its process.
pub struct Front {
    pub pid: u32,
    pub app: Option<AppRef>,
}

pub fn frontmost() -> Option<Front> {
    None
}

pub fn running_apps() -> Vec<AppRef> {
    Vec::new()
}

pub fn app_at(_path: &Path) -> Option<AppRef> {
    None
}

pub fn app_icon(_id: &str) -> Option<Vec<u8>> {
    None
}

pub fn detection() -> AppDetection {
    AppDetection::Unavailable
}

pub fn set_up_detection() -> std::io::Result<AppDetection> {
    Ok(AppDetection::Unavailable)
}
