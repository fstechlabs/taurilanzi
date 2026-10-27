#[cfg(target_os = "windows")]
mod credentials;
mod file;
#[cfg(target_os = "macos")]
mod keychain;
#[cfg(target_os = "linux")]
mod secret_service;
mod store;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod unsupported;

#[cfg(target_os = "windows")]
use credentials as platform;
#[cfg(target_os = "macos")]
use keychain as platform;
#[cfg(target_os = "linux")]
use secret_service as platform;
pub use store::Secrets;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
use unsupported as platform;
