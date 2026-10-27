mod apps;
mod icons;
mod names;

pub use apps::{app_at, frontmost, running_apps};
pub use icons::app_icon;

use super::AppDetection;

pub fn detection() -> AppDetection {
    AppDetection::Ready
}

pub fn set_up_detection() -> std::io::Result<AppDetection> {
    Ok(AppDetection::Ready)
}
