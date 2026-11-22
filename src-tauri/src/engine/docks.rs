//! The docks connected now: which known dock each connection is, and what each one shows. A
//! dock seen for the first time becomes a known dock with a profile of its own.

use std::collections::BTreeMap;

use log::{info, warn};
use uuid::Uuid;

use super::gestures::Gestures;
use super::runtime::Engine;
use super::screen::Screen;
use super::snapshot::EngineEvent;
use crate::config::{Config, DeviceSettings, KnownDevice, derived_id, dock_id, starter_profile};
use crate::driver::{ConnectionId, DeviceInfo, DeviceModel, Dock, DockEvent, Event};
use crate::plugins::PluginDock;

/// What new docks start with.
const DEFAULT_SETTINGS: DeviceSettings = DeviceSettings {
    brightness: 70,
    sleep_minutes: 0,
};

/// A connected dock, from the moment it said who it is until it goes.
pub(super) struct Session {
    pub connection: ConnectionId,
    pub model: &'static DeviceModel,
    pub dock: Dock,
    pub info: DeviceInfo,
    /// Whether it shows the keys, the screensaver or nothing.
    pub screen: Screen,
    /// Presses being told apart, which only make sense on the dock they happen on.
    pub gestures: Gestures,
}

#[derive(Default)]
pub(super) struct Docks {
    /// Found, but not yet told who they are, and whether they exist only in the app.
    attached: BTreeMap<ConnectionId, (&'static DeviceModel, Dock, bool)>,
    /// By known dock.
    pub sessions: BTreeMap<Uuid, Session>,
    /// A dock went away since the app started.
    lost: bool,
}

impl Docks {
    pub fn get(&self, device: Uuid) -> Option<&Session> {
        self.sessions.get(&device)
    }

    pub fn get_mut(&mut self, device: Uuid) -> Option<&mut Session> {
        self.sessions.get_mut(&device)
    }

    pub fn ids(&self) -> Vec<Uuid> {
        self.sessions.keys().copied().collect()
    }

    fn of_connection(&self, connection: ConnectionId) -> Option<Uuid> {
        self.sessions
            .iter()
            .find(|(_, s)| s.connection == connection)
            .map(|(id, _)| *id)
    }
}

impl Engine {
    pub(super) fn on_dock(&mut self, event: DockEvent) {
        let DockEvent { connection, event } = event;
        match event {
            Event::Attached {
                model,
                dock,
                simulated,
            } => {
                self.docks
                    .attached
                    .insert(connection, (model, dock, simulated));
            }
            Event::Connected(info) => self.connected(connection, info),
            Event::Key { index, pressed } => {
                let Some(device) = self.docks.of_connection(connection) else {
                    return;
                };
                self.activity.touch();
                if self.paused {
                    return;
                }
                if !self.showing_keys(device) {
                    if pressed {
                        self.press_while_away(device);
                    }
                    return;
                }
                self.emit(EngineEvent::Key {
                    device,
                    index,
                    pressed,
                });
                self.on_key(device, index, pressed);
            }
            Event::Disconnected => {
                self.docks.attached.remove(&connection);
                if let Some(device) = self.docks.of_connection(connection) {
                    let session = self.docks.sessions.remove(&device);
                    self.docks.lost = true;
                    if let Some(session) = session {
                        info!("{} gone, waiting for it to come back", session.model.name);
                    }
                    self.changed = true;
                    self.tell_plugins_of_docks();
                }
            }
        }
    }

    fn connected(&mut self, connection: ConnectionId, info: DeviceInfo) {
        let Some((model, dock, simulated)) = self.docks.attached.remove(&connection) else {
            return;
        };
        info!(
            "{} ready: {} with firmware {}",
            model.name, info.model, info.firmware
        );
        let device = self.known_device(model, &info, simulated);
        let mut gestures = Gestures::default();
        gestures.set_timing(super::display::timing(&self.config.preferences));
        self.docks.sessions.insert(
            device,
            Session {
                connection,
                model,
                dock,
                info,
                screen: Screen::default(),
                gestures,
            },
        );
        if self.config.device(self.focus).is_none() {
            self.focus = device;
        }
        let settings = self.settings_of(device);
        if let Some(session) = self.docks.get(device) {
            session
                .dock
                .set_brightness(if self.paused { 0 } else { settings.brightness });
            session.dock.set_sleep_minutes(settings.sleep_minutes);
        }
        self.store_screensaver(device);
        self.show_current_page(device);
        self.changed = true;
        self.tell_plugins_of_docks();
    }

