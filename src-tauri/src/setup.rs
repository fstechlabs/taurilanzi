use std::path::PathBuf;
use std::sync::Arc;

use tauri::{App, Manager};

use crate::engine::{self, Launch, LaunchError, PluginsLaunch};
use crate::login::Placement;
use crate::plugin_desktop::TauriDesktop;
use crate::state::AppState;
use crate::updates::Updates;
use crate::{ipc, tray, updates, window};

/// Moves the data folder elsewhere, so tests never touch the user's configuration.
pub const DATA_DIR_ENV: &str = "TAURILANZI_DATA_DIR";
/// Leaves the dock alone, so tests can run next to the app that owns it.
const NO_DOCK_ENV: &str = "TAURILANZI_NO_DOCK";

pub fn setup(
    app: &mut App,
    bindings: &tauri_specta::Builder<tauri::Wry>,
    background: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    bindings.mount_events(app);
    let placement = Placement::detect(&app.config().identifier, &app.package_info().name);
    if let Some(item) = placement.login_item()
        && let Err(err) = item.register()
    {
        log::warn!("could not register the app to start with the session: {err}");
    }

    let data_dir = data_dir(app)?;
    let running = match engine::start(Launch {
        data_dir: data_dir.clone(),
        identifier: app.config().identifier.clone(),
        dock: std::env::var_os(NO_DOCK_ENV).is_none(),
        watch_apps: std::env::var_os(DATA_DIR_ENV).is_none(),
        plugins: Some(PluginsLaunch {
            resources: app.path().resource_dir().ok(),
            desktop: Arc::new(TauriDesktop::new(app.handle().clone())),
        }),
    }) {
        Ok(running) => running,
        Err(LaunchError::AlreadyRunning(dir)) => {
            // A clean exit, so launchd does not keep starting it again.
            log::warn!(
                "another copy of the app is using {}, quitting",
                dir.display()
            );
            std::process::exit(0);
        }
        Err(err) => return Err(err.into()),
    };
    app.manage(running.engine.clone());
    app.manage(running.assets);
    app.manage(running.plugins);
    app.manage(AppState::new(placement, Updates::new(&data_dir)));

    // Test builds stay out of the Dock and the app switcher.
    #[cfg(all(feature = "e2e", target_os = "macos"))]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    let handle = app.handle();
    ipc::forward(handle.clone(), &running.engine);
    tray::create(handle, &running.engine)?;
    window::create(handle)?;
    if background {
        window::hide(handle);
    } else {
        window::show(handle);
    }
    updates::schedule(handle.clone());
    Ok(())
}

fn data_dir(app: &App) -> tauri::Result<PathBuf> {
    match std::env::var_os(DATA_DIR_ENV) {
        Some(dir) => Ok(dir.into()),
        None => app.path().app_data_dir(),
    }
}
