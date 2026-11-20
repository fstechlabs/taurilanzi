//! Lots of edits applied all or none, for the assistant: each edit goes through the same rules
//! as the editor's, nothing is saved or drawn until all of them went through, and undo takes the
//! whole lot back at once.

use super::commands::{Change, CommandError};
use super::history::Target;
use super::pages::NewPage;
use super::profiles::NewProfile;
use super::runtime::Engine;

impl Engine {
    /// Whether anything changed: a lot that changes nothing leaves no step to undo.
    pub(super) fn apply(&mut self, changes: Vec<Change>) -> Result<bool, CommandError> {
        let config = self.config.clone();
        let navigation = self.navigation.clone();
        let app_profiles = self.app_profiles.clone();
        self.applying = true;
        let result = self.edit(Target::Other, |engine| {
            for (at, change) in changes.into_iter().enumerate() {
                engine
                    .apply_change(change)
                    .map_err(|error| CommandError::Change {
                        at,
                        error: Box::new(error),
                    })?;
            }
            Ok(())
        });
        self.applying = false;
        match result {
            Ok(()) => {
                let changed = self.config.profiles != config.profiles;
                let saved = self.save();
                self.save_navigation();
                self.show_current_page();
                saved.map(|()| changed)
            }
            Err(err) => {
                self.config = config;
                self.navigation = navigation;
                self.app_profiles = app_profiles;
                self.forget_stale_states();
                self.changed = true;
                Err(err)
            }
        }
    }

