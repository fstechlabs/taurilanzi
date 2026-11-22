//! Profiles taken to another dock, and docks renamed or forgotten. A profile belongs to one dock;
//! moving or copying it to another keeps every key the other dock has a place for, by its place
//! in the grid, and leaves out the rest.

use std::collections::BTreeMap;

use serde::Serialize;
use uuid::Uuid;

use super::commands::CommandError;
use super::history::History;
use super::plugins::plugin_action;
use super::profiles::clean_name;
use super::runtime::Engine;
use crate::config::{Action, Key, Profile};
use crate::driver::DeviceModel;

const MAX_NAME: usize = 60;

/// A key of a profile, for the list of what taking it to another dock changes.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct TransferredKey {
    pub page: Uuid,
    pub page_name: String,
    pub index: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct TransferPlan {
    /// Keys on places the other dock does not have: they are left out.
    pub removed: Vec<TransferredKey>,
    /// Keys that go, but whose action the other dock cannot run: they show an alert there.
    pub unsupported: Vec<TransferredKey>,
    /// The profile is the only one of a connected dock, which cannot be left without one: it
    /// can be copied, not moved.
    pub only_profile: bool,
}

/// Where each key of `from` goes on `to`: the control in the same cell of the grid.
fn places(from: Option<&DeviceModel>, to: &DeviceModel) -> impl Fn(u8) -> Option<u8> {
    let cells: BTreeMap<u8, (u8, u8)> = from
        .map(|m| {
            m.controls
                .iter()
                .map(|c| (c.index, (c.cell.row, c.cell.column)))
                .collect()
        })
        .unwrap_or_default();
    let targets: BTreeMap<(u8, u8), u8> = to
        .controls
        .iter()
        .map(|c| ((c.cell.row, c.cell.column), c.index))
        .collect();
    let same = from.is_some_and(|m| m.id == to.id);
    let known = from.is_some();
    let indexes: Vec<u8> = to.indexes().collect();
    move |index| {
        if same || !known {
            return indexes.contains(&index).then_some(index);
        }
        cells
            .get(&index)
            .and_then(|cell| targets.get(cell))
            .copied()
    }
}

impl Engine {
    pub(super) fn transfer_plan(
        &self,
        profile: Uuid,
        device: Uuid,
    ) -> Result<TransferPlan, CommandError> {
        let original = self
            .config
            .profile(profile)
            .ok_or(CommandError::UnknownProfile(profile))?;
        let to = self
            .model_of(device)
            .ok_or(CommandError::UnknownDevice(device))?;
        let place = places(self.model_of(original.device), to);
        let mut plan = TransferPlan {
            removed: Vec::new(),
            unsupported: Vec::new(),
            only_profile: original.device != device && self.only_profile_of_connected(original),
        };
        for page in &original.pages {
            for (&index, key) in &page.keys {
                let at = TransferredKey {
                    page: page.id,
                    page_name: page.name.clone(),
                    index,
                };
                match place(index) {
                    None => plan.removed.push(at),
                    Some(_) if !self.key_runs_on(key, to) => plan.unsupported.push(at),
                    Some(_) => {}
                }
            }
        }
        Ok(plan)
    }

    fn only_profile_of_connected(&self, profile: &Profile) -> bool {
        self.docks.get(profile.device).is_some()
            && self.config.profiles_of(profile.device).count() == 1
    }

