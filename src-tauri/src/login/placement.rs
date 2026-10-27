use std::io;
use std::path::PathBuf;

use super::platform;

/// Argument the login item passes, so the app starts with only its menu bar icon.
pub const BACKGROUND_ARG: &str = "--background";
/// Lets a development build register itself too, to test it without packaging.
const MANAGE_ENV: &str = "TAURILANZI_MANAGE_LOGIN";

/// Whether this copy of the app may register itself to start with the session.
pub enum Placement {
    Managed(LoginItem),
    /// `tauri dev`: the system is left alone.
    Development,
    /// macOS runs an unsigned app from a temporary copy until it is moved to Applications.
    /// A login item pointing there would break, so nothing is registered.
    Translocated,
}

impl Placement {
    /// `label` names the registration: the app's identifier, so each build of it has its own.
    /// `name` is the app's, for systems that show it in their list of apps started at login.
    pub fn detect(label: &str, name: &str) -> Self {
        // An AppImage runs from a temporary mount; the file the user keeps is elsewhere.
        let app = match std::env::var_os("APPIMAGE") {
            Some(image) if cfg!(target_os = "linux") => Ok(PathBuf::from(image)),
            _ => std::env::current_exe(),
        };
        let Ok(app) = app else {
            return Self::Development;
        };
        if app.to_string_lossy().contains("/AppTranslocation/") {
            return Self::Translocated;
        }
        if cfg!(debug_assertions) && std::env::var_os(MANAGE_ENV).is_none() {
            return Self::Development;
        }
        Self::Managed(LoginItem {
            app,
            label: label.to_owned(),
            name: name.to_owned(),
        })
    }

    pub fn login_item(&self) -> Option<&LoginItem> {
        match self {
            Self::Managed(item) => Some(item),
            _ => None,
        }
    }
}

/// The app started at login in the background, and again by the system if it crashes.
pub struct LoginItem {
    app: PathBuf,
    label: String,
    name: String,
}

impl LoginItem {
    /// Registers the app, or points the registration at it again if it moved.
    pub fn register(&self) -> io::Result<()> {
        platform::register(&self.label, &self.name, &self.app, BACKGROUND_ARG)
    }
}