    /// The known dock a connected one is: the same unit seen before, the first dock of its model
    /// that no unit took yet, or a new one with a starter profile. A dock that exists only in the
    /// app never takes one a real unit may be.
    fn known_device(
        &mut self,
        model: &'static DeviceModel,
        info: &DeviceInfo,
        simulated: bool,
    ) -> Uuid {
        let connected = self.docks.ids();
        let free = |d: &&KnownDevice| d.model == model.id && !connected.contains(&d.id);
        let same_unit = self
            .config
            .devices
            .iter()
            .filter(free)
            .find(|d| d.serial.as_deref() == Some(info.serial.as_str()))
            .map(|d| d.id);
        let unclaimed = || {
            if simulated {
                return None;
            }
            self.config
                .devices
                .iter()
                .filter(free)
                .find(|d| d.serial.is_none())
                .map(|d| d.id)
        };
        let id = match same_unit.or_else(unclaimed) {
            Some(id) => id,
            None => {
                let id = if model.stable_serial {
                    dock_id(model.id, &info.serial)
                } else {
                    Uuid::new_v4()
                };
                if self.config.device(id).is_none() {
                    self.config.devices.push(KnownDevice {
                        id,
                        model: model.id.into(),
                        name: String::new(),
                        serial: None,
                        settings: DEFAULT_SETTINGS,
                    });
                }
                id
            }
        };
        let mut save = false;
        if let Some(known) = self.config.device_mut(id)
            && known.serial.is_none()
        {
            known.serial = Some(info.serial.clone());
            save = true;
        }
        save |= self.give_a_profile(id);
        if save && let Err(err) = self.save() {
            warn!("could not save the new dock: {err}");
        }
        id
    }

    /// A dock with no profile gets one to start with. Its ids follow from the dock's, so two
    /// computers that give one to the same dock give the same one. Returns whether it did.
    pub(super) fn give_a_profile(&mut self, device: Uuid) -> bool {
        if self.config.profiles_of(device).next().is_some() {
            return false;
        }
        let Some(model) = self.model_of(device) else {
            return false;
        };
        let profile = derived_id(device, device);
        let ids = (profile, derived_id(profile, device));
        if self.config.id_taken(profile) {
            return false;
        }
        match starter_profile(model, device, ids, &self.store.assets) {
            Ok(profile) => {
                self.config.active_profiles.insert(device, profile.id);
                self.config.profiles.push(profile);
                true
            }
            Err(err) => {
                warn!("could not make a profile for the dock: {err}");
                false
            }
        }
    }

    pub(super) fn settings_of(&self, device: Uuid) -> DeviceSettings {
        self.config
            .device(device)
            .map(|d| d.settings.clone())
            .unwrap_or(DEFAULT_SETTINGS)
    }

    /// The model of a known dock, if this version of the app knows it.
    pub(super) fn model_of(&self, device: Uuid) -> Option<&'static DeviceModel> {
        if let Some(session) = self.docks.get(device) {
            return Some(session.model);
        }
        crate::driver::model(&self.config.device(device)?.model)
    }

    pub(super) fn showing_keys(&self, device: Uuid) -> bool {
        self.docks
            .get(device)
            .is_none_or(|session| session.screen.showing_keys())
    }

    /// The dock the editor shows, which commands without one are about.
    pub(super) fn focused(&self) -> Uuid {
        self.focus
    }

    pub(super) fn focus_device(&mut self, device: Uuid) -> Result<(), super::CommandError> {
        if self.config.device(device).is_none() {
            return Err(super::CommandError::UnknownDevice(device));
        }
        if self.focus != device {
            self.focus = device;
            self.changed = true;
        }
        Ok(())
    }

    pub(super) fn device_name(&self, device: Uuid) -> String {
        name_of(&self.config, device)
    }

    /// Stream Deck plugins hear of the docks that came or went. Until a dock connects, or one
    /// goes, they take the focused one for connected, as when the app knew a single dock.
    pub(super) fn tell_plugins_of_docks(&self) {
        let Some(host) = self.plugin_keys.host() else {
            return;
        };
        let mut connected: std::collections::BTreeSet<String> =
            self.docks.ids().iter().map(Uuid::to_string).collect();
        if connected.is_empty() && !self.docks.lost {
            connected.insert(self.focused().to_string());
        }
        host.set_docks(plugin_docks(&self.config), connected);
    }
}

/// How the app names a dock: the user's name, or its model's, with its serial number when there
/// are two alike.
pub(super) fn name_of(config: &Config, device: Uuid) -> String {
    let Some(known) = config.device(device) else {
        return String::new();
    };
    if !known.name.trim().is_empty() {
        return known.name.clone();
    }
    let model = crate::driver::model(&known.model).map_or(known.model.as_str(), |m| m.name);
    let alike = config
        .devices
        .iter()
        .filter(|d| d.model == known.model && d.name.trim().is_empty())
        .count();
    match &known.serial {
        Some(serial) if alike > 1 => format!("{model} ({})", serial.trim()),
        _ => model.to_owned(),
    }
}

