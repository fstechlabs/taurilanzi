//! What a key can ask of OBS Studio, and where the app finds it.

use serde::{Deserialize, Serialize};

/// OBS refuses transitions shorter or longer than this.
const TRANSITION_MS: (u32, u32) = (50, 20_000);
const MAX_VOLUME_STEP_DB: i8 = 30;

/// One request to OBS. Names are as OBS shows them, so they can be typed without OBS running.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObsCommand {
    Record {
        mode: ObsSwitch,
    },
    /// On pauses the recording, off resumes it.
    RecordPause {
        mode: ObsSwitch,
    },
    Stream {
        mode: ObsSwitch,
    },
    VirtualCam {
        mode: ObsSwitch,
    },
    ReplayBuffer {
        mode: ObsSwitch,
    },
    SaveReplay,
    StudioMode {
        mode: ObsSwitch,
    },
    /// Sends the preview to the program, in studio mode.
    Transition,
    /// The transition used between scenes from now on.
    SetTransition {
        transition: String,
        #[serde(default)]
        duration_ms: Option<u32>,
    },
    Screenshot {
        /// Empty: the scene on the program.
        #[serde(default)]
        source: String,
        /// Empty: where OBS saves recordings.
        #[serde(default)]
        folder: String,
    },
    SceneCollection {
        collection: String,
    },
    Profile {
        profile: String,
    },
    Scene {
        scene: String,
        /// To the preview instead of the program, in studio mode.
        #[serde(default)]
        preview: bool,
    },
    SourceVisibility {
        scene: String,
        source: String,
        mode: ObsSwitch,
    },
    /// A chapter marker in the recording; OBS names it when `name` is empty.
    Chapter {
        #[serde(default)]
        name: String,
    },
    Filter {
        source: String,
        filter: String,
        mode: ObsSwitch,
    },
    Media {
        input: String,
        control: ObsMediaControl,
    },
    /// On mutes the input.
    Mute {
        input: String,
        mode: ObsSwitch,
    },
    Volume {
        input: String,
        change: ObsVolume,
    },
}

/// Whether a command turns something on, off, or to the other way it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ObsSwitch {
    #[default]
    Toggle,
    On,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ObsMediaControl {
    PlayPause,
    Play,
    Pause,
    Stop,
    Restart,
    Next,
    Previous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ObsVolume {
    /// Up or down this many decibels, never above 0 dB.
    Step { db: i8 },
    /// The position of the fader in OBS's mixer.
    Set { percent: u8 },
}

/// Where obs-websocket listened before servers had names; only read to move it to them.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct ObsSettings {
    pub host: String,
    pub port: u16,
}

impl ObsSwitch {
    /// The state to set, given how it is now; `current` is only asked for when toggling.
    pub fn target<E>(self, current: impl FnOnce() -> Result<bool, E>) -> Result<bool, E> {
        match self {
            Self::Toggle => current().map(|on| !on),
            Self::On => Ok(true),
            Self::Off => Ok(false),
        }
    }
}

