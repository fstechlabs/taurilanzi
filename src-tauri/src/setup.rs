use std::path::PathBuf;
use std::sync::Arc;

use tauri::{App, Manager};
use tauri_specta::Event;

use crate::cloud::{Cloud, CloudSetup, Open};
use crate::engine::{self, Launch, LaunchError, PluginsLaunch};
use crate::ipc::CloudChanged;
use crate::login::Placement;
use crate::plugin_desktop::TauriDesktop;
use crate::secrets::Secrets;
use crate::state::AppState;
use crate::updates::Updates;
use crate::{ipc, tray, updates, window};

/// Moves the data folder elsewhere, so tests never touch the user's configuration.
pub const DATA_DIR_ENV: &str = "TAURILANZI_DATA_DIR";
/// Leaves the dock alone, so tests can run next to the app that owns it.
const NO_DOCK_ENV: &str = "TAURILANZI_NO_DOCK";
/// Another cloud than the one of this build.
const CLOUD_ENV: &str = "TAURILANZI_CLOUD";
const RELEASED_IDENTIFIER: &str = "com.taurilanzi.desktop";
const CLOUD_URL: &str = "https://taurilanzi.com";
/// `taurilanzi-cloud` running locally: development builds never touch real accounts.
const LOCAL_CLOUD_URL: &str = "http://localhost:3000";

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
    let cloud = Cloud::start(CloudSetup {
        data_dir: data_dir.clone(),
        url: cloud_url(&app.config().identifier),
        app_name: app.package_info().name.clone(),
        engine: running.engine.clone(),
        secrets: Secrets::new(&data_dir, &app.config().identifier),
        assets: running.assets.clone(),
        notify: {
            let handle = app.handle().clone();
            Box::new(move |status| {
                if let Err(err) = CloudChanged(status).emit(&handle) {
                    log::warn!("could not send the cloud status to the UI: {err}");
                }
            })
        },
        open: browser(app),
    })?;
    app.manage(cloud);
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

#[cfg(not(feature = "e2e"))]
fn browser(app: &App) -> Open {
    use tauri_plugin_opener::OpenerExt;

    let handle = app.handle().clone();
    Box::new(move |url| {
        handle
            .opener()
            .open_url(url, None::<&str>)
            .map_err(std::io::Error::other)
    })
}

/// The end-to-end tests have no browser and must not open the user's: following the cloud's
/// redirects is what the browser would do.
#[cfg(feature = "e2e")]
fn browser(_app: &App) -> Open {
    Box::new(|url| {
        let url = url.to_owned();
        std::thread::spawn(move || {
            if let Err(err) = ureq::get(&url).call() {
                log::warn!("the tests' browser could not open {url}: {err}");
            }
        });
        Ok(())
    })
}

fn cloud_url(identifier: &str) -> String {
    std::env::var(CLOUD_ENV)
        .ok()
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| match identifier {
            RELEASED_IDENTIFIER => CLOUD_URL.into(),
            _ => LOCAL_CLOUD_URL.into(),
        })
}

fn data_dir(app: &App) -> tauri::Result<PathBuf> {
    match std::env::var_os(DATA_DIR_ENV) {
        Some(dir) => Ok(dir.into()),
        None => app.path().app_data_dir(),
    }
}