    fn apply_change(&mut self, change: Change) -> Result<(), CommandError> {
        match change {
            Change::SetKey { page, index, key } => self.set_key(page, index, key),
            Change::MoveKey { from, to } => self.move_key(from, to),
            Change::CreatePage {
                profile,
                id,
                name,
                position,
            } => {
                let new = NewPage {
                    profile: Some(profile),
                    id: Some(id),
                };
                self.create_page(new, name, position).map(|_| ())
            }
            Change::UpdatePage { page, changes } => self.update_page(page, changes),
            Change::DuplicatePage { page, id } => self.duplicate_page(page, Some(id)).map(|_| ()),
            Change::DeletePage { page } => self.delete_page(page),
            Change::SetHomePage { page } => self.set_home_page(page),
            Change::CreateProfile { id, home, name } => {
                let ids = NewProfile {
                    id: Some(id),
                    home: Some(home),
                };
                self.create_profile(ids, name).map(|_| ())
            }
            Change::RenameProfile { profile, name } => self.rename_profile(profile, name),
            Change::DuplicateProfile { profile, id } => {
                self.duplicate_profile(profile, Some(id)).map(|_| ())
            }
            Change::DeleteProfile { profile } => self.delete_profile(profile),
            Change::MoveProfile { profile, index } => self.move_profile(profile, index),
            Change::LinkApp { profile, app } => self.link_app(profile, app),
            Change::UnlinkApp { profile, app } => self.unlink_app(profile, app),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::mpsc;

    use tempfile::TempDir;
    use tokio::sync::broadcast;
    use uuid::Uuid;

    use super::*;
    use crate::actions::{Context, SoundPlayer};
    use crate::config::{
        Action, Appearance, Config, DeviceSettings, Key, Page, Position, Profile, Store,
    };
    use crate::driver::d200::D200;
    use crate::engine::commands::KeySlot;
    use crate::engine::handle::Activity;
    use crate::engine::runtime::Links;
    use crate::render::{Background, Color, Renderer};
    use crate::secrets::Secrets;

    struct Setup {
        engine: Engine,
        dir: TempDir,
    }

    fn profile(name: &str) -> Profile {
        let home = Page {
            id: Uuid::new_v4(),
            name: "Inicio".into(),
            keys: BTreeMap::new(),
            position: Position::default(),
        };
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
        Setup { engine, dir }
    }

    fn saved(setup: &Setup) -> Option<String> {
        std::fs::read_to_string(setup.dir.path().join("config.json")).ok()
    }

    #[test]
    fn a_lot_builds_a_profile_that_is_not_on_the_dock_and_undoes_at_once() {
        let work = profile("Trabajo");
        let active = work.id;
        let mut setup = engine(vec![work]);
        let (profile, page) = (Uuid::new_v4(), Uuid::new_v4());
        setup
            .engine
            .apply(vec![
                Change::CreateProfile {
                    id: profile,
                    home: Uuid::new_v4(),
                    name: Some("Streaming".into()),
                },
                Change::CreatePage {
                    profile,
                    id: page,
                    name: Some("Escenas".into()),
                    position: Position { x: 300.0, y: 0.0 },
                },
            ])
            .unwrap();
        let home = setup
            .engine
            .config
            .profiles
            .iter()
            .find(|p| p.id == profile)
            .unwrap()
            .home_page;
        setup
            .engine
            .apply(vec![
                Change::SetKey {
                    page: home,
                    index: 3,
                    key: Some(key(vec![Action::OpenPage {
                        page,
                        waypoints: Vec::new(),
                    }])),
                },
                Change::MoveKey {
                    from: KeySlot {
                        page: home,
                        index: 3,
                    },
                    to: KeySlot {
                        page: home,
                        index: 13,
                    },
                },
            ])
            .unwrap();

        let config = &setup.engine.config;
        assert_eq!(config.active_profile, active, "the dock stays where it was");
        let streaming = config.profiles.iter().find(|p| p.id == profile).unwrap();
        assert_eq!(streaming.pages.len(), 2);
        assert!(streaming.page(home).unwrap().keys.contains_key(&13));
        assert!(saved(&setup).unwrap().contains("Escenas"));

        setup.engine.undo().unwrap();
        let streaming = setup
            .engine
            .config
            .profiles
            .iter()
            .find(|p| p.id == profile)
            .unwrap();
        assert!(
            streaming.page(home).unwrap().keys.is_empty(),
            "the second lot is undone whole"
        );
        setup.engine.undo().unwrap();
        assert_eq!(setup.engine.config.profiles.len(), 1);
    }

    #[test]
    fn a_change_that_fails_leaves_everything_as_it_was() {
        let work = profile("Trabajo");
        let home = work.home_page;
        let mut setup = engine(vec![work]);
        let before = setup.engine.config.clone();
        let file = saved(&setup);
        let err = setup
            .engine
            .apply(vec![
                Change::SetKey {
                    page: home,
                    index: 1,
                    key: Some(key(vec![Action::Home])),
                },
                Change::CreatePage {
                    profile: before.profiles[0].id,
                    id: Uuid::new_v4(),
                    name: None,
                    position: Position::default(),
                },
                Change::SetKey {
                    page: home,
                    index: 2,
                    key: Some(key(vec![Action::Home, Action::Back])),
                },
            ])
            .unwrap_err();
        assert!(matches!(err, CommandError::Change { at: 2, .. }), "{err}");
        assert_eq!(setup.engine.config, before);
        assert_eq!(saved(&setup), file, "nothing was written");
        assert!(!setup.engine.history.can_undo());
    }

    #[test]
    fn new_ids_must_be_free() {
        let work = profile("Trabajo");
        let (id, home) = (work.id, work.home_page);
        let mut setup = engine(vec![work]);
        let err = setup
            .engine
            .apply(vec![Change::CreatePage {
                profile: id,
                id: home,
                name: None,
                position: Position::default(),
            }])
            .unwrap_err();
        assert!(err.to_string().contains("already used"), "{err}");
    }

    #[test]
    fn a_lot_that_changes_nothing_says_so_and_leaves_no_step() {
        let work = profile("Trabajo");
        let id = work.id;
        let mut setup = engine(vec![work]);
        assert!(!setup.engine.apply(Vec::new()).unwrap());
        let same = Change::RenameProfile {
            profile: id,
            name: "Trabajo".into(),
        };
        assert!(!setup.engine.apply(vec![same]).unwrap());
        assert!(!setup.engine.history.can_undo());
    }
}
