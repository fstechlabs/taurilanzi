use tauri::{AppHandle, Manager};

use super::menu::{BRIGHTNESS_PREFIX, OPEN, PAUSE, PROFILE_PREFIX, QUIT, UPDATES};
use crate::config::DeviceSettings;
use crate::engine::EngineHandle;
use crate::state::AppState;
use crate::updates::{self, Status};
use crate::window;

pub fn on_menu(app: &AppHandle, id: &str) {
    match id {
        OPEN => window::show(app),
        QUIT => app.exit(0),
        UPDATES => {
            if matches!(app.state::<AppState>().updates.status(), Status::Ready(_)) {
                updates::install(app);
            } else {
                updates::check(app);
            }
        }
        _ => {
            let app = app.clone();
            let id = id.to_owned();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = act(&app, &id).await {
                    log::warn!("menu action {id} failed: {err}");
                }
            });
        }
    }
}

async fn act(app: &AppHandle, id: &str) -> Result<(), String> {
    let engine = app.state::<EngineHandle>().inner().clone();
    let snapshot = app
        .state::<AppState>()
        .snapshot()
        .ok_or("the engine has not started yet")?;
    let result = if id == PAUSE {
        engine.set_paused(!snapshot.paused).await
    } else if let Some(profile) = id.strip_prefix(PROFILE_PREFIX) {
        let profile = profile.parse().map_err(|_| "bad profile id")?;
        engine.set_profile(profile).await
    } else if let Some(level) = id.strip_prefix(BRIGHTNESS_PREFIX) {
        let brightness = level.parse().map_err(|_| "bad brightness")?;
        engine
            .set_device(DeviceSettings {
                brightness,
                ..snapshot.settings.clone()
            })
            .await
    } else {
        return Ok(());
    };
    result.map_err(|err| err.to_string())
}