impl ObsCommand {
    /// What is missing for the command to be saved.
    pub fn problem(&self) -> Option<&'static str> {
        let blank = |name: &str| name.trim().is_empty();
        match self {
            Self::SetTransition { transition, .. } if blank(transition) => {
                Some("choose the transition")
            }
            Self::SetTransition {
                duration_ms: Some(ms),
                ..
            } if !(TRANSITION_MS.0..=TRANSITION_MS.1).contains(ms) => {
                Some("a transition lasts between 50 ms and 20 s")
            }
            Self::SceneCollection { collection } if blank(collection) => {
                Some("choose the scene collection")
            }
            Self::Profile { profile } if blank(profile) => Some("choose the OBS profile"),
            Self::Scene { scene, .. } if blank(scene) => Some("choose the scene"),
            Self::SourceVisibility { scene, source, .. } if blank(scene) || blank(source) => {
                Some("choose the scene and the source")
            }
            Self::Filter { source, filter, .. } if blank(source) || blank(filter) => {
                Some("choose the source and the filter")
            }
            Self::Media { input, .. } | Self::Mute { input, .. } | Self::Volume { input, .. }
                if blank(input) =>
            {
                Some("choose the source")
            }
            Self::Volume {
                change: ObsVolume::Step { db },
                ..
            } if *db == 0 || db.unsigned_abs() > MAX_VOLUME_STEP_DB.unsigned_abs() => {
                Some("a volume step goes from 1 to 30 dB")
            }
            Self::Volume {
                change: ObsVolume::Set { percent },
                ..
            } if *percent > 100 => Some("the volume goes up to 100"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_command() -> Vec<ObsCommand> {
        use ObsCommand::*;
        vec![
            Record {
                mode: ObsSwitch::Toggle,
            },
            RecordPause {
                mode: ObsSwitch::On,
            },
            Stream {
                mode: ObsSwitch::Off,
            },
            VirtualCam {
                mode: ObsSwitch::Toggle,
            },
            ReplayBuffer {
                mode: ObsSwitch::On,
            },
            SaveReplay,
            StudioMode {
                mode: ObsSwitch::Off,
            },
            Transition,
            SetTransition {
                transition: "Fundido".into(),
                duration_ms: Some(300),
            },
            Screenshot {
                source: "Cámara".into(),
                folder: "/tmp".into(),
            },
            SceneCollection {
                collection: "Directo".into(),
            },
            Profile {
                profile: "Twitch".into(),
            },
            Scene {
                scene: "Juego".into(),
                preview: true,
            },
            SourceVisibility {
                scene: "Juego".into(),
                source: "Cámara".into(),
                mode: ObsSwitch::Toggle,
            },
            Chapter {
                name: "Intro".into(),
            },
            Filter {
                source: "Cámara".into(),
                filter: "Croma".into(),
                mode: ObsSwitch::On,
            },
            Media {
                input: "Vídeo".into(),
                control: ObsMediaControl::PlayPause,
            },
            Mute {
                input: "Mic".into(),
                mode: ObsSwitch::Toggle,
            },
            Volume {
                input: "Mic".into(),
                change: ObsVolume::Step { db: -3 },
            },
            Volume {
                input: "Mic".into(),
                change: ObsVolume::Set { percent: 80 },
            },
        ]
    }

    #[test]
    fn every_command_round_trips_and_is_valid() {
        for command in every_command() {
            let json = serde_json::to_string(&command).unwrap();
            assert_eq!(
                serde_json::from_str::<ObsCommand>(&json).unwrap(),
                command,
                "{json}"
            );
            assert_eq!(command.problem(), None, "{json}");
        }
    }

    #[test]
    fn uses_snake_case_and_defaults_optional_fields() {
        let json = serde_json::to_value(ObsCommand::Volume {
            input: "Mic".into(),
            change: ObsVolume::Step { db: 5 },
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "kind": "volume", "input": "Mic", "change": { "type": "step", "db": 5 } })
        );
        let parse = |json: &str| serde_json::from_str::<ObsCommand>(json).unwrap();
        assert_eq!(
            parse(r#"{"kind":"screenshot"}"#),
            ObsCommand::Screenshot {
                source: String::new(),
                folder: String::new(),
            }
        );
        assert_eq!(
            parse(r#"{"kind":"scene","scene":"A"}"#),
            ObsCommand::Scene {
                scene: "A".into(),
                preview: false,
            }
        );
        assert_eq!(
            parse(r#"{"kind":"set_transition","transition":"Corte"}"#),
            ObsCommand::SetTransition {
                transition: "Corte".into(),
                duration_ms: None,
            }
        );
        assert_eq!(
            parse(r#"{"kind":"chapter"}"#),
            ObsCommand::Chapter {
                name: String::new()
            }
        );
        assert_eq!(
            parse(r#"{"kind":"media","input":"V","control":"play_pause"}"#),
            ObsCommand::Media {
                input: "V".into(),
                control: ObsMediaControl::PlayPause,
            }
        );
        assert_eq!(
            parse(r#"{"kind":"record_pause","mode":"on"}"#),
            ObsCommand::RecordPause {
                mode: ObsSwitch::On
            }
        );
    }

    #[test]
    fn names_are_required() {
        use ObsCommand::*;
        let blank = || " ".to_string();
        let cases = [
            (
                SetTransition {
                    transition: blank(),
                    duration_ms: None,
                },
                "choose the transition",
            ),
            (
                SceneCollection {
                    collection: blank(),
                },
                "choose the scene collection",
            ),
            (Profile { profile: blank() }, "choose the OBS profile"),
            (
                Scene {
                    scene: blank(),
                    preview: false,
                },
                "choose the scene",
            ),
            (
                SourceVisibility {
                    scene: "A".into(),
                    source: blank(),
                    mode: ObsSwitch::Toggle,
                },
                "choose the scene and the source",
            ),
            (
                SourceVisibility {
                    scene: blank(),
                    source: "B".into(),
                    mode: ObsSwitch::Toggle,
                },
                "choose the scene and the source",
            ),
            (
                Filter {
                    source: "A".into(),
                    filter: blank(),
                    mode: ObsSwitch::On,
                },
                "choose the source and the filter",
            ),
            (
                Media {
                    input: blank(),
                    control: ObsMediaControl::Stop,
                },
                "choose the source",
            ),
            (
                Mute {
                    input: blank(),
                    mode: ObsSwitch::On,
                },
                "choose the source",
            ),
            (
                Volume {
                    input: String::new(),
                    change: ObsVolume::Set { percent: 10 },
                },
                "choose the source",
            ),
        ];
        for (command, problem) in cases {
            assert_eq!(command.problem(), Some(problem), "{command:?}");
        }
        let optional = [
            Screenshot {
                source: String::new(),
                folder: String::new(),
            },
            Chapter {
                name: String::new(),
            },
        ];
        for command in optional {
            assert_eq!(command.problem(), None, "{command:?}");
        }
    }

    #[test]
    fn limits_of_transitions_and_volume() {
        let transition = |ms| ObsCommand::SetTransition {
            transition: "Fundido".into(),
            duration_ms: Some(ms),
        };
        assert_eq!(transition(50).problem(), None);
        assert_eq!(transition(20_000).problem(), None);
        for ms in [0, 49, 20_001] {
            assert_eq!(
                transition(ms).problem(),
                Some("a transition lasts between 50 ms and 20 s")
            );
        }
        let volume = |change| ObsCommand::Volume {
            input: "Mic".into(),
            change,
        };
        for db in [-30, -1, 1, 30] {
            assert_eq!(volume(ObsVolume::Step { db }).problem(), None, "{db}");
        }
        for db in [0, 31, -31, i8::MIN, i8::MAX] {
            assert_eq!(
                volume(ObsVolume::Step { db }).problem(),
                Some("a volume step goes from 1 to 30 dB"),
                "{db}"
            );
        }
        assert_eq!(volume(ObsVolume::Set { percent: 100 }).problem(), None);
        assert_eq!(
            volume(ObsVolume::Set { percent: 101 }).problem(),
            Some("the volume goes up to 100")
        );
    }

    #[test]
    fn switches_only_ask_for_the_state_when_toggling() {
        let asked = |_: ()| -> Result<bool, ()> { panic!("asked for the state") };
        assert_eq!(ObsSwitch::On.target(|| asked(())), Ok(true));
        assert_eq!(ObsSwitch::Off.target(|| asked(())), Ok(false));
        assert_eq!(ObsSwitch::Toggle.target(|| Ok::<_, ()>(true)), Ok(false));
        assert_eq!(
            ObsSwitch::Toggle.target(|| Err::<bool, _>("lost")),
            Err("lost")
        );
    }
}
