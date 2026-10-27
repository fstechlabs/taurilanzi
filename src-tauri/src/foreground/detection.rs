use serde::Serialize;

/// Whether the app can tell which app is in front, which switching profiles by app needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(
    not(target_os = "linux"),
    allow(dead_code, reason = "only Linux needs setting up")
)]
pub enum AppDetection {
    Ready,
    /// GNOME tells no app which window is in front; a small extension of the app's does.
    NeedsExtension,
    /// The extension is in place, and GNOME loads it at the next login.
    NeedsRelogin,
    /// This desktop gives no way to know.
    Unavailable,
}
