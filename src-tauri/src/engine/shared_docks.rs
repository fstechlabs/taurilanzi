//! The known docks as the computers of the account share them. One physical dock is one known
//! dock everywhere: met on two computers before they synced, it is joined into one; two units
//! that took the same dock on two computers are told apart, and the one that leaves takes a copy
//! of every profile it showed. Nothing a dock had is lost on the way.

use std::collections::{BTreeMap, BTreeSet};

use uuid::Uuid;

use super::commands::CommandError;
use super::runtime::Engine;
use super::transfer::actions_mut;
use crate::config::{
    Action, DeviceSettings, FIRST_DOCK, KnownDevice, Profile, derived_id, dock_id,
};

impl Engine {
    /// The docks the other computers know. One that is connected here, or still has profiles
    /// here, stays, and so does the unit it was matched with here. Settings from a version
    /// that knew one dock are the first dock's.
    pub(super) fn take_devices(
        &mut self,
        remote: Vec<KnownDevice>,
        legacy: DeviceSettings,
    ) -> Result<(), CommandError> {
        if remote.is_empty() {
            if self.config.device(FIRST_DOCK).is_some() {
                self.set_device(FIRST_DOCK, legacy)?;
            }
            return Ok(());
        }
        let mut devices = remote;
        // Units this computer took a dock for, which the account knows with another unit.
        let mut taken_twice = Vec::new();
        for device in &mut devices {
            let Some(here) = self.config.device(device.id) else {
                continue;
            };
            match (&here.serial, &device.serial) {
                (Some(mine), Some(theirs)) if mine != theirs => {
                    taken_twice.push((device.id, mine.clone(), theirs.clone()));
                }
                (Some(mine), None) => device.serial = Some(mine.clone()),
                _ => {}
            }
        }
        let kept: Vec<KnownDevice> = self
            .config
            .devices
            .iter()
            .filter(|d| !devices.iter().any(|r| r.id == d.id))
            .filter(|d| {
                self.docks.get(d.id).is_some() || self.config.profiles_of(d.id).next().is_some()
            })
            .cloned()
            .collect();
        devices.extend(kept);
        let settings: Vec<(Uuid, DeviceSettings)> =
            devices.iter().map(|d| (d.id, d.settings.clone())).collect();
        for device in &mut devices {
            if let Some(here) = self.config.device(device.id) {
                device.settings = here.settings.clone();
            }
        }
        self.config.devices = devices;
        for (device, mine, theirs) in taken_twice {
            self.tell_units_apart(device, mine, theirs);
        }
        self.join_same_units();
        for (device, settings) in settings {
            if self.config.device(device).is_some() {
                self.set_device(device, settings)?;
            }
        }
        Ok(())
    }

    /// The unit with the smaller serial keeps the dock, the same choice on every computer. When
    /// it is the one this computer met, nothing changes here; otherwise that unit becomes a dock
    /// of its own, with a copy of every synced profile of the dock and the profiles only this
    /// computer had, so it goes on showing what it showed.
    fn tell_units_apart(&mut self, device: Uuid, mine: String, theirs: String) {
        if mine < theirs {
            if let Some(known) = self.config.device_mut(device) {
                known.serial = Some(mine);
            }
            return;
        }
        let Some(known) = self.config.device(device).cloned() else {
            return;
        };
        let unit = dock_id(&known.model, &mine);
        if self.config.device(unit).is_none() {
            self.config.devices.push(KnownDevice {
                id: unit,
                model: known.model.clone(),
                name: String::new(),
                serial: Some(mine),
                settings: known.settings.clone(),
            });
        }
        if let Some(known) = self.config.device_mut(device) {
            known.serial = Some(theirs);
        }
        let active = self.config.active_profile_of(device).map(|p| p.id);
        let local = self.config.local_profiles.clone();
        let synced: BTreeSet<Uuid> = self
            .config
            .profiles_of(device)
            .filter(|p| !local.contains(&p.id))
            .map(|p| p.id)
            .collect();
        let copies: Vec<Profile> = self
            .config
            .profiles_of(device)
            .filter(|p| synced.contains(&p.id))
            .map(|p| copy_for(p, unit, &synced))
            .filter(|copy| !self.config.id_taken(copy.id))
            .collect();
        self.config.profiles.extend(copies);
        for profile in self.config.profiles.iter_mut() {
            if profile.device == device && local.contains(&profile.id) {
                profile.device = unit;
                for key in profile.pages.iter_mut().flat_map(|p| p.keys.values_mut()) {
                    point_switches_at(key, &synced, unit);
                }
            }
        }
        if let Some(active) = active {
            let there = if synced.contains(&active) {
                derived_id(active, unit)
            } else {
                active
            };
            self.config.active_profiles.insert(unit, there);
        }
        self.config.active_profiles.remove(&device);
        self.move_session(device, unit);
        log::info!("two docks were the same one on different computers: told apart");
    }