    /// Moves a profile to `device`, or puts a copy of it there with the id `copy`. Returns the
    /// id of the profile on the other dock.
    pub(super) fn transfer_profile(
        &mut self,
        profile: Uuid,
        device: Uuid,
        copy: Option<Uuid>,
    ) -> Result<Uuid, CommandError> {
        if let Some(taken) = copy.filter(|&c| self.config.id_taken(c)) {
            return Err(CommandError::IdTaken(taken));
        }
        let original = self
            .config
            .profile(profile)
            .ok_or(CommandError::UnknownProfile(profile))?
            .clone();
        let to = self
            .model_of(device)
            .ok_or(CommandError::UnknownDevice(device))?;
        let source = original.device;
        if source == device && copy.is_none() {
            return Ok(profile);
        }
        if copy.is_none() && self.only_profile_of_connected(&original) {
            return Err(CommandError::OnlyProfile);
        }
        let place = places(self.model_of(source), to);
        let mut moved = match copy {
            Some(id) => {
                let mut fresh = Self::fresh_copy(&original, id);
                fresh.apps.clear();
                fresh
            }
            None => original,
        };
        for page in &mut moved.pages {
            page.keys = std::mem::take(&mut page.keys)
                .into_iter()
                .filter_map(|(index, key)| place(index).map(|at| (at, key)))
                .collect();
        }
        moved.device = device;
        let on_target: Vec<Uuid> = self.config.profiles_of(device).map(|p| p.id).collect();
        for key in moved.pages.iter_mut().flat_map(|p| p.keys.values_mut()) {
            keep_switches_to(key, |target| {
                target == moved.id || on_target.contains(&target)
            });
        }
        let linked_there: Vec<String> = self
            .config
            .profiles_of(device)
            .flat_map(|p| p.apps.iter().map(|a| a.id.clone()))
            .collect();
        moved.apps.retain(|app| !linked_there.contains(&app.id));
        let names_there: Vec<String> = self
            .config
            .profiles_of(device)
            .map(|p| p.name.clone())
            .collect();
        while names_there.contains(&moved.name) {
            let next = clean_name(&trf!("profiles.copy", name = moved.name));
            if next == moved.name {
                break;
            }
            moved.name = next;
        }
        let id = moved.id;
        // Last of the other dock's profiles, where the user looks for it.
        match copy {
            Some(_) => {
                if self.config.local_profiles.contains(&profile) {
                    self.config.local_profiles.insert(id);
                }
                self.config.profiles.push(moved);
            }
            None => {
                let at = self.profile_position(profile)?;
                self.config.profiles.remove(at);
                self.config.profiles.push(moved);
                for key in self
                    .config
                    .profiles
                    .iter_mut()
                    .filter(|p| p.device == source)
                    .flat_map(|p| &mut p.pages)
                    .flat_map(|page| page.keys.values_mut())
                {
                    keep_switches_to(key, |target| target != profile);
                }
                if let Some(app_profiles) = self.app_profiles.get_mut(&source) {
                    app_profiles.retain(|p| p != profile);
                }
                if self.config.active_profiles.get(&source) == Some(&profile) {
                    self.config.active_profiles.remove(&source);
                }
            }
        }
        if !self.config.active_profiles.contains_key(&device)
            || self.config.profiles_of(device).count() == 1
        {
            self.config.active_profiles.insert(device, id);
        }
        self.save()?;
        self.show_current_page(source);
        self.show_current_page(device);
        Ok(id)
    }

    pub(super) fn rename_device(&mut self, device: Uuid, name: String) -> Result<(), CommandError> {
        let name: String = name.trim().chars().take(MAX_NAME).collect();
        let known = self
            .config
            .device_mut(device)
            .ok_or(CommandError::UnknownDevice(device))?;
        if known.name == name {
            return Ok(());
        }
        known.name = name;
        self.save()
    }

    /// Drops a dock that is not connected, and its profiles. Undo cannot bring them back, so it
    /// forgets everything before.
    pub(super) fn forget_device(&mut self, device: Uuid) -> Result<(), CommandError> {
        if self.config.device(device).is_none() {
            return Err(CommandError::UnknownDevice(device));
        }
        if self.docks.get(device).is_some() {
            return Err(CommandError::DeviceConnected);
        }
        if self.config.devices.len() == 1 {
            return Err(CommandError::LastDevice);
        }
        self.config.devices.retain(|d| d.id != device);
        self.config.profiles.retain(|p| p.device != device);
        self.config.active_profiles.remove(&device);
        self.app_profiles.remove(&device);
        let exists: Vec<Uuid> = self.config.profiles.iter().map(|p| p.id).collect();
        self.config.local_profiles.retain(|id| exists.contains(id));
        if self.focus == device {
            self.focus = self
                .docks
                .ids()
                .first()
                .copied()
                .or_else(|| self.config.devices.first().map(|d| d.id))
                .unwrap_or_default();
        }
        self.history = History::default();
        self.forget_stale_states();
        self.save()
    }

