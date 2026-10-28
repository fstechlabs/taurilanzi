use std::fs::{File, TryLockError};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;

use log::{error, info};
use tokio::sync::broadcast;

use super::commands::Input;
use super::handle::{Activity, EngineHandle};
use super::integrations::secret_accounts;
use super::plugins::PluginKeys;
use super::runtime::{Engine, Links};
use crate::actions;
use crate::clients::Clients;
use crate::config::{self, AssetStore, Store};
use crate::driver::d200::D200;
use crate::i18n;
use crate::plugins::{Desktop, Plugins, PluginsSetup};
use crate::render::Renderer;
use crate::secrets::Secrets;
use crate::{foreground, session};

const EVENT_BUFFER: usize = 256;
const LOCK_FILE: &str = "engine.lock";

pub struct Launch {
    pub data_dir: PathBuf,
    /// The app's, under which earlier versions kept their secrets in the keychain.
    pub identifier: String,
    /// Off in tests, so they never take the dock from the app the user is running.
    pub dock: bool,
    /// Follows the app in front to switch profiles. Off in tests, so whatever app the user is
    /// in does not change the profile under them.
    pub watch_apps: bool,
    /// The plugin host; none in tests that do not need it.
    pub plugins: Option<PluginsLaunch>,
}

pub struct PluginsLaunch {
    /// The app's resources, where its Node is.
    pub resources: Option<PathBuf>,
    pub desktop: Arc<dyn Desktop>,
}

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("another copy of the app is using {0}")]
    AlreadyRunning(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct Running {
    pub engine: EngineHandle,
    pub assets: AssetStore,
    pub plugins: Option<Arc<Plugins>>,
}

/// Loads the configuration, connects to the dock and runs the engine on its own thread.
pub fn start(launch: Launch) -> Result<Running, LaunchError> {
    let store = Store::open(launch.data_dir.clone())?;
    let lock = lock_data_dir(&launch.data_dir)?;
    info!("data folder: {}", launch.data_dir.display());
    let stored = store.load()?;
    // Before the first configuration, so its page and profile get names in the user's language.
    i18n::set(i18n::resolve(
        stored
            .as_ref()
            .map(|c| c.preferences.language)
            .unwrap_or_default(),
    ));
    let config = match stored {
        Some(config) => config,
        None => {
            info!("first run, creating the default configuration");
            let mut config = config::first_run(&store.assets)?;
            store.save(&mut config)?;
            config
        }
    };

    let (inputs, input_rx) = mpsc::channel();
    let (events, _) = broadcast::channel(EVENT_BUFFER);
    let activity = Activity::new();
    let (obs, hass) = (Clients::default(), Clients::default());
    let engine = EngineHandle::new(inputs.clone(), events.clone(), obs.clone(), hass.clone());
    let engine_inputs = inputs.clone();

    let plugins = launch.plugins.and_then(|setup| {
        let plugin_inputs = inputs.clone();
        let started = Plugins::start(PluginsSetup {
            data_dir: launch.data_dir.clone(),
            resources: setup.resources,
            language: i18n::current().tag().into(),
            notify: Box::new(move |input| {
                let _ = plugin_inputs.send(Input::Plugin(input));
            }),
            desktop: setup.desktop,
        });
        match started {
            Ok(plugins) => Some(Arc::new(plugins)),
            Err(err) => {
                log::warn!("plugins cannot run: {err}");
                None
            }
        }
    });
    let plugin_keys = PluginKeys::new(
        plugins.as_ref().map(|p| p.host()),
        plugins.as_ref().map(|p| p.registry()).unwrap_or_default(),
    );

    if launch.watch_apps {
        let front_inputs = inputs.clone();
        if let Err(err) = foreground::watch(&launch.identifier, move |app| {
            front_inputs.send(Input::FrontApp(app)).is_ok()
        }) {
            log::warn!("could not watch the app in front: {err}");
        }
    }

    let (dock, dock_events) = if launch.dock {
        let lock_inputs = inputs.clone();
        if let Err(err) =
            session::watch(move |locked| lock_inputs.send(Input::Locked(locked)).is_ok())
        {
            log::warn!("could not watch the screen lock: {err}");
        }
        D200::start()
    } else {
        info!("running without the dock");
        D200::detached()
    };
    thread::Builder::new()
        .name("dock-events".into())
        .spawn(move || {
            for event in dock_events {
                if inputs.send(Input::Dock(event)).is_err() {
                    break;
                }
            }
        })?;

    let secrets = Secrets::new(&launch.data_dir, &launch.identifier);
    secrets.migrate_from_keychain(&secret_accounts(&config));
    let assets = store.assets.clone();
    let context = actions::Context {
        assets: assets.clone(),
        sounds: actions::SoundPlayer::start(),
        obs,
        hass,
        secrets,
    };
    thread::Builder::new()
        .name("engine".into())
        .spawn(move || {
            let _lock = lock;
            let engine = Engine::new(
                store,
                config,
                Renderer::new(),
                dock,
                Links {
                    events,
                    activity,
                    inputs: engine_inputs,
                    plugins: plugin_keys,
                },
                context,
            );
            if panic::catch_unwind(AssertUnwindSafe(|| engine.run(input_rx))).is_err() {
                // Without the engine the dock is dead; failing makes launchd start the app again.
                error!("the engine stopped unexpectedly, quitting");
                std::process::exit(1);
            }
        })?;
    Ok(Running {
        engine,
        assets,
        plugins,
    })
}

/// Two copies on the same data would fight over the dock and the configuration. The OS
/// releases the lock when the process exits, even after a crash.
fn lock_data_dir(data_dir: &Path) -> Result<File, LaunchError> {
    let file = File::create(data_dir.join(LOCK_FILE))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(LaunchError::AlreadyRunning(data_dir.to_path_buf())),
        Err(TryLockError::Error(err)) => Err(err.into()),
    }
}
