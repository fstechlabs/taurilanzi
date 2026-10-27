//! Apps linked to profiles: when one of them comes to the front, the dock shows its profile.

use serde::{Deserialize, Serialize};

/// An app as the system knows it, the same on every Mac.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AppRef {
    /// The bundle identifier on macOS (`com.figma.Desktop`); on Windows and Linux it will be the
    /// executable name (`figma.exe`, `figma`), which is what those systems report for the app in
    /// front.
    pub id: String,
    /// What the user sees, as it was when the app was linked.
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct ProfileSwitching {
    /// Switches to the profile of the app in front, and back when the user leaves it.
    pub follow_apps: bool,
}

impl Default for ProfileSwitching {
    fn default() -> Self {
        Self { follow_apps: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Preferences, Profile};

    #[test]
    fn profiles_saved_before_linked_apps_still_load() {
        let profile: Profile = serde_json::from_str(
            r#"{"id":"6f1c0d52-4a43-4f39-9a0e-1a2b3c4d5e6f","name":"P",
                "home_page":"0e1f2a3b-4c5d-4e6f-8a9b-0c1d2e3f4a5b","pages":[]}"#,
        )
        .unwrap();
        assert!(profile.apps.is_empty());
    }

    #[test]
    fn preferences_saved_before_profiles_and_obs_still_load() {
        let prefs: Preferences =
            serde_json::from_str(r#"{"gestures":{"press_feedback":false}}"#).unwrap();
        assert!(prefs.profiles.follow_apps);
        let prefs: Preferences = serde_json::from_str(r#"{"profiles":{}}"#).unwrap();
        assert_eq!(prefs.profiles, ProfileSwitching::default());
    }

    #[test]
    fn linked_apps_round_trip() {
        let app = AppRef {
            id: "com.figma.Desktop".into(),
            name: "Figma".into(),
        };
        let json = serde_json::to_string(&app).unwrap();
        assert_eq!(json, r#"{"id":"com.figma.Desktop","name":"Figma"}"#);
        assert_eq!(serde_json::from_str::<AppRef>(&json).unwrap(), app);
    }
}