    fn profile_position(&self, id: Uuid) -> Result<usize, CommandError> {
        self.config
            .profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or(CommandError::UnknownProfile(id))
    }

    /// A copy with ids of its own for it, its pages and its plugin keys, and its lines between
    /// pages leading to the copies.
    pub(super) fn fresh_copy(original: &Profile, id: Uuid) -> Profile {
        let mut copy = original.clone();
        copy.id = id;
        Self::renew_plugin_ids(copy.pages.iter_mut().flat_map(|p| p.keys.values_mut()));
        let new_ids: BTreeMap<Uuid, Uuid> = copy
            .pages
            .iter()
            .map(|page| (page.id, Uuid::new_v4()))
            .collect();
        for page in &mut copy.pages {
            page.id = new_ids[&page.id];
            for key in page.keys.values_mut() {
                for action in actions_mut(key) {
                    if let Action::OpenPage { page, .. } = action
                        && let Some(new) = new_ids.get(page)
                    {
                        *page = *new;
                    }
                }
            }
        }
        copy.home_page = new_ids
            .get(&copy.home_page)
            .copied()
            .unwrap_or(copy.home_page);
        copy
    }

    /// Whether every action of a key can run on a dock of this model.
    pub(super) fn key_runs_on(&self, key: &Key, model: &DeviceModel) -> bool {
        plugin_action(key).is_none_or(|(action, _, _)| self.plugin_runs_on(action, model))
    }
}

/// Every list of actions of a key, on press, on its gestures and in its second state.
pub(super) fn actions_mut(key: &mut Key) -> impl Iterator<Item = &mut Action> {
    key.actions
        .iter_mut()
        .chain(&mut key.double_press)
        .chain(&mut key.long_press)
        .chain(
            key.second_state
                .iter_mut()
                .flat_map(|state| &mut state.actions),
        )
}

