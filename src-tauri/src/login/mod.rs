#[cfg(target_os = "linux")]
mod autostart;
#[cfg(target_os = "macos")]
mod launch_agents;
mod placement;
#[cfg(target_os = "windows")]
mod run_key;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod unsupported;

#[cfg(target_os = "linux")]
use autostart as platform;
#[cfg(target_os = "macos")]
use launch_agents as platform;
pub use placement::{BACKGROUND_ARG, Placement};
#[cfg(target_os = "windows")]
use run_key as platform;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
use unsupported as platform;
