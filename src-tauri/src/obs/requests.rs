//! What each command, each list of names and each followed state asks OBS.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::{Value, json};

use super::error::ObsError;
use super::link::Link;
use super::protocol::status;
use super::status::{ObsStates, Watch};
use crate::config::{ObsCommand, ObsMediaControl, ObsSwitch, ObsVolume};

const MIN_DB: f64 = -100.0;
const MEDIA_KINDS: &[&str] = &["ffmpeg_source", "vlc_source", "slideshow", "slideshow_v2"];

/// Names OBS has, for the editor to suggest.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObsList {
    Scenes,
    /// Scenes and inputs: whatever can have filters or be captured.
    Sources,
    AudioInputs,
    MediaInputs,
    /// The sources in a scene.
    SceneItems {
        scene: String,
    },
    Filters {
        source: String,
    },
    Transitions,
    Profiles,
    SceneCollections,
}

/// Scene items found while reading states, so their events can be told apart: by scene and
/// item id.
pub type ItemIds = BTreeMap<(String, i64), Watch>;

pub fn run(link: &mut Link, command: &ObsCommand) -> Result<(), ObsError> {
    match command {
        ObsCommand::Record { mode } => output(link, *mode, "Record"),
        ObsCommand::RecordPause { mode } => {
            let request = match mode {
                ObsSwitch::Toggle => "ToggleRecordPause",
                ObsSwitch::On => "PauseRecord",
                ObsSwitch::Off => "ResumeRecord",
            };
            done(link.call(request, Value::Null), *mode != ObsSwitch::Toggle)
        }
        ObsCommand::Stream { mode } => output(link, *mode, "Stream"),
        ObsCommand::VirtualCam { mode } => output(link, *mode, "VirtualCam"),
        ObsCommand::ReplayBuffer { mode } => output(link, *mode, "ReplayBuffer"),
        ObsCommand::SaveReplay => done(link.call("SaveReplayBuffer", Value::Null), false),
        ObsCommand::StudioMode { mode } => {
            let on = mode.target(|| {
                flag(
                    &link.call("GetStudioModeEnabled", Value::Null)?,
                    "studioModeEnabled",
                )
            })?;
            let request = json!({ "studioModeEnabled": on });
            done(link.call("SetStudioModeEnabled", request), true)
        }
        ObsCommand::Transition => {
            done(link.call("TriggerStudioModeTransition", Value::Null), false)
        }
        ObsCommand::SetTransition {
            transition,
            duration_ms,
        } => {
            let request = json!({ "transitionName": transition });
            link.call("SetCurrentSceneTransition", request)?;
            if let Some(ms) = duration_ms {
                let request = json!({ "transitionDuration": ms });
                link.call("SetCurrentSceneTransitionDuration", request)?;
            }
            Ok(())
        }
        ObsCommand::Screenshot { source, folder } => screenshot(link, source, folder),
        ObsCommand::SceneCollection { collection } => {
            let request = json!({ "sceneCollectionName": collection });
            done(link.call("SetCurrentSceneCollection", request), false)
        }
        ObsCommand::Profile { profile } => {
            let request = json!({ "profileName": profile });
            done(link.call("SetCurrentProfile", request), false)
        }
        ObsCommand::Scene { scene, preview } => {
            let request = if *preview {
                "SetCurrentPreviewScene"
            } else {
                "SetCurrentProgramScene"
            };
            done(link.call(request, json!({ "sceneName": scene })), false)
        }
        ObsCommand::SourceVisibility {
            scene,
            source,
            mode,
        } => {
            let id = scene_item_id(link, scene, source)?;
            let item = json!({ "sceneName": scene, "sceneItemId": id });
            let on = mode.target(|| {
                flag(
                    &link.call("GetSceneItemEnabled", item.clone())?,
                    "sceneItemEnabled",
                )
            })?;
            let request = json!({ "sceneName": scene, "sceneItemId": id, "sceneItemEnabled": on });
            done(link.call("SetSceneItemEnabled", request), false)
        }
        ObsCommand::Chapter { name } => {
            let request = if name.trim().is_empty() {
                Value::Null
            } else {
                json!({ "chapterName": name })
            };
            done(link.call("CreateRecordChapter", request), false)
        }
        ObsCommand::Filter {
            source,
            filter,
            mode,
        } => {
            let target = json!({ "sourceName": source, "filterName": filter });
            let on = mode.target(|| {
                flag(
                    &link.call("GetSourceFilter", target.clone())?,
                    "filterEnabled",
                )
            })?;
            let request =
                json!({ "sourceName": source, "filterName": filter, "filterEnabled": on });
            done(link.call("SetSourceFilterEnabled", request), false)
        }
        ObsCommand::Media { input, control } => media(link, input, *control),
        ObsCommand::Mute { input, mode } => match mode {
            ObsSwitch::Toggle => done(
                link.call("ToggleInputMute", json!({ "inputName": input })),
                false,
            ),
            _ => {
                let request = json!({ "inputName": input, "inputMuted": *mode == ObsSwitch::On });
                done(link.call("SetInputMute", request), false)
            }
        },
        ObsCommand::Volume { input, change } => volume(link, input, *change),
    }
}

