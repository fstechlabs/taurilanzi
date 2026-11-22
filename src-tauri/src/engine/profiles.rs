//! Profiles as the sidebar edits them, the apps linked to them, and switching to the profile of
//! the app in front.

use std::collections::BTreeMap;

use log::warn;
use uuid::Uuid;

use super::app_profiles::AppProfiles;
use super::commands::CommandError;
use super::runtime::Engine;
use super::transfer::keep_switches_to;
use crate::config::{AppRef, Page, Position, Profile};

const MAX_NAME: usize = 60;

impl Engine {
    /// A new profile has one empty page, its home. It goes on the focused dock unless told
    /// another.
    pub(super) fn create_profile(
        &mut self,
        ids: NewProfile,
        name: Option<String>,
    ) -> Result<Uuid, CommandError> {
        let device = ids.device.unwrap_or(self.focused());
        if self.config.device(device).is_none() {
            return Err(CommandError::UnknownDevice(device));
        }
        for taken in [ids.id, ids.home].into_iter().flatten() {
            if self.config.id_taken(taken) {
                return Err(CommandError::IdTaken(taken));
            }
        }
        if let (Some(id), Some(home)) = (ids.id, ids.home)
            && id == home
        {
            return Err(CommandError::IdTaken(id));
        }
        let page = Page {
            id: ids.home.unwrap_or_else(Uuid::new_v4),
            name: tr!("profiles.homePage").into(),
            keys: BTreeMap::new(),
            position: Position::default(),
        };
        let name = name
            .map(|n| clean_name(&n))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.free_profile_name(device));
        let profile = Profile {
            id: ids.id.unwrap_or_else(Uuid::new_v4),
            device,
            name,
            home_page: page.id,
            pages: vec![page],
            apps: Vec::new(),
        };
        let id = profile.id;
        self.config.profiles.push(profile);
        self.save()?;
        Ok(id)
    }

    pub(super) fn rename_profile(&mut self, id: Uuid, name: String) -> Result<(), CommandError> {
        let name = clean_name(&name);
        let profile = self.profile_by_id(id)?;
        if name.is_empty() || profile.name == name {
            return Ok(());
        }
        profile.name = name;
        self.save()
    }

    /// The copy gets pages of its own, with the lines between them, right after the original.
    /// Apps stay with the original, since an app belongs to one profile.
    pub(super) fn duplicate_profile(
        &mut self,
        id: Uuid,
        copy_id: Option<Uuid>,
    ) -> Result<Uuid, CommandError> {
        if let Some(taken) = copy_id.filter(|&c| self.config.id_taken(c)) {
            return Err(CommandError::IdTaken(taken));
        }
        let at = self.profile_index(id)?;
        let mut copy = Self::fresh_copy(
            &self.config.profiles[at],
            copy_id.unwrap_or_else(Uuid::new_v4),
        );
        copy.name = clean_name(&trf!("profiles.copy", name = copy.name));
        copy.apps.clear();
        let copy_id = copy.id;
        self.config.profiles.insert(at + 1, copy);
        if self.config.local_profiles.contains(&id) {
            self.config.local_profiles.insert(copy_id);
        }
        self.save()?;
        Ok(copy_id)
    }

    /// Keys that switched to the profile stop doing so. If its dock showed it, it goes back to
    /// the profile the user picked last there, or to a neighbour.
    pub(super) fn delete_profile(&mut self, id: Uuid) -> Result<(), CommandError> {
        let at = self.profile_index(id)?;
        let device = self.config.profiles[at].device;
        let siblings: Vec<Uuid> = self.config.profiles_of(device).map(|p| p.id).collect();
        if siblings.len() == 1 {
            return Err(CommandError::LastProfile);
        }
        let on_dock = self.config.is_active(id);
        self.config.profiles.remove(at);
        for key in self
            .config
            .profiles
            .iter_mut()
            .flat_map(|p| &mut p.pages)
            .flat_map(|page| page.keys.values_mut())
        {
            keep_switches_to(key, |target| target != id);
        }
        let exists = profile_ids(&self.config.profiles);
        for app_profiles in self.app_profiles.values_mut() {
            app_profiles.retain(|p| exists.contains(&p));
        }
        if on_dock {
            let place = siblings.iter().position(|p| *p == id).unwrap_or(0);
            let neighbour = siblings
                .iter()
                .filter(|p| **p != id)
                .nth(place.min(siblings.len() - 2))
                .copied();
            let next = self
                .app_profiles
                .get(&device)
                .and_then(AppProfiles::manual)
                .or(neighbour);
            if let Some(next) = next {
                self.config.active_profiles.insert(device, next);
                self.app_profiles_of(device).chosen(next);
            }
        }
        self.save()?;
        if on_dock {
            self.show_current_page(device);
        }
        Ok(())
    }

    /// Moves a profile to another place among those of its dock.
    pub(super) fn move_profile(&mut self, id: Uuid, index: usize) -> Result<(), CommandError> {
        let at = self.profile_index(id)?;
        let profile = self.config.profiles.remove(at);
        let device = profile.device;
        let to = self
            .config
            .profiles
            .iter()
            .enumerate()
            .filter(|(_, p)| p.device == device)
            .nth(index)
            .map_or(self.config.profiles.len(), |(at, _)| at);
        self.config.profiles.insert(to, profile);
        self.save()
    }

    /// An app belongs to one profile at most on each dock, so the app in front always has a
    /// clear one there.
    pub(super) fn link_app(&mut self, id: Uuid, app: AppRef) -> Result<(), CommandError> {
        let app = AppRef {
            id: app.id.trim().to_owned(),
            name: clean_name(&app.name),
        };
        if app.id.is_empty() {
            return Err(CommandError::UnknownApp);
        }
        let device = self
            .config
            .profile(id)
            .ok_or(CommandError::UnknownProfile(id))?
            .device;
        if let Some(other) = self
            .config
            .profiles_of(device)
            .find(|p| p.id != id && p.apps.iter().any(|a| a.id == app.id))
        {
            return Err(CommandError::AppAlreadyLinked {
                app: app.name,
                profile: other.name.clone(),
            });
        }
        let profile = self.profile_by_id(id)?;
        if profile.apps.iter().any(|a| a.id == app.id) {
            return Ok(());
        }
        profile.apps.push(app);
        self.save()
    }

    pub(super) fn unlink_app(&mut self, id: Uuid, app: String) -> Result<(), CommandError> {
        let profile = self.profile_by_id(id)?;
        let before = profile.apps.len();
        profile.apps.retain(|a| a.id != app);
        if profile.apps.len() == before {
            return Ok(());
        }
        self.save()
    }

    /// Another app came to the front: each dock shows the profile it has for it. Switching for
    /// it is not an edit, so undo leaves it alone; the profile on each dock is saved, as with any
    /// other switch, so it comes back after a restart.
    pub(super) fn on_front_app(&mut self, app: AppRef) {
        if self.front_app.as_ref() != Some(&app) {
            self.front_app = Some(app.clone());
            self.changed = true;
        }
        let devices: Vec<Uuid> = self.config.devices.iter().map(|d| d.id).collect();
        for device in devices {
            let Some(active) = self.config.active_profile_of(device).map(|p| p.id) else {
                continue;
            };
            if !self.config.preferences.profiles.follow_apps {
                self.app_profiles_of(device).chosen(active);
                continue;
            }
            let linked = self
                .config
                .profiles_of(device)
                .find(|p| p.apps.iter().any(|a| a.id == app.id))
                .map(|p| p.id);
            let was_automatic = self.app_profiles_of(device).is_automatic(active);
            match self.app_profiles_of(device).front_changed(linked, active) {
                Some(next) if self.config.profiles_of(device).any(|p| p.id == next) => {
                    log::info!("{} came to the front, switching profile", app.name);
                    self.config.active_profiles.insert(device, next);
                    if let Err(err) = self.save() {
                        warn!("could not save the profile on the dock: {err}");
                    }
                    self.show_current_page(device);
                }
                _ => {
                    if was_automatic != self.app_profiles_of(device).is_automatic(active) {
                        self.changed = true;
                    }
                }
            }
        }
    }

    /// The user picked a profile for a dock, which also stops following the app in front there
    /// until another one with a profile comes up.
    pub(super) fn chose_profile(&mut self, device: Uuid, id: Uuid) {
        let active = self.config.active_profile_of(device).map(|p| p.id);
        if active.is_some_and(|active| self.app_profiles_of(device).is_automatic(active)) {
            self.changed = true;
        }
        self.app_profiles_of(device).chosen(id);
    }

    /// Undo and redo can bring back or take away whole profiles.
    pub(super) fn profiles_restored(&mut self, before: &BTreeMap<Uuid, Uuid>) {
        let exists = profile_ids(&self.config.profiles);
        for app_profiles in self.app_profiles.values_mut() {
            app_profiles.retain(|p| exists.contains(&p));
        }
        let devices: Vec<Uuid> = self.config.devices.iter().map(|d| d.id).collect();
        for device in devices {
            let Some(active) = self.config.active_profile_of(device).map(|p| p.id) else {
                continue;
            };
            if before.get(&device) != Some(&active) {
                self.config.active_profiles.insert(device, active);
                self.app_profiles_of(device).chosen(active);
            }
        }
    }

    /// The profile each dock shows now, to tell afterwards which ones changed.
    pub(super) fn active_profiles(&self) -> BTreeMap<Uuid, Uuid> {
        self.config
            .devices
            .iter()
            .filter_map(|d| Some((d.id, self.config.active_profile_of(d.id)?.id)))
            .collect()
    }

    /// Whether the profile on a dock is there because of the app in front.
    pub(super) fn automatic_profile(&self, device: Uuid) -> bool {
        let Some(active) = self.config.active_profile_of(device) else {
            return false;
        };
        self.config.preferences.profiles.follow_apps
            && self
                .app_profiles
                .get(&device)
                .is_some_and(|a| a.is_automatic(active.id))
            && self
                .front_app
                .as_ref()
                .is_some_and(|app| active.apps.iter().any(|a| a.id == app.id))
    }

    fn app_profiles_of(&mut self, device: Uuid) -> &mut AppProfiles {
        let active = self
            .config
            .active_profile_of(device)
            .map(|p| p.id)
            .unwrap_or_default();
        self.app_profiles
            .entry(device)
            .or_insert_with(|| AppProfiles::new(active))
    }

    fn profile_index(&self, id: Uuid) -> Result<usize, CommandError> {
        self.config
            .profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or(CommandError::UnknownProfile(id))
    }

    fn profile_by_id(&mut self, id: Uuid) -> Result<&mut Profile, CommandError> {
        self.config
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or(CommandError::UnknownProfile(id))
    }

    fn free_profile_name(&self, device: Uuid) -> String {
        (self.config.profiles_of(device).count() + 1..)
            .map(|n| trf!("profiles.numbered", number = n))
            .find(|name| !self.config.profiles_of(device).any(|p| &p.name == name))
            .expect("some number is free")
    }
}