/// Every known dock of a model this version knows, as Stream Deck plugins are told of them.
pub(super) fn plugin_docks(config: &Config) -> Vec<PluginDock> {
    config
        .devices
        .iter()
        .filter_map(|known| {
            let model = crate::driver::model(&known.model)?;
            Some(PluginDock {
                id: known.id.to_string(),
                name: name_of(config, known.id),
                columns: model.columns,
                rows: model.rows,
                kind: model.plugins.elgato_type,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FIRST_DOCK, first_device};

    fn config(devices: Vec<KnownDevice>) -> Config {
        Config {
            schema: crate::config::SCHEMA_VERSION,
            revision: 0,
            devices,
            preferences: Default::default(),
            active_profiles: Default::default(),
            profiles: Vec::new(),
            plugin_settings: Default::default(),
            integrations: Vec::new(),
            local_profiles: Default::default(),
            plugins: Default::default(),
            declined_plugins: Default::default(),
        }
    }

    #[test]
    fn docks_alike_are_told_apart_by_their_serial() {
        let first = first_device(DEFAULT_SETTINGS);
        let mut config = config(vec![first.clone()]);
        assert_eq!(
            name_of(&config, FIRST_DOCK),
            "Ulanzi D200",
            "alone, its model"
        );
        let other = Uuid::new_v4();
        config.devices.push(KnownDevice {
            id: other,
            serial: Some("SN-TEST-42".into()),
            ..first
        });
        assert_eq!(name_of(&config, other), "Ulanzi D200 (SN-TEST-42)");
        assert_eq!(name_of(&config, FIRST_DOCK), "Ulanzi D200", "no serial yet");
        config.devices[0].name = "Mesa".into();
        assert_eq!(name_of(&config, FIRST_DOCK), "Mesa");
        assert_eq!(
            name_of(&config, other),
            "Ulanzi D200",
            "the other one is alone now"
        );
    }

    mod plugged {
        use crate::config::{FIRST_DOCK, derived_id, dock_id};
        use crate::engine::test_engine::{engine, known_d200, profile};
        use uuid::Uuid;

        #[test]
        fn the_first_d200_takes_the_first_dock_and_keeps_it_by_its_serial() {
            let mut setup = engine(Vec::new(), vec![profile("Mesa", FIRST_DOCK)]);
            assert_eq!(setup.plug("SN-A", false), FIRST_DOCK);
            let known = setup.engine.config.device(FIRST_DOCK).unwrap();
            assert_eq!(known.serial.as_deref(), Some("SN-A"));
            setup.unplug(FIRST_DOCK);
            assert!(setup.engine.docks.get(FIRST_DOCK).is_none());
            assert_eq!(setup.plug("SN-A", false), FIRST_DOCK, "the same unit again");
            assert_eq!(
                setup.engine.config.profiles.len(),
                1,
                "no profile was added"
            );
        }

        #[test]
        fn another_unit_is_a_dock_of_its_own_with_a_profile_to_start_with() {
            let mut setup = engine(Vec::new(), vec![profile("Mesa", FIRST_DOCK)]);
            setup.plug("SN-A", false);
            let other = setup.plug("SN-B", false);
            assert_eq!(other, dock_id("ulanzi.d200", "SN-B"));
            let profiles: Vec<Uuid> = setup
                .engine
                .config
                .profiles_of(other)
                .map(|p| p.id)
                .collect();
            assert_eq!(profiles, [derived_id(other, other)]);
            assert_eq!(
                setup.engine.config.active_profile_of(other).unwrap().id,
                profiles[0]
            );
            assert_eq!(setup.engine.config.profiles_of(FIRST_DOCK).count(), 1);
        }

        #[test]
        fn two_computers_make_the_same_dock_and_profile_for_a_new_unit() {
            let made = || {
                let mut setup = engine(Vec::new(), vec![profile("Mesa", FIRST_DOCK)]);
                setup.plug("SN-A", false);
                let dock = setup.plug("SN-B", false);
                let profile = setup
                    .engine
                    .config
                    .profiles_of(dock)
                    .next()
                    .unwrap()
                    .clone();
                (dock, profile.id, profile.home_page)
            };
            assert_eq!(made(), made());
        }

        #[test]
        fn a_virtual_dock_never_takes_one_a_real_unit_may_be() {
            let mut setup = engine(Vec::new(), vec![profile("Mesa", FIRST_DOCK)]);
            let virtual_dock = setup.plug("VIRTUAL-1", true);
            assert_ne!(virtual_dock, FIRST_DOCK);
            assert_eq!(setup.engine.config.device(FIRST_DOCK).unwrap().serial, None);
            assert_eq!(setup.plug("SN-A", false), FIRST_DOCK);
        }

        #[test]
        fn a_unit_takes_a_dock_of_its_model_that_no_unit_took_yet() {
            let synced = Uuid::new_v4();
            let mut setup = engine(
                vec![known_d200(synced)],
                vec![profile("Mesa", FIRST_DOCK), profile("Sala", synced)],
            );
            setup.plug("SN-A", false);
            assert_eq!(setup.plug("SN-B", false), synced);
        }
    }
}
