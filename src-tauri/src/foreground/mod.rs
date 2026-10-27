mod detection;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod unsupported;
mod watcher;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use self::linux as platform;
#[cfg(target_os = "macos")]
use self::macos as platform;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
use self::unsupported as platform;
#[cfg(target_os = "windows")]
use self::windows as platform;
pub use detection::AppDetection;
pub use platform::{app_at, app_icon, detection, running_apps, set_up_detection};
pub use watcher::watch;