    /// Two known docks of the same model and serial are one unit met on two computers: they
    /// become the first dock if one of them is it, otherwise the one with the smaller id.
    fn join_same_units(&mut self) {
        let mut units: BTreeMap<(String, String), Vec<Uuid>> = BTreeMap::new();
        for device in &self.config.devices {
            if let Some(serial) = &device.serial {
                units
                    .entry((device.model.clone(), serial.clone()))
                    .or_default()
                    .push(device.id);
            }
        }
        for ids in units.into_values().filter(|ids| ids.len() > 1) {
            let keep = if ids.contains(&FIRST_DOCK) {
                FIRST_DOCK
            } else {
                *ids.iter().min().expect("two or more")
            };
            for gone in ids.into_iter().filter(|id| *id != keep) {
                self.join_into(gone, keep);
            }
        }
    }

    /// Every profile of `gone` goes to `keep`, and so does its session if it is connected.
    fn join_into(&mut self, gone: Uuid, keep: Uuid) {
        for profile in self.config.profiles.iter_mut() {
            if profile.device == gone {
                profile.device = keep;
            }
        }
        if let Some(active) = self.config.active_profiles.remove(&gone) {
            self.config.active_profiles.entry(keep).or_insert(active);
        }
        self.config.devices.retain(|d| d.id != gone);
        self.move_session(gone, keep);
        log::info!("one dock met on two computers: joined");
    }

    /// The connected unit that was `from` is `to` from now on.
    fn move_session(&mut self, from: Uuid, to: Uuid) {
        self.app_profiles.remove(&from);
        if let Some(session) = self.docks.sessions.remove(&from) {
            self.docks.sessions.insert(to, session);
        }
        if self.focus == from {
            self.focus = to;
        }
        self.changed = true;
    }
}

/// A copy of a synced profile for `device`, with ids that follow from the original's and the
/// dock's, so every computer that makes it makes the same one. Its lines between pages and its
/// switches to the other synced profiles lead to the copies.
fn copy_for(original: &Profile, device: Uuid, synced: &BTreeSet<Uuid>) -> Profile {
    let mut copy = original.clone();
    copy.id = derived_id(original.id, device);
    copy.device = device;
    let pages: BTreeMap<Uuid, Uuid> = copy
        .pages
        .iter()
        .map(|page| (page.id, derived_id(page.id, device)))
        .collect();
    copy.home_page = pages
        .get(&copy.home_page)
        .copied()
        .unwrap_or(copy.home_page);
    for page in &mut copy.pages {
        page.id = pages[&page.id];
        for key in page.keys.values_mut() {
            for action in actions_mut(key) {
                match action {
                    Action::OpenPage { page, .. } => {
                        if let Some(new) = pages.get(page) {
                            *page = *new;
                        }
                    }
                    Action::Plugin { id, .. } => *id = derived_id(*id, device),
                    _ => {}
                }
            }
            point_switches_at(key, synced, device);
        }
    }
    copy
}

/// Switches to synced profiles lead to their copies on `device`.
fn point_switches_at(key: &mut crate::config::Key, synced: &BTreeSet<Uuid>, device: Uuid) {
    for action in actions_mut(key) {
        if let Action::SwitchProfile { profile } = action
            && synced.contains(profile)
        {
            *profile = derived_id(*profile, device);
        }
    }
}
