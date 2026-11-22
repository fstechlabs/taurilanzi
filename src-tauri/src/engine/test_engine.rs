//! An engine on a temporary folder whose docks are plugged in and out by hand, for the tests of
//! several docks.

use std::collections::BTreeMap;
use std::sync::mpsc;

use tempfile::TempDir;
use tokio::sync::broadcast;
use uuid::Uuid;

use super::handle::Activity;
use super::plugins::PluginKeys;
use super::runtime::{Engine, Links};
use crate::actions::{Context, SoundPlayer};
use crate::config::{
    Appearance, Config, DeviceSettings, KnownDevice, Page, Position, Profile, SCHEMA_VERSION,
    Store, first_device,
};
use crate::driver::{DockEvent, Event, FIRST_MODEL};
use crate::render::{Background, Color, Renderer};
use crate::secrets::Secrets;

pub struct Setup {
    pub engine: Engine,
    _dir: TempDir,
}

pub const SETTINGS: DeviceSettings = DeviceSettings {
    brightness: 50,
    sleep_minutes: 0,
};

/// A profile of `device` with an empty home page.
pub fn profile(name: &str, device: Uuid) -> Profile {
    let home = Page {
        id: Uuid::new_v4(),
        name: "Inicio".into(),
        keys: BTreeMap::new(),
        position: Position::default(),
    };
    Profile {
        id: Uuid::new_v4(),
        device,
        name: name.into(),
        home_page: home.id,
        pages: vec![home],
        apps: Vec::new(),
    }
}

/// A key with a black background and these actions.
pub fn key(actions: Vec<crate::config::Action>) -> crate::config::Key {
    crate::config::Key {
        kind: None,
        appearance: Appearance {
            background: Background::Solid {
                color: Color::BLACK,
            },
            layers: Vec::new(),
        },
        actions,
        double_press: Vec::new(),
        long_press: Vec::new(),
        second_state: None,
    }
}

/// A D200 the app knows, never connected yet.
pub fn known_d200(id: Uuid) -> KnownDevice {
    KnownDevice {
        id,
        ..first_device(SETTINGS)
    }
}

/// An engine that knows the first dock, and these other docks and profiles.
pub fn engine(others: Vec<KnownDevice>, profiles: Vec<Profile>) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let mut devices = vec![first_device(SETTINGS)];
    devices.extend(others);
    let config = Config {
        schema: SCHEMA_VERSION,
        revision: 0,
        devices,
        preferences: Default::default(),
        active_profiles: BTreeMap::new(),
        profiles,
        plugin_settings: Default::default(),
        integrations: Vec::new(),
        local_profiles: Default::default(),
        plugins: Default::default(),
        declined_plugins: Default::default(),
    };
    let (events, _) = broadcast::channel(16);
    let context = Context {
        assets: store.assets.clone(),
        sounds: SoundPlayer::start(),
        obs: Default::default(),
        hass: Default::default(),
        secrets: Secrets::new(dir.path(), "com.taurilanzi.test"),
    };
    let links = Links {
        events,
        activity: Activity::new(),
        inputs: mpsc::channel().0,
        plugins: PluginKeys::new(None, Default::default()),
    };
    let engine = Engine::new(store, config, Renderer::new(), links, context);
    Setup { engine, _dir: dir }
}

impl Setup {
    /// Plugs in a D200 with this serial; returns the known dock it became.
    pub fn plug(&mut self, serial: &str, simulated: bool) -> Uuid {
        let model = crate::driver::model(FIRST_MODEL).unwrap();
        for event in crate::driver::plugged(model, serial, simulated) {
            self.engine.on_dock(event);
        }
        let (device, _) = self
            .engine
            .docks
            .sessions
            .iter()
            .find(|(_, s)| s.info.serial == serial)
            .expect("the dock connected");
        *device
    }

    pub fn unplug(&mut self, device: Uuid) {
        let connection = self.engine.docks.get(device).unwrap().connection;
        self.engine.on_dock(DockEvent {
            connection,
            event: Event::Disconnected,
        });
    }
}
