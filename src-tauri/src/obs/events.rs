//! OBS's events, applied to the states keys follow.

use super::protocol::Event;
use super::requests::ItemIds;
use super::status::{ObsStates, Watch};

pub enum Effect {
    None,
    /// The states changed; tell the engine.
    Changed,
    /// Scenes, sources or filters were added, removed or renamed: read everything again. What
    /// the event itself says is already applied.
    Reread,
}

pub fn apply(states: &mut ObsStates, ids: &ItemIds, event: &Event) -> Effect {
    let data = &event.event_data;
    let flag = |field: &str| data[field].as_bool().unwrap_or(false);
    let text = |field: &str| data[field].as_str().unwrap_or_default().to_string();
    let before = states.clone();
    match event.event_type.as_str() {
        "RecordStateChanged" => {
            states.recording = flag("outputActive");
            match data["outputState"].as_str() {
                Some("OBS_WEBSOCKET_OUTPUT_PAUSED") => states.record_paused = true,
                Some("OBS_WEBSOCKET_OUTPUT_RESUMED") => states.record_paused = false,
                _ => {}
            }
            if !states.recording {
                states.record_paused = false;
            }
        }
        "StreamStateChanged" => states.streaming = flag("outputActive"),
        "VirtualcamStateChanged" => states.virtual_cam = flag("outputActive"),
        "ReplayBufferStateChanged" => states.replay_buffer = flag("outputActive"),
        "StudioModeStateChanged" => {
            states.studio_mode = flag("studioModeEnabled");
            if states.studio_mode {
                // OBS does not always say which scene the preview starts with.
                return Effect::Reread;
            }
            states.preview_scene = None;
        }
        "CurrentProgramSceneChanged" => states.program_scene = Some(text("sceneName")),
        "CurrentPreviewSceneChanged" => states.preview_scene = Some(text("sceneName")),
        "SceneItemEnableStateChanged" => {
            let key = (
                text("sceneName"),
                data["sceneItemId"].as_i64().unwrap_or(-1),
            );
            if let Some(watch) = ids.get(&key) {
                states.items.insert(watch.clone(), flag("sceneItemEnabled"));
            }
        }
        "SourceFilterEnableStateChanged" => {
            let watch = Watch::FilterEnabled {
                source: text("sourceName"),
                filter: text("filterName"),
            };
            if let Some(on) = states.items.get_mut(&watch) {
                *on = flag("filterEnabled");
            }
        }
        "InputMuteStateChanged" => {
            if let Some(on) = states.items.get_mut(&Watch::InputMuted(text("inputName"))) {
                *on = flag("inputMuted");
            }
        }
        "CurrentSceneCollectionChanged"
        | "SceneCreated"
        | "SceneRemoved"
        | "SceneNameChanged"
        | "InputCreated"
        | "InputRemoved"
        | "InputNameChanged"
        | "SceneItemCreated"
        | "SceneItemRemoved"
        | "SourceFilterCreated"
        | "SourceFilterRemoved"
        | "SourceFilterNameChanged" => return Effect::Reread,
        _ => {}
    }
    if *states == before {
        Effect::None
    } else {
        Effect::Changed
    }
}
