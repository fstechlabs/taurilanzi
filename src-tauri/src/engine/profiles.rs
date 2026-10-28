//! Profiles as the sidebar edits them, the apps linked to them, and switching to the profile of
//! the app in front.

use std::collections::{BTreeMap, HashMap};

use log::warn;
use uuid::Uuid;

use super::commands::CommandError;
use super::runtime::Engine;
use crate::config::{Action, AppRef, Key, Page, Position, Profile};

const MAX_NAME: usize = 60;

impl Engine {
    /// A new profile has one empty page, its home.
    pub(super) fn create_profile(&mut self, name: Option<String>) -> Result<Uuid, CommandError> {
        let page = Page {
            id: Uuid::new_v4(),
            name: tr!("profiles.homePage").into(),
            keys: BTreeMap::new(),
            position: Position::default(),
        };
        let name = name
            .map(|n| clean_name(&n))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.free_profile_name());
        let profile = Profile {
            id: Uuid::new_v4(),
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
    pub(super) fn duplicate_profile(&mut self, id: Uuid) -> Result<Uuid, CommandError> {
        let at = self.profile_index(id)?;
        let mut copy = self.config.profiles[at].clone();
        copy.id = Uuid::new_v4();
        copy.name = clean_name(&trf!("profiles.copy", name = copy.name));
        copy.apps.clear();
        Self::renew_plugin_ids(copy.pages.iter_mut().flat_map(|p| p.keys.values_mut()));
        let new_ids: HashMap<Uuid, Uuid> = copy
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
        let copy_id = copy.id;
        self.config.profiles.insert(at + 1, copy);
        if self.config.local_profiles.contains(&id) {
            self.config.local_profiles.insert(copy_id);
        }
        self.save()?;
        Ok(copy_id)
    }

    /// Keys that switched to the profile stop doing so. If the dock showed it, it goes back to
    /// the profile the user picked last, or to a neighbour.
    pub(super) fn delete_profile(&mut self, id: Uuid) -> Result<(), CommandError> {
        let at = self.profile_index(id)?;
        if self.config.profiles.len() == 1 {
            return Err(CommandError::LastProfile);
        }
        self.config.profiles.remove(at);
        for key in self
            .config
            .profiles
            .iter_mut()
            .flat_map(|p| &mut p.pages)
            .flat_map(|page| page.keys.values_mut())
        {
            drop_switch_to(key, id);
        }
        let exists = profile_ids(&self.config.profiles);
        self.app_profiles.retain(|p| exists.contains(&p));
        let on_dock = self.config.active_profile == id;
        if on_dock {
            let next = self
                .app_profiles
                .manual()
                .unwrap_or(self.config.profiles[at.min(self.config.profiles.len() - 1)].id);
            self.config.active_profile = next;
            self.app_profiles.chosen(next);
        }
        self.save()?;
        if on_dock {
            self.show_current_page();
        }
        Ok(())
    }

    /// Moves a profile to another place in the list.
    pub(super) fn move_profile(&mut self, id: Uuid, index: usize) -> Result<(), CommandError> {
        let at = self.profile_index(id)?;
        let profile = self.config.profiles.remove(at);
        let to = index.min(self.config.profiles.len());
        self.config.profiles.insert(to, profile);
        self.save()
    }

    /// An app belongs to one profile at most, so the app in front always has a clear one.
    pub(super) fn link_app(&mut self, id: Uuid, app: AppRef) -> Result<(), CommandError> {
        let app = AppRef {
            id: app.id.trim().to_owned(),
            name: clean_name(&app.name),
        };
        if app.id.is_empty() {
            return Err(CommandError::UnknownApp);
        }
        if let Some(other) = self
            .config
            .profiles
            .iter()
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

    /// Another app came to the front. Switching for it is not an edit, so undo leaves it alone;
    /// the profile on the dock is saved, as with any other switch, so it comes back after a
    /// restart.
    pub(super) fn on_front_app(&mut self, app: AppRef) {
        if self.front_app.as_ref() != Some(&app) {
            self.front_app = Some(app.clone());
            self.changed = true;
        }
        let active = self.config.active_profile;
        if !self.config.preferences.profiles.follow_apps {
            self.app_profiles.chosen(active);
            return;
        }
        let linked = self
            .config
            .profiles
            .iter()
            .find(|p| p.apps.iter().any(|a| a.id == app.id))
            .map(|p| p.id);
        let was_automatic = self.app_profiles.is_automatic(active);
        match self.app_profiles.front_changed(linked, active) {
            Some(next) if self.config.profiles.iter().any(|p| p.id == next) => {
                log::info!("{} came to the front, switching profile", app.name);
                self.config.active_profile = next;
                if let Err(err) = self.save() {
                    warn!("could not save the profile on the dock: {err}");
                }
                self.show_current_page();
            }
            _ => {
                if was_automatic != self.app_profiles.is_automatic(active) {
                    self.changed = true;
                }
            }
        }
    }

    /// The user picked a profile, which also stops following the app in front until another
    /// one with a profile comes up.
    pub(super) fn chose_profile(&mut self, id: Uuid) {
        if self.app_profiles.is_automatic(self.config.active_profile) {
            self.changed = true;
        }
        self.app_profiles.chosen(id);
    }

    /// Undo and redo can bring back or take away whole profiles.
    pub(super) fn profiles_restored(&mut self, before: Uuid) {
        let exists = profile_ids(&self.config.profiles);
        self.app_profiles.retain(|p| exists.contains(&p));
        if self.config.active_profile != before {
            self.app_profiles.chosen(self.config.active_profile);
        }
    }

    /// Whether the profile on the dock is there because of the app in front.
    pub(super) fn automatic_profile(&self) -> bool {
        let active = self.config.active_profile;
        self.config.preferences.profiles.follow_apps
            && self.app_profiles.is_automatic(active)
            && self.front_app.as_ref().is_some_and(|app| {
                self.config
                    .active_profile()
                    .is_some_and(|p| p.apps.iter().any(|a| a.id == app.id))
            })
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

    fn free_profile_name(&self) -> String {
        (self.config.profiles.len() + 1..)
            .map(|n| trf!("profiles.numbered", number = n))
            .find(|name| !self.config.profiles.iter().any(|p| &p.name == name))
            .expect("some number is free")
    }
}

/// Every list of actions of a key, on press, on its gestures and in its second state.
fn actions_mut(key: &mut Key) -> impl Iterator<Item = &mut Action> {
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

fn drop_switch_to(key: &mut Key, profile: Uuid) {
    let leads_there =
        |action: &Action| matches!(action, Action::SwitchProfile { profile: p } if *p == profile);
    key.actions.retain(|a| !leads_there(a));
    key.double_press.retain(|a| !leads_there(a));
    key.long_press.retain(|a| !leads_there(a));
    if let Some(state) = &mut key.second_state {
        state.actions.retain(|a| !leads_there(a));
    }
}

fn profile_ids(profiles: &[Profile]) -> Vec<Uuid> {
    profiles.iter().map(|p| p.id).collect()
}

fn clean_name(name: &str) -> String {
    name.trim().chars().take(MAX_NAME).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use tempfile::TempDir;
    use tokio::sync::broadcast;

    use super::*;
    use crate::actions::{Context, SoundPlayer};
    use crate::config::{Appearance, Config, DeviceSettings, KeyState, Store};
    use crate::driver::d200::D200;
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
            name: name.into(),
            home_page: home.id,
            pages: vec![home],
            apps: Vec::new(),
        }
    }

    fn key(actions: Vec<Action>) -> Key {
        Key {
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
            schema: 2,
            revision: 0,
            device: DeviceSettings {
                brightness: 50,
                sleep_minutes: 0,
            },
            preferences: Default::default(),
            active_profile: profiles[0].id,
            profiles,
            plugin_settings: Default::default(),
            integrations: Vec::new(),
            local_profiles: Default::default(),
            plugins: Default::default(),
            declined_plugins: Default::default(),
        };
        let (dock, _) = D200::detached();
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
        let engine = Engine::new(store, config, Renderer::new(), dock, links, context);
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
        let id = engine.create_profile(None).unwrap();
        let created = find(engine, id);
        assert_eq!(created.name, "Perfil 2");
        assert!(created.apps.is_empty());
        let [home] = &created.pages[..] else {
            panic!("one page");
        };
        assert_eq!(home.name, "Inicio");
        assert_eq!(created.home_page, home.id);
        assert!(home.keys.is_empty());
        let named = engine.create_profile(Some("  Trabajo ".into())).unwrap();
        assert_eq!(find(engine, named).name, "Trabajo");
        let blank = engine.create_profile(Some("   ".into())).unwrap();
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

        let copy_id = engine.duplicate_profile(id).unwrap();
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
        assert_eq!(engine.config.active_profile, c);
        engine.delete_profile(c).unwrap();
        assert_eq!(engine.config.active_profile, a);
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
        assert_eq!(engine.config.active_profile, c);
        engine.delete_profile(c).unwrap();
        assert_eq!(engine.config.active_profile, b);
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
        assert_eq!(engine.config.active_profile, b);
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
        assert_eq!(engine.config.active_profile, b);
        assert!(engine.automatic_profile());
        assert_eq!(engine.store.load().unwrap().unwrap().active_profile, b);

        engine.on_front_app(app("com.apple.Safari"));
        assert_eq!(engine.config.active_profile, a);
        assert!(!engine.automatic_profile());
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
        assert!(!engine.automatic_profile());
        engine.on_front_app(app("com.apple.Safari"));
        assert_eq!(engine.config.active_profile, b);
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
        assert_eq!(engine.config.active_profile, a);
        assert!(!engine.automatic_profile());
        assert_eq!(engine.front_app, Some(app("com.figma.Desktop")));
    }
}
