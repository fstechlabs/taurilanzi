//! Each kind of Linux desktop tells which window is in front its own way: X11 tells every app,
//! GNOME only its extensions, KDE its scripts, and wlroots desktops (Sway, Hyprland) a protocol.

use std::sync::{Mutex, OnceLock, PoisonError};

use super::{gnome, kwin, wlroots, x11};
use crate::config::AppRef;
use crate::foreground::AppDetection;

pub struct Front {
    pub pid: u32,
    pub app: Option<AppRef>,
}

enum Backend {
    X11(Box<Mutex<x11::X11>>),
    Gnome,
    Kwin(kwin::Kwin),
    Wlroots(wlroots::Wlroots),
    None,
}

static BACKEND: OnceLock<Backend> = OnceLock::new();

fn backend() -> &'static Backend {
    BACKEND.get_or_init(|| {
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        let wayland = session == "wayland"
            || (session != "x11" && std::env::var_os("WAYLAND_DISPLAY").is_some());
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_ascii_uppercase();
        let backend = if !wayland {
            x11::X11::connect().map(|x| Backend::X11(Box::new(Mutex::new(x))))
        } else if desktop.split(':').any(|d| d == "GNOME") {
            gnome::refresh();
            Some(Backend::Gnome)
        } else if desktop.split(':').any(|d| d == "KDE") {
            kwin::Kwin::start().map(Backend::Kwin)
        } else {
            wlroots::Wlroots::start().map(Backend::Wlroots)
        };
        let backend = backend.unwrap_or(Backend::None);
        log::info!(
            "app in front read through {}",
            match backend {
                Backend::X11(_) => "X11",
                Backend::Gnome => "the GNOME extension",
                Backend::Kwin(_) => "a KWin script",
                Backend::Wlroots(_) => "wlr-foreign-toplevel",
                Backend::None => "nothing",
            }
        );
        backend
    })
}

pub fn frontmost() -> Option<Front> {
    match backend() {
        Backend::X11(x11) => x11
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .frontmost(),
        Backend::Gnome => gnome::frontmost(),
        Backend::Kwin(kwin) => kwin.frontmost(),
        Backend::Wlroots(wlroots) => wlroots.frontmost(),
        Backend::None => None,
    }
}

pub fn detection() -> AppDetection {
    match backend() {
        Backend::Gnome => gnome::detection(),
        Backend::None => AppDetection::Unavailable,
        _ => AppDetection::Ready,
    }
}

/// Installs what this desktop needs, which only GNOME does.
pub fn set_up_detection() -> std::io::Result<AppDetection> {
    if let Backend::Gnome = backend() {
        gnome::install()?;
    }
    Ok(detection())
}