/// The ids a new profile and its home page get, new ones by default, and its dock, the focused
/// one by default.
#[derive(Debug, Clone, Copy, Default)]
pub struct NewProfile {
    pub id: Option<Uuid>,
    pub home: Option<Uuid>,
    pub device: Option<Uuid>,
}

fn profile_ids(profiles: &[Profile]) -> Vec<Uuid> {
    profiles.iter().map(|p| p.id).collect()
}

pub(super) fn clean_name(name: &str) -> String {
    name.trim().chars().take(MAX_NAME).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use tempfile::TempDir;
    use tokio::sync::broadcast;

    use super::*;
    use crate::actions::{Context, SoundPlayer};
    use crate::config::{Action, Key};
    use crate::config::{Appearance, Config, DeviceSettings, KeyState, Store};
    use crate::engine::handle::Activity;
    use crate::engine::history::Target;
    use crate::engine::runtime::Links;
    use crate::render::{Background, Color, Renderer};
    use crate::secrets::Secrets;

    struct Setup {
        engine: Engine,
        _dir: TempDir,
    }

    fn page(name: &str) -> Page {
        Page {
            id: Uuid::new_v4(),
            name: name.into(),
            keys: BTreeMap::new(),
            position: Position::default(),
        }
    }

    fn profile(name: &str) -> Profile {
        let home = page("Inicio");
        Profile {
            id: Uuid::new_v4(),
            device: crate::config::FIRST_DOCK,
            name: name.into(),
            home_page: home.id,
            pages: vec![home],
            apps: Vec::new(),
        }
    }

    fn key(actions: Vec<Action>) -> Key {
        Key {
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

    fn app(id: &str) -> AppRef {
        AppRef {
            id: id.into(),
            name: id.into(),
        }
    }

    fn engine(profiles: Vec<Profile>) -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().to_path_buf()).unwrap();
        let config = Config {
            schema: crate::config::SCHEMA_VERSION,
            revision: 0,
            devices: vec![crate::config::first_device(DeviceSettings {
                brightness: 50,
                sleep_minutes: 0,
            })],
            preferences: Default::default(),
            active_profiles: std::collections::BTreeMap::from([(
                crate::config::FIRST_DOCK,
                profiles[0].id,
            )]),
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
            plugins: super::super::plugins::PluginKeys::new(None, Default::default()),
        };
        let engine = Engine::new(store, config, Renderer::new(), links, context);
        Setup { engine, _dir: dir }
    }

    fn ids(engine: &Engine) -> Vec<Uuid> {
        engine.config.profiles.iter().map(|p| p.id).collect()
    }

    fn find(engine: &Engine, id: Uuid) -> &Profile {
        engine.config.profiles.iter().find(|p| p.id == id).unwrap()
    }

    #[test]
    fn a_new_profile_has_one_empty_home_page() {
        let mut setup = engine(vec![profile("Perfil 1")]);
        let engine = &mut setup.engine;
        let id = engine.create_profile(NewProfile::default(), None).unwrap();
        let created = find(engine, id);
        assert_eq!(created.name, "Perfil 2");
        assert!(created.apps.is_empty());
        let [home] = &created.pages[..] else {
            panic!("one page");
        };
        assert_eq!(home.name, "Inicio");
        assert_eq!(created.home_page, home.id);
        assert!(home.keys.is_empty());
        let named = engine
            .create_profile(NewProfile::default(), Some("  Trabajo ".into()))
            .unwrap();
        assert_eq!(find(engine, named).name, "Trabajo");
        let blank = engine
            .create_profile(NewProfile::default(), Some("   ".into()))
            .unwrap();
        assert_eq!(find(engine, blank).name, "Perfil 4");
    }

    #[test]
    fn a_copy_gets_pages_of_its_own_and_no_apps() {
        let mut original = profile("Edición");
        let second = page("Atajos");
        let (home, there) = (original.pages[0].id, second.id);
        let mut linked = key(vec![Action::OpenPage {
            page: there,
            waypoints: Vec::new(),
        }]);
        linked.second_state = Some(KeyState {
            appearance: linked.appearance.clone(),
            actions: vec![Action::OpenPage {
                page: home,
                waypoints: Vec::new(),
            }],
        });
        original.pages[0].keys.insert(0, linked);
        original.pages.push(second);
        original.home_page = there;
        original.apps.push(app("com.figma.Desktop"));
        let other = profile("Otro");
        let (id, other_id) = (original.id, other.id);
        let mut setup = engine(vec![original.clone(), other]);
        let engine = &mut setup.engine;

        let copy_id = engine.duplicate_profile(id, None).unwrap();
        assert_eq!(ids(engine), [id, copy_id, other_id]);
        assert_eq!(find(engine, id), &original);
        let copy = find(engine, copy_id);
        assert_eq!(copy.name, "Edición (copia)");
        assert!(copy.apps.is_empty());
        let (copy_home, copy_there) = (copy.pages[0].id, copy.pages[1].id);
        assert!(![home, there].contains(&copy_home) && ![home, there].contains(&copy_there));
        assert_eq!(copy.home_page, copy_there);
        let key = &copy.pages[0].keys[&0];
        assert!(matches!(key.actions[..], [Action::OpenPage { page, .. }] if page == copy_there));
        let state = key.second_state.as_ref().unwrap();
        assert!(matches!(state.actions[..], [Action::OpenPage { page, .. }] if page == copy_home));
    }

    #[test]
    fn the_last_profile_cannot_be_deleted() {
        let only = profile("Solo");
        let id = only.id;
        let mut setup = engine(vec![only]);
        assert!(matches!(
            setup.engine.delete_profile(id),
            Err(CommandError::LastProfile)
        ));
        assert_eq!(ids(&setup.engine), [id]);
    }

    #[test]
    fn deleting_drops_the_keys_that_switched_to_it() {
        let (mut first, second) = (profile("A"), profile("B"));
        let (a, b) = (first.id, second.id);
        let to = |profile| Action::SwitchProfile { profile };
        let mut switcher = key(vec![to(b), to(a)]);
        switcher.double_press = vec![to(b)];
        switcher.long_press = vec![to(b), Action::Back];
        switcher.second_state = Some(KeyState {
            appearance: switcher.appearance.clone(),
            actions: vec![to(b)],
        });
        first.pages[0].keys.insert(0, switcher);
        let mut setup = engine(vec![first, second]);
        let engine = &mut setup.engine;

        engine.delete_profile(b).unwrap();
        assert_eq!(ids(engine), [a]);
        let key = &find(engine, a).pages[0].keys[&0];
        assert_eq!(key.actions, [to(a)]);
        assert!(key.double_press.is_empty());
        assert_eq!(key.long_press, [Action::Back]);
        assert!(key.second_state.as_ref().unwrap().actions.is_empty());
    }

    #[test]
    fn deleting_the_profile_on_the_dock_shows_a_neighbour() {
        let profiles = vec![profile("A"), profile("B"), profile("C")];
        let [a, b, c] = [profiles[0].id, profiles[1].id, profiles[2].id];
        let mut setup = engine(profiles);
        let engine = &mut setup.engine;
        engine.set_profile(b).unwrap();
        engine.delete_profile(b).unwrap();
        assert_eq!(
            engine
                .config
                .active_profile_of(crate::config::FIRST_DOCK)
                .unwrap()
                .id,
            c
        );
        engine.delete_profile(c).unwrap();
        assert_eq!(
            engine
                .config
                .active_profile_of(crate::config::FIRST_DOCK)
                .unwrap()
                .id,
            a
        );
    }

    #[test]
    fn deleting_an_automatic_profile_goes_back_to_the_chosen_one() {
        let (manual, chosen, mut linked) = (profile("A"), profile("B"), profile("C"));
        linked.apps.push(app("com.figma.Desktop"));
        let (b, c) = (chosen.id, linked.id);
        let mut setup = engine(vec![manual, chosen, linked]);
        let engine = &mut setup.engine;
        engine.set_profile(b).unwrap();
        engine.on_front_app(app("com.figma.Desktop"));
        assert_eq!(
            engine
                .config
                .active_profile_of(crate::config::FIRST_DOCK)
                .unwrap()
                .id,
            c
        );
        engine.delete_profile(c).unwrap();
        assert_eq!(
            engine
                .config
                .active_profile_of(crate::config::FIRST_DOCK)
                .unwrap()
                .id,
            b
        );
    }

    #[test]
    fn undoing_a_delete_brings_the_profile_back() {
        let (mut first, second) = (profile("A"), profile("B"));
        let (a, b) = (first.id, second.id);
        first.pages[0]
            .keys
            .insert(0, key(vec![Action::SwitchProfile { profile: b }]));
        let before = vec![first.clone(), second.clone()];
        let mut setup = engine(vec![first, second]);
        let engine = &mut setup.engine;
        engine.edit(Target::Other, |e| e.delete_profile(b)).unwrap();
        assert_eq!(ids(engine), [a]);
        engine.undo().unwrap();
        assert_eq!(engine.config.profiles, before);
        engine.set_profile(b).unwrap();
        assert_eq!(
            engine
                .config
                .active_profile_of(crate::config::FIRST_DOCK)
                .unwrap()
                .id,
            b
        );
    }

    #[test]
    fn an_app_belongs_to_one_profile() {
        let (first, second) = (profile("A"), profile("B"));
        let (a, b) = (first.id, second.id);
        let mut setup = engine(vec![first, second]);
        let engine = &mut setup.engine;
        let figma = AppRef {
            id: " com.figma.Desktop ".into(),
            name: " Figma ".into(),
        };
        engine.link_app(a, figma.clone()).unwrap();
        engine.link_app(a, figma.clone()).unwrap();
        assert_eq!(
            find(engine, a).apps,
            [AppRef {
                id: "com.figma.Desktop".into(),
                name: "Figma".into(),
            }]
        );
        match engine.link_app(b, figma) {
            Err(CommandError::AppAlreadyLinked { app, profile }) => {
                assert_eq!((app.as_str(), profile.as_str()), ("Figma", "A"));
            }
            other => panic!("expected AppAlreadyLinked, got {other:?}"),
        }
        assert!(find(engine, b).apps.is_empty());
        assert!(matches!(
            engine.link_app(b, app("  ")),
            Err(CommandError::UnknownApp)
        ));
        engine.unlink_app(a, "com.figma.Desktop".into()).unwrap();
        engine.link_app(b, app("com.figma.Desktop")).unwrap();
        assert_eq!(find(engine, b).apps.len(), 1);
    }

    #[test]
    fn follows_the_app_in_front_and_comes_back() {
        let (manual, mut linked) = (profile("A"), profile("B"));
        linked.apps.push(app("com.figma.Desktop"));
        let (a, b) = (manual.id, linked.id);
        let mut setup = engine(vec![manual, linked]);
        let engine = &mut setup.engine;

        engine.on_front_app(app("com.figma.Desktop"));
        assert_eq!(
            engine
                .config
                .active_profile_of(crate::config::FIRST_DOCK)
                .unwrap()
                .id,
            b
        );
        assert!(engine.automatic_profile(crate::config::FIRST_DOCK));
        assert_eq!(
            engine.store.load().unwrap().unwrap().active_profiles[&crate::config::FIRST_DOCK],
            b
        );

        engine.on_front_app(app("com.apple.Safari"));
        assert_eq!(
            engine
                .config
                .active_profile_of(crate::config::FIRST_DOCK)
                .unwrap()
                .id,
            a
        );
        assert!(!engine.automatic_profile(crate::config::FIRST_DOCK));
    }

    #[test]
    fn a_profile_chosen_by_hand_stays_when_the_app_goes() {
        let (manual, mut linked) = (profile("A"), profile("B"));
        linked.apps.push(app("com.figma.Desktop"));
        let b = linked.id;
        let mut setup = engine(vec![manual, linked]);
        let engine = &mut setup.engine;
        engine.on_front_app(app("com.figma.Desktop"));
        engine.set_profile(b).unwrap();
        assert!(!engine.automatic_profile(crate::config::FIRST_DOCK));
        engine.on_front_app(app("com.apple.Safari"));
        assert_eq!(
            engine
                .config
                .active_profile_of(crate::config::FIRST_DOCK)
                .unwrap()
                .id,
            b
        );
    }

    #[test]
    fn does_not_follow_apps_when_turned_off() {
        let (manual, mut linked) = (profile("A"), profile("B"));
        linked.apps.push(app("com.figma.Desktop"));
        let a = manual.id;
        let mut setup = engine(vec![manual, linked]);
        let engine = &mut setup.engine;
        engine.config.preferences.profiles.follow_apps = false;
        engine.on_front_app(app("com.figma.Desktop"));
        assert_eq!(
            engine
                .config
                .active_profile_of(crate::config::FIRST_DOCK)
                .unwrap()
                .id,
            a
        );
        assert!(!engine.automatic_profile(crate::config::FIRST_DOCK));
        assert_eq!(engine.front_app, Some(app("com.figma.Desktop")));
    }
}