/// Recording, streaming, the virtual camera and the replay buffer, which share their requests.
fn output(link: &mut Link, mode: ObsSwitch, name: &str) -> Result<(), ObsError> {
    let verb = match mode {
        ObsSwitch::Toggle => "Toggle",
        ObsSwitch::On => "Start",
        ObsSwitch::Off => "Stop",
    };
    done(
        link.call(&format!("{verb}{name}"), Value::Null),
        mode != ObsSwitch::Toggle,
    )
}

/// Turning on what is on, or off what is off, is not a failure.
fn done(result: Result<Value, ObsError>, settles: bool) -> Result<(), ObsError> {
    match result {
        Ok(_) => Ok(()),
        Err(ObsError::Rejected { code, .. })
            if settles
                && matches!(
                    code,
                    status::OUTPUT_RUNNING
                        | status::OUTPUT_NOT_RUNNING
                        | status::OUTPUT_PAUSED
                        | status::OUTPUT_NOT_PAUSED
                        | status::STUDIO_MODE_ACTIVE
                        | status::STUDIO_MODE_NOT_ACTIVE
                ) =>
        {
            Ok(())
        }
        Err(err) => Err(err),
    }
}

/// A PNG named like OBS's own, by default of the program and where OBS saves recordings.
fn screenshot(link: &mut Link, source: &str, folder: &str) -> Result<(), ObsError> {
    let source = if source.trim().is_empty() {
        program_scene(link)?
    } else {
        source.to_string()
    };
    let folder = if folder.trim().is_empty() {
        text(
            &link.call("GetRecordDirectory", Value::Null)?,
            "recordDirectory",
        )?
    } else {
        folder.to_string()
    };
    let safe: String = source
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c })
        .collect();
    let path = format!(
        "{}/Captura {safe} {}.png",
        folder.trim_end_matches(['/', '\\']),
        jiff::Zoned::now().strftime("%Y-%m-%d %H-%M-%S")
    );
    let request = json!({ "sourceName": source, "imageFormat": "png", "imageFilePath": path });
    done(link.call("SaveSourceScreenshot", request), false)
}

fn media(link: &mut Link, input: &str, control: ObsMediaControl) -> Result<(), ObsError> {
    let action = match control {
        ObsMediaControl::PlayPause => {
            let status = link.call("GetMediaInputStatus", json!({ "inputName": input }))?;
            if status["mediaState"] == "OBS_MEDIA_STATE_PLAYING" {
                "PAUSE"
            } else {
                "PLAY"
            }
        }
        ObsMediaControl::Play => "PLAY",
        ObsMediaControl::Pause => "PAUSE",
        ObsMediaControl::Stop => "STOP",
        ObsMediaControl::Restart => "RESTART",
        ObsMediaControl::Next => "NEXT",
        ObsMediaControl::Previous => "PREVIOUS",
    };
    let request = json!({
        "inputName": input,
        "mediaAction": format!("OBS_WEBSOCKET_MEDIA_INPUT_ACTION_{action}"),
    });
    done(link.call("TriggerMediaInputAction", request), false)
}

