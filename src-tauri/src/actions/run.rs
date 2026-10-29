use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::error::ActionError;
use super::http;
use super::open_url::open_url;
use super::platform;
use super::sound::{Play, SoundPlayer};
use crate::clients::Clients;
use crate::config::{Action, AssetStore};
use crate::hass::Hass;
use crate::obs::Obs;
use crate::secrets::Secrets;

/// What actions need from the rest of the app.
#[derive(Clone)]
pub struct Context {
    pub assets: AssetStore,
    pub sounds: SoundPlayer,
    pub obs: Clients<Obs>,
    pub hass: Clients<Hass>,
    /// Where the clients keep passwords and tokens.
    pub secrets: Secrets,
}

/// Runs an action that does not move the dock; the engine does those itself.
pub fn run(action: &Action, context: &Context) -> Result<(), ActionError> {
    match action {
        Action::OpenUrl { url } => open_url(url),
        Action::Open { path } => {
            if std::path::Path::new(path).exists() {
                platform::open(path)
            } else {
                Err(ActionError::NotFound(path.clone()))
            }
        }
        Action::CloseApp { app } => platform::close_app(app),
        Action::TypeText { text, enter, paste } => platform::type_text(text, *enter, *paste),
        Action::Hotkey { keys } => platform::press_combo(keys),
        Action::Media { key } => platform::media_key(*key),
        Action::Delay { ms } => {
            thread::sleep(Duration::from_millis(u64::from(*ms)));
            Ok(())
        }
        Action::Random { actions } => match pick(actions) {
            Some(action) => run(action, context),
            None => Ok(()),
        },
        Action::PlaySound {
            sound,
            volume,
            mode,
            device,
            ..
        } => {
            let path = context
                .assets
                .sound_path(sound)
                .map_err(|err| ActionError::Sound(err.to_string()))?;
            if !path.exists() {
                return Err(ActionError::Sound(format!("{sound} is missing")));
            }
            context.sounds.play(Play {
                id: sound.clone(),
                path,
                volume: f32::from(*volume) / 100.0,
                mode: *mode,
                device: device.clone(),
            });
            Ok(())
        }
        Action::StopSounds => {
            context.sounds.stop_all();
            Ok(())
        }
        Action::Obs { server, command } => Ok(context
            .obs
            .get(*server)
            .ok_or(ActionError::NoServer)?
            .run(command)?),
        Action::Http { request } => http::run(request),
        Action::HomeAssistant { command, .. } if command.does_nothing() => Ok(()),
        Action::HomeAssistant { server, command } => Ok(context
            .hass
            .get(*server)
            .ok_or(ActionError::NoServer)?
            .run(command)?),
        // The engine sends `run` to the plugin, in order with the rest of the list.
        Action::Plugin { .. } => Ok(()),
        Action::OpenPage { .. }
        | Action::Back
        | Action::Home
        | Action::PreviousPage
        | Action::NextPage
        | Action::SwitchProfile { .. } => Ok(()),
    }
}

/// A shortcut asked for by a plugin, as a hotkey action would press it.
pub fn press_hotkey(keys: &crate::config::KeyCombo) -> Result<(), ActionError> {
    platform::press_combo(keys)
}

/// Good enough for picking a key's action; nothing here needs real randomness.
fn pick(actions: &[Action]) -> Option<&Action> {
    if actions.is_empty() {
        return None;
    }
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or_default();
    actions.get(seed % actions.len())
}