/// Drops the switches to profiles that `keep` turns down.
pub(super) fn keep_switches_to(key: &mut Key, keep: impl Fn(Uuid) -> bool) {
    let stays = |action: &Action| match action {
        Action::SwitchProfile { profile } => keep(*profile),
        _ => true,
    };
    key.actions.retain(stays);
    key.double_press.retain(stays);
    key.long_press.retain(stays);
    if let Some(state) = &mut key.second_state {
        state.actions.retain(stays);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FIRST_DOCK;
    use crate::engine::Change;
    use crate::engine::test_engine::{engine, key, known_d200, profile};

    fn move_to(engine: &mut Engine, profile: Uuid, device: Uuid, copy: Option<Uuid>) {
        engine
            .apply(vec![Change::TransferProfile {
                profile,
                device,
                copy,
            }])
            .unwrap();
    }

    fn names_on(engine: &Engine, device: Uuid) -> Vec<String> {
        engine
            .config
            .profiles_of(device)
            .map(|p| p.name.clone())
            .collect()
    }

    #[test]
    fn a_profile_moves_to_another_dock_and_undo_brings_it_back() {
        let other = Uuid::new_v4();
        let mut work = profile("Trabajo", FIRST_DOCK);
        let home = work.home_page;
        work.pages[0].keys.insert(4, key(Vec::new()));
        let id = work.id;
        let mut setup = engine(
            vec![known_d200(other)],
            vec![profile("Mesa", FIRST_DOCK), work, profile("Sala", other)],
        );
        let plan = setup.engine.transfer_plan(id, other).unwrap();
        assert!(plan.removed.is_empty() && plan.unsupported.is_empty());
        assert!(!plan.only_profile);

        move_to(&mut setup.engine, id, other, None);
        assert_eq!(names_on(&setup.engine, FIRST_DOCK), ["Mesa"]);
        assert_eq!(names_on(&setup.engine, other), ["Sala", "Trabajo"]);
        let moved = setup.engine.config.profile(id).unwrap();
        assert_eq!(moved.home_page, home, "the same profile, not a copy");
        assert!(moved.pages[0].keys.contains_key(&4), "its keys go with it");

        setup.engine.undo().unwrap();
        assert_eq!(names_on(&setup.engine, FIRST_DOCK), ["Mesa", "Trabajo"]);
        assert_eq!(names_on(&setup.engine, other), ["Sala"]);
    }

    #[test]
    fn the_only_profile_of_a_connected_dock_is_copied_not_moved() {
        let other = Uuid::new_v4();
        let only = profile("Mesa", FIRST_DOCK);
        let id = only.id;
        let mut setup = engine(vec![known_d200(other)], vec![only, profile("Mesa", other)]);
        setup.plug("SN-A", false);
        assert!(setup.engine.transfer_plan(id, other).unwrap().only_profile);
        let refused = setup.engine.apply(vec![Change::TransferProfile {
            profile: id,
            device: other,
            copy: None,
        }]);
        assert!(matches!(
            refused,
            Err(CommandError::Change { error, .. }) if matches!(*error, CommandError::OnlyProfile)
        ));

        let copy = Uuid::new_v4();
        move_to(&mut setup.engine, id, other, Some(copy));
        assert_eq!(names_on(&setup.engine, FIRST_DOCK), ["Mesa"]);
        assert_eq!(
            names_on(&setup.engine, other),
            ["Mesa", "Mesa (copia)"],
            "a name of its own on the other dock"
        );
        let copied = setup.engine.config.profile(copy).unwrap();
        assert_ne!(
            copied.home_page,
            setup.engine.config.profile(id).unwrap().home_page
        );
    }

    #[test]
    fn a_moved_profile_only_switches_to_profiles_of_its_new_dock() {
        let other = Uuid::new_v4();
        let mesa = profile("Mesa", FIRST_DOCK);
        let mut work = profile("Trabajo", FIRST_DOCK);
        work.pages[0].keys.insert(
            0,
            key(vec![
                Action::SwitchProfile { profile: mesa.id },
                Action::SwitchProfile { profile: work.id },
            ]),
        );
        let id = work.id;
        let mut setup = engine(
            vec![known_d200(other)],
            vec![mesa, work, profile("Sala", other)],
        );
        move_to(&mut setup.engine, id, other, None);
        let moved = setup.engine.config.profile(id).unwrap();
        assert_eq!(
            moved.pages[0].keys[&0].actions,
            [Action::SwitchProfile { profile: id }]
        );
    }

    #[test]
    fn a_dock_is_forgotten_with_its_profiles_only_while_unplugged() {
        let other = Uuid::new_v4();
        let mut setup = engine(
            vec![known_d200(other)],
            vec![profile("Mesa", FIRST_DOCK), profile("Sala", other)],
        );
        setup.plug("SN-A", false);
        assert!(matches!(
            setup.engine.forget_device(FIRST_DOCK),
            Err(CommandError::DeviceConnected)
        ));
        setup.engine.forget_device(other).unwrap();
        assert!(setup.engine.config.device(other).is_none());
        assert_eq!(names_on(&setup.engine, FIRST_DOCK), ["Mesa"]);
        assert_eq!(setup.engine.config.profiles.len(), 1);

        setup.unplug(FIRST_DOCK);
        assert!(matches!(
            setup.engine.forget_device(FIRST_DOCK),
            Err(CommandError::LastDevice)
        ));
    }

    #[test]
    fn a_dock_is_renamed_and_an_empty_name_gives_back_its_model() {
        let mut setup = engine(Vec::new(), vec![profile("Mesa", FIRST_DOCK)]);
        let long = format!("  {}  ", "x".repeat(80));
        setup.engine.rename_device(FIRST_DOCK, long).unwrap();
        assert_eq!(setup.engine.device_name(FIRST_DOCK), "x".repeat(MAX_NAME));
        setup.engine.rename_device(FIRST_DOCK, " ".into()).unwrap();
        assert_eq!(setup.engine.device_name(FIRST_DOCK), "Ulanzi D200");
    }
}