/// Fader positions follow OBS's mixer, where the volume is the cube of the position.
fn volume(link: &mut Link, input: &str, change: ObsVolume) -> Result<(), ObsError> {
    let request = match change {
        ObsVolume::Step { db } => {
            let current = link.call("GetInputVolume", json!({ "inputName": input }))?;
            // OBS sends no number for a fader all the way down.
            let now = current["inputVolumeDb"].as_f64().unwrap_or(MIN_DB);
            let next = (now + f64::from(db)).clamp(MIN_DB, now.max(0.0));
            json!({ "inputName": input, "inputVolumeDb": next })
        }
        ObsVolume::Set { percent } => {
            let position = f64::from(percent.min(100)) / 100.0;
            json!({ "inputName": input, "inputVolumeMul": position.powi(3) })
        }
    };
    done(link.call("SetInputVolume", request), false)
}

pub fn names(link: &mut Link, list: &ObsList) -> Result<Vec<String>, ObsError> {
    match list {
        ObsList::Scenes => scenes(link),
        ObsList::Sources => {
            let mut names = scenes(link)?;
            for input in inputs(link)? {
                if !names.contains(&input.0) {
                    names.push(input.0);
                }
            }
            Ok(names)
        }
        ObsList::AudioInputs => {
            let mut names = Vec::new();
            for (name, _) in inputs(link)? {
                match link.call("GetInputMute", json!({ "inputName": name })) {
                    Ok(_) => names.push(name),
                    Err(ObsError::Rejected { .. }) => {}
                    Err(err) => return Err(err),
                }
            }
            Ok(names)
        }
        ObsList::MediaInputs => Ok(inputs(link)?
            .into_iter()
            .filter(|(_, kind)| MEDIA_KINDS.contains(&kind.as_str()))
            .map(|(name, _)| name)
            .collect()),
        ObsList::SceneItems { scene } => {
            let items = link.call("GetSceneItemList", json!({ "sceneName": scene }))?;
            let mut names = names_in(&items, "sceneItems", "sourceName");
            names.reverse();
            Ok(names)
        }
        ObsList::Filters { source } => {
            let filters = link.call("GetSourceFilterList", json!({ "sourceName": source }))?;
            Ok(names_in(&filters, "filters", "filterName"))
        }
        ObsList::Transitions => {
            let transitions = link.call("GetSceneTransitionList", Value::Null)?;
            Ok(names_in(&transitions, "transitions", "transitionName"))
        }
        ObsList::Profiles => strings(&link.call("GetProfileList", Value::Null)?, "profiles"),
        ObsList::SceneCollections => strings(
            &link.call("GetSceneCollectionList", Value::Null)?,
            "sceneCollections",
        ),
    }
}

/// In the order OBS shows them, top first; obs-websocket sends them bottom first.
fn scenes(link: &mut Link) -> Result<Vec<String>, ObsError> {
    let list = link.call("GetSceneList", Value::Null)?;
    let mut names = names_in(&list, "scenes", "sceneName");
    names.reverse();
    Ok(names)
}

/// Names and kinds.
fn inputs(link: &mut Link) -> Result<Vec<(String, String)>, ObsError> {
    let list = link.call("GetInputList", Value::Null)?;
    Ok(list["inputs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|input| {
            let name = input["inputName"].as_str()?;
            let kind = input["inputKind"].as_str().unwrap_or_default();
            Some((name.to_string(), kind.to_string()))
        })
        .collect())
}

/// Everything a key may follow, read afresh, after connecting or when OBS changed a lot.
pub fn read_states(
    link: &mut Link,
    watches: &BTreeSet<Watch>,
    ids: &mut ItemIds,
) -> Result<ObsStates, ObsError> {
    let record = link.call("GetRecordStatus", Value::Null)?;
    let mut states = ObsStates {
        recording: flag(&record, "outputActive")?,
        record_paused: flag(&record, "outputPaused").unwrap_or(false),
        streaming: output_active(link, "GetStreamStatus")?,
        virtual_cam: output_active(link, "GetVirtualCamStatus")?,
        replay_buffer: output_active(link, "GetReplayBufferStatus")?,
        studio_mode: flag(
            &link.call("GetStudioModeEnabled", Value::Null)?,
            "studioModeEnabled",
        )?,
        program_scene: Some(program_scene(link)?),
        preview_scene: None,
        items: BTreeMap::new(),
    };
    if states.studio_mode {
        states.preview_scene = rejected_as_none(
            link.call("GetCurrentPreviewScene", Value::Null)
                .and_then(|scene| text(&scene, "currentPreviewSceneName")),
        )?;
    }
    read_items(link, watches, ids, &mut states)?;
    Ok(states)
}

