use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::hass::HassCommand;
use super::http::HttpRequest;
use super::model::Position;
use super::obs::ObsCommand;

/// What a key does when pressed. A key runs a list of them in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    OpenUrl {
        url: String,
    },
    /// An app, a file or a folder, as Finder would open it.
    Open {
        path: String,
    },
    /// Quits every running copy of an app, given as the path to its bundle.
    CloseApp {
        app: String,
    },
    TypeText {
        text: String,
        /// Presses Return after the text.
        #[serde(default)]
        enter: bool,
        /// Pastes the text instead of typing it, which is faster and keeps characters that
        /// cannot be typed with the current keyboard layout.
        #[serde(default)]
        paste: bool,
    },
    Hotkey {
        keys: KeyCombo,
    },
    Media {
        key: MediaKey,
    },
    /// A line on the canvas from this key to a page of the same profile.
    OpenPage {
        page: Uuid,
        /// Points the user added to steer the line across the canvas.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        waypoints: Vec<Position>,
    },
    /// To the page the user came from.
    Back,
    Home,
    /// The page before or after the current one, in the order of the profile's pages.
    PreviousPage,
    NextPage,
    SwitchProfile {
        profile: Uuid,
    },
    /// Waits before the next action of the list.
    Delay {
        ms: u32,
    },
    /// Runs one of the actions, picked at random on each press.
    Random {
        actions: Vec<Action>,
    },
    PlaySound {
        /// Audio file in the asset store.
        sound: String,
        /// The file name it had, to show in the editor.
        name: String,
        /// 0–100.
        volume: u8,
        mode: SoundMode,
        /// Output device by name; the system default when missing or not found.
        #[serde(default)]
        device: Option<String>,
    },
    StopSounds,
    Obs {
        /// The id of its server; nil is the one OBS of before servers had names.
        #[serde(default)]
        server: Uuid,
        command: ObsCommand,
    },
    /// A request to a web API or a webhook.
    Http {
        request: HttpRequest,
    },
    HomeAssistant {
        #[serde(default)]
        server: Uuid,
        command: HassCommand,
    },
    /// An action of a plugin made for Ulanzi Studio. The plugin draws the key and gets its
    /// presses; a key has one at most.
    Plugin {
        /// Its id in the plugin's manifest, whose first four segments are the plugin's.
        action: String,
        /// This placement of the action, which the plugin tells its keys apart by.
        id: Uuid,
        /// What the plugin saved for this placement. The plugin owns it: editing the key keeps it.
        #[serde(default)]
        #[specta(type = specta_typescript::Unknown)]
        settings: serde_json::Value,
    },
}

/// A key combination, by physical key (`KeyboardEvent.code` names, such as `KeyC` or `F5`), so
/// it stays the same whatever the keyboard layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct KeyCombo {
    pub key: String,
    #[serde(default)]
    pub command: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub option: bool,
    #[serde(default)]
    pub control: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum MediaKey {
    PlayPause,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
    Mute,
    BrightnessUp,
    BrightnessDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SoundMode {
    /// Plays it again on every press, over itself if it is still playing.
    Overlap,
    /// Starts it from the beginning on every press.
    Restart,
    /// A press while it plays stops it.
    Toggle,
    /// Plays it over and over until pressed again.
    Loop,
}

impl Action {
    /// Moves the dock to another page or profile. A key may do this once.
    pub fn navigates(&self) -> bool {
        matches!(
            self,
            Self::OpenPage { .. }
                | Self::Back
                | Self::Home
                | Self::PreviousPage
                | Self::NextPage
                | Self::SwitchProfile { .. }
        )
    }

    /// Whether the key shows for a moment how it went, nested picks included.
    pub fn shows_result(&self) -> bool {
        match self {
            Self::Http { request } => request.show_result,
            Self::Random { actions } => actions.iter().any(Self::shows_result),
            _ => false,
        }
    }

    /// Whether the key shows for a moment that it failed, when it says nothing on success.
    pub fn shows_failure(&self) -> bool {
        match self {
            Self::HomeAssistant { command, .. } => !command.does_nothing(),
            Self::Random { actions } => actions.iter().any(Self::shows_failure),
            _ => self.shows_result(),
        }
    }

    /// Sound files this action uses, nested ones included.
    pub fn sounds(&self) -> Vec<&str> {
        match self {
            Self::PlaySound { sound, .. } => vec![sound.as_str()],
            Self::Random { actions } => actions.iter().flat_map(Self::sounds).collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HassPress;

    fn hass(command: HassCommand) -> Action {
        Action::HomeAssistant {
            server: Uuid::nil(),
            command,
        }
    }

    #[test]
    fn keys_that_only_show_an_entity_never_show_a_failure() {
        let show = hass(HassCommand::Show {
            entity: "sensor.temp".into(),
        });
        let still = hass(HassCommand::Tile {
            entity: "light.a".into(),
            press: HassPress::Nothing,
        });
        let tile = hass(HassCommand::Tile {
            entity: "light.a".into(),
            press: HassPress::Auto,
        });
        assert!(!show.shows_failure());
        assert!(!still.shows_failure());
        assert!(tile.shows_failure());
        assert!(
            Action::Random {
                actions: vec![show, tile]
            }
            .shows_failure()
        );
    }
}
