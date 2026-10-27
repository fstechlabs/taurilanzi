mod feeds;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod sampler;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod unsupported;
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
#[cfg(test)]
pub use feeds::Process;
pub use feeds::{Battery, PING_HISTORY, Pings, Power, Processes, Temperatures, Track, Volume};
pub use sampler::{Monitor, Readings, SAMPLE_EVERY};