/// The states of the sources, filters and inputs keys follow.
pub fn read_items(
    link: &mut Link,
    watches: &BTreeSet<Watch>,
    ids: &mut ItemIds,
    states: &mut ObsStates,
) -> Result<(), ObsError> {
    ids.clear();
    states.items.clear();
    for watch in watches.iter().filter(|w| w.is_of_an_item()) {
        let on = rejected_as_none(read_item(link, watch, ids))?.unwrap_or(false);
        states.items.insert(watch.clone(), on);
    }
    Ok(())
}

fn read_item(link: &mut Link, watch: &Watch, ids: &mut ItemIds) -> Result<bool, ObsError> {
    match watch {
        Watch::SourceVisible { scene, source } => {
            let id = scene_item_id(link, scene, source)?;
            ids.insert((scene.clone(), id), watch.clone());
            let item = json!({ "sceneName": scene, "sceneItemId": id });
            flag(&link.call("GetSceneItemEnabled", item)?, "sceneItemEnabled")
        }
        Watch::FilterEnabled { source, filter } => {
            let request = json!({ "sourceName": source, "filterName": filter });
            flag(&link.call("GetSourceFilter", request)?, "filterEnabled")
        }
        Watch::InputMuted(input) => flag(
            &link.call("GetInputMute", json!({ "inputName": input }))?,
            "inputMuted",
        ),
        _ => Ok(false),
    }
}

/// What OBS cannot find or has turned off counts as off; a lost connection stops everything.
fn rejected_as_none<T>(result: Result<T, ObsError>) -> Result<Option<T>, ObsError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(ObsError::Rejected { .. }) => Ok(None),
        Err(err) => Err(err),
    }
}

fn output_active(link: &mut Link, request: &str) -> Result<bool, ObsError> {
    let status = rejected_as_none(link.call(request, Value::Null))?;
    Ok(status.is_some_and(|status| status["outputActive"].as_bool().unwrap_or(false)))
}

fn program_scene(link: &mut Link) -> Result<String, ObsError> {
    text(
        &link.call("GetCurrentProgramScene", Value::Null)?,
        "currentProgramSceneName",
    )
}

fn scene_item_id(link: &mut Link, scene: &str, source: &str) -> Result<i64, ObsError> {
    let request = json!({ "sceneName": scene, "sourceName": source });
    link.call("GetSceneItemId", request)?["sceneItemId"]
        .as_i64()
        .ok_or_else(|| ObsError::Protocol("sceneItemId".into()))
}

fn flag(value: &Value, field: &str) -> Result<bool, ObsError> {
    value[field]
        .as_bool()
        .ok_or_else(|| ObsError::Protocol(format!("{field} is missing")))
}

fn text(value: &Value, field: &str) -> Result<String, ObsError> {
    value[field]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| ObsError::Protocol(format!("{field} is missing")))
}

fn names_in(value: &Value, list: &str, field: &str) -> Vec<String> {
    value[list]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item[field].as_str().map(str::to_string))
        .collect()
}

fn strings(value: &Value, list: &str) -> Result<Vec<String>, ObsError> {
    Ok(value[list]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|name| name.as_str().map(str::to_string))
        .collect())
}

#[cfg(test)]
mod tests {
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread;

    use tungstenite::{Message, WebSocket};

    use super::*;
    use crate::obs::protocol::op;

    type Seen = Arc<Mutex<Vec<(String, Value)>>>;

    /// A minimal obs-websocket on a local port: answers `GetVersion` itself and every other
    /// request with `answer`, an error being a rejection with that code.
    fn fake_obs(
        answer: impl Fn(&str, &Value) -> Result<Value, u16> + Send + 'static,
    ) -> (Link, Seen) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Seen::default();
        let log = Arc::clone(&seen);
        thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let send = |socket: &mut WebSocket<TcpStream>, op: u8, d: Value| {
                let _ = socket.send(Message::text(json!({ "op": op, "d": d }).to_string()));
            };
            send(&mut socket, op::HELLO, json!({ "rpcVersion": 1 }));
            while let Ok(Message::Text(text)) = socket.read() {
                let envelope: Value = serde_json::from_str(text.as_str()).unwrap();
                let d = &envelope["d"];
                match envelope["op"].as_u64().map(|op| op as u8) {
                    Some(op::IDENTIFY) => send(
                        &mut socket,
                        op::IDENTIFIED,
                        json!({ "negotiatedRpcVersion": 1 }),
                    ),
                    Some(op::REQUEST) => {
                        let kind = d["requestType"].as_str().unwrap().to_string();
                        let data = d["requestData"].clone();
                        let result = if kind == "GetVersion" {
                            Ok(json!({ "obsVersion": "31.0.0", "obsWebSocketVersion": "5.5.0" }))
                        } else {
                            log.lock().unwrap().push((kind.clone(), data.clone()));
                            answer(&kind, &data)
                        };
                        let (status, data) = match result {
                            Ok(data) => (json!({ "result": true, "code": 100 }), data),
                            Err(code) => (json!({ "result": false, "code": code }), Value::Null),
                        };
                        let response = json!({
                            "requestType": kind,
                            "requestId": d["requestId"],
                            "requestStatus": status,
                            "responseData": data,
                        });
                        send(&mut socket, op::REQUEST_RESPONSE, response);
                    }
                    _ => {}
                }
            }
        });
        let (link, versions) = Link::open("127.0.0.1", port, || None).unwrap();
        assert_eq!(
            (versions.obs.as_str(), versions.websocket.as_str()),
            ("31.0.0", "5.5.0")
        );
        (link, seen)
    }

    fn requests(seen: &Seen) -> Vec<(String, Value)> {
        std::mem::take(&mut *seen.lock().unwrap())
    }

    fn ok(_: &str, _: &Value) -> Result<Value, u16> {
        Ok(Value::Null)
    }

    fn run_one(command: ObsCommand) -> (String, Value) {
        let (mut link, seen) = fake_obs(ok);
        run(&mut link, &command).unwrap();
        let [request] = &requests(&seen)[..] else {
            panic!("one request for {command:?}");
        };
        request.clone()
    }

    #[test]
    fn outputs_use_toggle_start_and_stop() {
        let cases = [
            (
                ObsCommand::Record {
                    mode: ObsSwitch::Toggle,
                },
                "ToggleRecord",
            ),
            (
                ObsCommand::Stream {
                    mode: ObsSwitch::On,
                },
                "StartStream",
            ),
            (
                ObsCommand::VirtualCam {
                    mode: ObsSwitch::Off,
                },
                "StopVirtualCam",
            ),
            (
                ObsCommand::ReplayBuffer {
                    mode: ObsSwitch::On,
                },
                "StartReplayBuffer",
            ),
            (
                ObsCommand::RecordPause {
                    mode: ObsSwitch::Toggle,
                },
                "ToggleRecordPause",
            ),
            (
                ObsCommand::RecordPause {
                    mode: ObsSwitch::On,
                },
                "PauseRecord",
            ),
            (
                ObsCommand::RecordPause {
                    mode: ObsSwitch::Off,
                },
                "ResumeRecord",
            ),
            (ObsCommand::SaveReplay, "SaveReplayBuffer"),
            (ObsCommand::Transition, "TriggerStudioModeTransition"),
        ];
        for (command, request) in cases {
            assert_eq!(run_one(command), (request.into(), Value::Null));
        }
    }

    #[test]
    fn starting_what_already_runs_is_not_a_failure() {
        let (mut link, _) = fake_obs(|_, _| Err(status::OUTPUT_RUNNING));
        run(
            &mut link,
            &ObsCommand::Record {
                mode: ObsSwitch::On,
            },
        )
        .unwrap();
        assert!(matches!(
            run(
                &mut link,
                &ObsCommand::Record {
                    mode: ObsSwitch::Toggle
                }
            ),
            Err(ObsError::Rejected { code: 500, .. })
        ));
        assert!(matches!(
            run(
                &mut link,
                &ObsCommand::Scene {
                    scene: "A".into(),
                    preview: false
                }
            ),
            Err(ObsError::Rejected { .. })
        ));
    }

    #[test]
    fn names_and_settings_go_in_the_request() {
        let cases = [
            (
                ObsCommand::Scene {
                    scene: "Juego".into(),
                    preview: false,
                },
                "SetCurrentProgramScene",
                json!({ "sceneName": "Juego" }),
            ),
            (
                ObsCommand::Scene {
                    scene: "Juego".into(),
                    preview: true,
                },
                "SetCurrentPreviewScene",
                json!({ "sceneName": "Juego" }),
            ),
            (
                ObsCommand::SceneCollection {
                    collection: "Directo".into(),
                },
                "SetCurrentSceneCollection",
                json!({ "sceneCollectionName": "Directo" }),
            ),
            (
                ObsCommand::Profile {
                    profile: "Twitch".into(),
                },
                "SetCurrentProfile",
                json!({ "profileName": "Twitch" }),
            ),
            (
                ObsCommand::Chapter { name: " ".into() },
                "CreateRecordChapter",
                Value::Null,
            ),
            (
                ObsCommand::Chapter {
                    name: "Intro".into(),
                },
                "CreateRecordChapter",
                json!({ "chapterName": "Intro" }),
            ),
            (
                ObsCommand::Mute {
                    input: "Mic".into(),
                    mode: ObsSwitch::Toggle,
                },
                "ToggleInputMute",
                json!({ "inputName": "Mic" }),
            ),
            (
                ObsCommand::Mute {
                    input: "Mic".into(),
                    mode: ObsSwitch::Off,
                },
                "SetInputMute",
                json!({ "inputName": "Mic", "inputMuted": false }),
            ),
            (
                ObsCommand::Media {
                    input: "Vídeo".into(),
                    control: ObsMediaControl::Restart,
                },
                "TriggerMediaInputAction",
                json!({
                    "inputName": "Vídeo",
                    "mediaAction": "OBS_WEBSOCKET_MEDIA_INPUT_ACTION_RESTART",
                }),
            ),
            (
                ObsCommand::Volume {
                    input: "Mic".into(),
                    change: ObsVolume::Set { percent: 50 },
                },
                "SetInputVolume",
                json!({ "inputName": "Mic", "inputVolumeMul": 0.125 }),
            ),
        ];
        for (command, request, data) in cases {
            assert_eq!(run_one(command), (request.into(), data));
        }
    }

    #[test]
    fn transitions_set_the_duration_only_when_given() {
        let (mut link, seen) = fake_obs(ok);
        let command = |duration_ms| ObsCommand::SetTransition {
            transition: "Fundido".into(),
            duration_ms,
        };
        run(&mut link, &command(Some(300))).unwrap();
        assert_eq!(
            requests(&seen),
            [
                (
                    "SetCurrentSceneTransition".into(),
                    json!({ "transitionName": "Fundido" })
                ),
                (
                    "SetCurrentSceneTransitionDuration".into(),
                    json!({ "transitionDuration": 300 })
                ),
            ]
        );
        run(&mut link, &command(None)).unwrap();
        assert_eq!(requests(&seen).len(), 1);
    }

    #[test]
    fn toggles_read_the_state_first() {
        let (mut link, seen) = fake_obs(|kind, _| {
            Ok(match kind {
                "GetStudioModeEnabled" => json!({ "studioModeEnabled": true }),
                "GetSceneItemId" => json!({ "sceneItemId": 7 }),
                "GetSceneItemEnabled" => json!({ "sceneItemEnabled": false }),
                "GetSourceFilter" => json!({ "filterEnabled": true }),
                "GetMediaInputStatus" => json!({ "mediaState": "OBS_MEDIA_STATE_PLAYING" }),
                _ => Value::Null,
            })
        });
        run(
            &mut link,
            &ObsCommand::StudioMode {
                mode: ObsSwitch::Toggle,
            },
        )
        .unwrap();
        assert_eq!(
            requests(&seen)[1],
            (
                "SetStudioModeEnabled".into(),
                json!({ "studioModeEnabled": false })
            )
        );
        run(
            &mut link,
            &ObsCommand::SourceVisibility {
                scene: "Juego".into(),
                source: "Cámara".into(),
                mode: ObsSwitch::Toggle,
            },
        )
        .unwrap();
        let seen_now = requests(&seen);
        assert_eq!(
            seen_now[0],
            (
                "GetSceneItemId".into(),
                json!({ "sceneName": "Juego", "sourceName": "Cámara" })
            )
        );
        assert_eq!(
            seen_now[2],
            (
                "SetSceneItemEnabled".into(),
                json!({ "sceneName": "Juego", "sceneItemId": 7, "sceneItemEnabled": true })
            )
        );
        run(
            &mut link,
            &ObsCommand::Filter {
                source: "Cámara".into(),
                filter: "Croma".into(),
                mode: ObsSwitch::Toggle,
            },
        )
        .unwrap();
        assert_eq!(
            requests(&seen)[1].1,
            json!({ "sourceName": "Cámara", "filterName": "Croma", "filterEnabled": false })
        );
        run(
            &mut link,
            &ObsCommand::Media {
                input: "Vídeo".into(),
                control: ObsMediaControl::PlayPause,
            },
        )
        .unwrap();
        assert_eq!(
            requests(&seen)[1].1["mediaAction"],
            "OBS_WEBSOCKET_MEDIA_INPUT_ACTION_PAUSE"
        );
        run(
            &mut link,
            &ObsCommand::StudioMode {
                mode: ObsSwitch::On,
            },
        )
        .unwrap();
        assert_eq!(requests(&seen).len(), 1, "no need to read it");
    }

    #[test]
    fn volume_steps_stay_below_zero_decibels() {
        let step = |now: Value, db| {
            let (mut link, seen) = fake_obs(move |kind, _| {
                Ok(match kind {
                    "GetInputVolume" => json!({ "inputVolumeDb": now }),
                    _ => Value::Null,
                })
            });
            let change = ObsVolume::Step { db };
            run(
                &mut link,
                &ObsCommand::Volume {
                    input: "Mic".into(),
                    change,
                },
            )
            .unwrap();
            requests(&seen)[1].1["inputVolumeDb"].as_f64().unwrap()
        };
        assert_eq!(step(json!(-20.0), 5), -15.0);
        assert_eq!(step(json!(-3.0), 10), 0.0);
        assert_eq!(step(json!(-95.0), -10), -100.0);
        assert_eq!(step(Value::Null, 10), -90.0);
        assert_eq!(step(json!(4.0), 3), 4.0);
    }

    #[test]
    fn screenshots_default_to_the_program_and_the_recording_folder() {
        let (mut link, seen) = fake_obs(|kind, _| {
            Ok(match kind {
                "GetCurrentProgramScene" => json!({ "currentProgramSceneName": "Juego: 1/2" }),
                "GetRecordDirectory" => json!({ "recordDirectory": "/Volumes/Datos/Vídeos/" }),
                _ => Value::Null,
            })
        });
        let command = ObsCommand::Screenshot {
            source: String::new(),
            folder: String::new(),
        };
        run(&mut link, &command).unwrap();
        let seen = requests(&seen);
        let (kind, data) = seen.last().unwrap();
        assert_eq!(kind, "SaveSourceScreenshot");
        assert_eq!(data["sourceName"], "Juego: 1/2");
        assert_eq!(data["imageFormat"], "png");
        let path = data["imageFilePath"].as_str().unwrap();
        assert!(
            path.starts_with("/Volumes/Datos/Vídeos/Captura Juego- 1-2 "),
            "{path}"
        );
        assert!(path.ends_with(".png"), "{path}");
    }

    #[test]
    fn lists_names_in_the_order_obs_shows_them() {
        let (mut link, _) = fake_obs(|kind, _| {
            Ok(match kind {
                "GetSceneList" => {
                    json!({ "scenes": [{ "sceneName": "Bottom" }, { "sceneName": "Top" }] })
                }
                "GetInputList" => json!({ "inputs": [
                    { "inputName": "Mic", "inputKind": "coreaudio_input_capture" },
                    { "inputName": "Clip", "inputKind": "ffmpeg_source" },
                    { "inputName": "Top", "inputKind": "image_source" },
                ] }),
                "GetSceneItemList" => {
                    json!({ "sceneItems": [{ "sourceName": "Back" }, { "sourceName": "Front" }] })
                }
                "GetProfileList" => json!({ "profiles": ["Twitch", "YouTube"] }),
                _ => Value::Null,
            })
        });
        let mut names = |list| super::names(&mut link, &list).unwrap();
        assert_eq!(names(ObsList::Scenes), ["Top", "Bottom"]);
        assert_eq!(names(ObsList::Sources), ["Top", "Bottom", "Mic", "Clip"]);
        assert_eq!(names(ObsList::MediaInputs), ["Clip"]);
        assert_eq!(
            names(ObsList::SceneItems {
                scene: "Top".into()
            }),
            ["Front", "Back"]
        );
        assert_eq!(names(ObsList::Profiles), ["Twitch", "YouTube"]);
        assert!(names(ObsList::Transitions).is_empty());
    }
}
