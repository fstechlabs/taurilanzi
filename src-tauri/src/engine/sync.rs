//! The engine's side of the cloud: what it has to sync, when each part was edited, and applying
//! what comes from other computers.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use jiff::Timestamp;
use uuid::Uuid;

use super::commands::CommandError;
use super::history::History;
use super::runtime::Engine;
use crate::cloud::{SETTINGS_ID, SharedSettings, SyncChanges, SyncExport, is_synced_account};
use crate::config::{Config, Integration, Profile};
use crate::hass::token_account;
use crate::obs::password_account;

/// The synced part of the configuration as it was last saved, to tell what each save changed.
pub(super) struct Saved {
    profiles: HashMap<Uuid, Profile>,
    settings: SharedSettings,
}

impl Saved {
    pub(super) fn of(config: &Config) -> Self {
        Self {
            profiles: config.profiles.iter().map(|p| (p.id, p.clone())).collect(),
            settings: SharedSettings::of(config),
        }
    }
}

impl Engine {
    /// Notes when each profile and the settings changed, so the most recent edit wins when two
    /// computers changed the same thing.
    pub(super) fn note_edits(&mut self) {
        let now = Timestamp::now();
        let saved = Saved::of(&self.config);
        for (id, profile) in &saved.profiles {
            if self.saved.profiles.get(id) != Some(profile) {
                self.edited.insert(*id, now);
            }
        }
        for id in self.saved.profiles.keys() {
            if !saved.profiles.contains_key(id) {
                self.edited.insert(*id, now);
            }
        }
        if saved.settings != self.saved.settings {
            self.edited.insert(SETTINGS_ID, now);
        }
        self.saved = saved;
    }

    pub(super) fn sync_export(&self) -> SyncExport {
        let local = &self.config.local_profiles;
        SyncExport {
            profiles: self
                .config
                .profiles
                .iter()
                .filter(|p| !local.contains(&p.id))
                .cloned()
                .collect(),
            local: self
                .config
                .profiles
                .iter()
                .map(|p| p.id)
                .filter(|id| local.contains(id))
                .collect(),
            settings: SharedSettings::of(&self.config),
            edited: self.edited.clone(),
        }
    }

    /// Takes in what other computers changed. Undo forgets everything before it, since going back
    /// would undo their changes too.
    pub(super) fn apply_sync(&mut self, changes: SyncChanges) -> Result<SyncExport, CommandError> {
        let before = self.config.active_profile;
        let local = self.config.local_profiles.clone();
        for profile in changes.profiles {
            if local.contains(&profile.id) {
                continue;
            }
            match self.config.profiles.iter_mut().find(|p| p.id == profile.id) {
                Some(existing) => *existing = profile,
                None => self.config.profiles.push(profile),
            }
        }
        for id in &changes.deleted {
            let Some(at) = self.config.profiles.iter().position(|p| p.id == *id) else {
                continue;
            };
            if local.contains(id) {
                continue;
            }
            if self.config.profiles.len() == 1 {
                // The dock always has a profile: the last one stays, on this computer only.
                self.config.local_profiles.insert(*id);
                continue;
            }
            self.config.profiles.remove(at);
        }
        if let Some(settings) = changes.settings {
            reorder(
                &mut self.config.profiles,
                &settings.profile_order,
                &self.config.local_profiles,
            );
            self.config.plugin_settings = settings.plugin_settings;
            self.config.integrations = settings.integrations;
            self.set_device(settings.device)?;
            let preferences = crate::config::Preferences {
                language: self.config.preferences.language,
                ..settings.preferences
            };
            self.set_preferences(preferences)?;
        }
        if let Some(secrets) = changes.secrets {
            let changed = self.context.secrets.replace(&secrets, is_synced_account)?;
            self.restart_clients(&changed);
        }
        self.sync_integrations();
        if self.config.active_profile().is_none()
            && let Some(first) = self.config.profiles.first()
        {
            self.config.active_profile = first.id;
        }
        self.profiles_restored(before);
        self.history = History::default();
        self.save()?;
        self.edited.extend(changes.edited);
        self.forget_stale_states();
        self.show_current_page();
        Ok(self.sync_export())
    }

    /// Keeps a profile on this computer only, or syncs it again.
    pub(super) fn set_profile_local(&mut self, id: Uuid, local: bool) -> Result<(), CommandError> {
        if !self.config.profiles.iter().any(|p| p.id == id) {
            return Err(CommandError::UnknownProfile(id));
        }
        let changed = if local {
            self.config.local_profiles.insert(id)
        } else {
            self.config.local_profiles.remove(&id)
        };
        if !changed {
            return Ok(());
        }
        self.save()?;
        if !local {
            self.edited.insert(id, Timestamp::now());
        }
        Ok(())
    }

    /// Clients keep the password or token they read: those whose secret changed start again.
    fn restart_clients(&mut self, accounts: &[String]) {
        let stale: Vec<Uuid> = self
            .config
            .integrations
            .iter()
            .map(Integration::id)
            .filter(|id| {
                accounts
                    .iter()
                    .any(|a| *a == password_account(*id) || *a == token_account(*id))
            })
            .collect();
        self.context.obs.retain(|id| !stale.contains(&id));
        self.context.hass.retain(|id| !stale.contains(&id));
    }
}

/// Puts the synced profiles in the cloud's order; profiles kept here only stay where they are.
fn reorder(profiles: &mut [Profile], order: &[Uuid], local: &BTreeSet<Uuid>) {
    let place: BTreeMap<Uuid, usize> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut synced: Vec<Profile> = profiles
        .iter()
        .filter(|p| !local.contains(&p.id))
        .cloned()
        .collect();
    synced.sort_by_key(|p| place.get(&p.id).copied().unwrap_or(usize::MAX));
    let mut synced = synced.into_iter();
    for profile in profiles.iter_mut() {
        if !local.contains(&profile.id)
            && let Some(next) = synced.next()
        {
            *profile = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use tempfile::TempDir;
    use tokio::sync::broadcast;

    use super::*;
    use crate::actions::{Context, SoundPlayer};
    use crate::cloud::TOKEN_ACCOUNT;
    use crate::config::{DeviceSettings, Page, Position, Store};
    use crate::driver::d200::D200;
    use crate::engine::handle::Activity;
    use crate::engine::history::Target;
    use crate::engine::runtime::Links;
    use crate::i18n::LanguageChoice;
    use crate::render::Renderer;
    use crate::secrets::Secrets;

    struct Setup {
        engine: Engine,
        _dir: TempDir,
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

    fn names(engine: &Engine) -> Vec<&str> {
        engine
            .config
            .profiles
            .iter()
            .map(|p| p.name.as_str())
            .collect()
    }

    fn timestamp(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    #[test]
    fn notes_when_each_profile_and_the_settings_were_edited() {
        let (a, b) = (profile("A"), profile("B"));
        let (a_id, b_id) = (a.id, b.id);
        let mut setup = engine(vec![a, b]);
        let engine = &mut setup.engine;
        engine.config.profiles[0].name = "A2".into();
        engine.save().unwrap();
        assert!(engine.edited.contains_key(&a_id));
        assert!(!engine.edited.contains_key(&b_id));
        assert!(!engine.edited.contains_key(&SETTINGS_ID));

        engine.set_profile(b_id).unwrap();
        assert!(
            !engine.edited.contains_key(&b_id),
            "switching is not an edit"
        );
        assert!(!engine.edited.contains_key(&SETTINGS_ID));

        engine.config.device.brightness = 10;
        engine.save().unwrap();
        assert!(engine.edited.contains_key(&SETTINGS_ID));
    }

    #[test]
    fn a_deleted_profile_counts_as_edited() {
        let (a, b) = (profile("A"), profile("B"));
        let b_id = b.id;
        let mut setup = engine(vec![a, b]);
        let engine = &mut setup.engine;
        engine.delete_profile(b_id).unwrap();
        assert!(engine.edited.contains_key(&b_id));
    }

    #[test]
    fn takes_in_new_changed_and_deleted_profiles() {
        let (a, b) = (profile("A"), profile("B"));
        let (a_id, b_id) = (a.id, b.id);
        let mut setup = engine(vec![a.clone(), b]);
        let engine = &mut setup.engine;
        engine
            .history
            .record(engine.config.profiles.clone(), Target::Other);
        let mut changed = a;
        changed.name = "A desde otro equipo".into();
        let new = profile("Nuevo");
        let new_id = new.id;
        let at = timestamp("2026-01-02T03:04:05.000Z");

        let export = engine
            .apply_sync(SyncChanges {
                profiles: vec![changed, new],
                deleted: vec![b_id],
                edited: BTreeMap::from([(a_id, at), (new_id, at), (b_id, at)]),
                ..SyncChanges::default()
            })
            .unwrap();

        assert_eq!(names(engine), ["A desde otro equipo", "Nuevo"]);
        assert_eq!(export.profiles.len(), 2);
        assert_eq!(engine.edited[&a_id], at, "the other computer's edit time");
        assert_eq!(engine.edited[&new_id], at);
        assert!(
            !engine.history.can_undo(),
            "undo would undo the other computer"
        );
    }

    #[test]
    fn leaves_profiles_kept_here_only_alone() {
        let (a, b) = (profile("A"), profile("Local"));
        let b_id = b.id;
        let mut setup = engine(vec![a, b.clone()]);
        let engine = &mut setup.engine;
        engine.set_profile_local(b_id, true).unwrap();
        let mut changed = b;
        changed.name = "Cambiado".into();
        engine
            .apply_sync(SyncChanges {
                profiles: vec![changed],
                deleted: vec![b_id],
                ..SyncChanges::default()
            })
            .unwrap();
        assert_eq!(names(engine), ["A", "Local"]);
    }

    #[test]
    fn never_deletes_the_last_profile_and_keeps_it_here_only() {
        let a = profile("A");
        let a_id = a.id;
        let mut setup = engine(vec![a]);
        let engine = &mut setup.engine;
        let export = engine
            .apply_sync(SyncChanges {
                deleted: vec![a_id],
                ..SyncChanges::default()
            })
            .unwrap();
        assert_eq!(names(engine), ["A"]);
        assert!(export.local.contains(&a_id));
        assert!(export.profiles.is_empty());
    }

    #[test]
    fn a_deleted_profile_on_the_dock_gives_way_to_another() {
        let (a, b) = (profile("A"), profile("B"));
        let (a_id, b_id) = (a.id, b.id);
        let mut setup = engine(vec![a, b]);
        let engine = &mut setup.engine;
        assert_eq!(engine.config.active_profile, a_id);
        engine
            .apply_sync(SyncChanges {
                deleted: vec![a_id],
                ..SyncChanges::default()
            })
            .unwrap();
        assert_eq!(engine.config.active_profile, b_id);
    }

    #[test]
    fn takes_in_the_settings_but_keeps_its_language_and_where_local_profiles_are() {
        let (a, local, b) = (profile("A"), profile("Local"), profile("B"));
        let (a_id, local_id, b_id) = (a.id, local.id, b.id);
        let mut setup = engine(vec![a, local, b]);
        let engine = &mut setup.engine;
        engine.config.preferences.language = LanguageChoice::En;
        engine.set_profile_local(local_id, true).unwrap();
        let mut settings = SharedSettings::of(&engine.config);
        settings.profile_order = vec![b_id, a_id];
        settings.device.brightness = 12;
        settings.preferences.language = LanguageChoice::Es;
        let server = Integration::Obs {
            id: Uuid::new_v4(),
            name: "OBS".into(),
            host: "127.0.0.1".into(),
            port: 4455,
        };
        settings.integrations = vec![server.clone()];

        engine
            .apply_sync(SyncChanges {
                settings: Some(settings),
                ..SyncChanges::default()
            })
            .unwrap();

        assert_eq!(names(engine), ["B", "Local", "A"]);
        assert_eq!(engine.config.device.brightness, 12);
        assert_eq!(engine.config.preferences.language, LanguageChoice::En);
        assert_eq!(engine.config.integrations, vec![server]);
    }

    #[test]
    fn takes_in_the_secrets_but_keeps_this_computers_token() {
        let mut setup = engine(vec![profile("A")]);
        let engine = &mut setup.engine;
        let secrets = engine.context.secrets.clone();
        secrets.set(TOKEN_ACCOUNT, Some("token")).unwrap();
        secrets.set("obs:old", Some("x")).unwrap();
        engine
            .apply_sync(SyncChanges {
                secrets: Some(BTreeMap::from([("obs:new".to_string(), "y".to_string())])),
                ..SyncChanges::default()
            })
            .unwrap();
        let all = secrets.all().unwrap();
        assert_eq!(all.get(TOKEN_ACCOUNT).map(String::as_str), Some("token"));
        assert_eq!(all.get("obs:new").map(String::as_str), Some("y"));
        assert!(!all.contains_key("obs:old"));
    }

    #[test]
    fn keeps_profiles_here_only_and_syncs_them_again() {
        let (a, b) = (profile("A"), profile("B"));
        let b_id = b.id;
        let mut setup = engine(vec![a, b]);
        let engine = &mut setup.engine;
        engine.set_profile_local(b_id, true).unwrap();
        let export = engine.sync_export();
        assert_eq!(export.local, BTreeSet::from([b_id]));
        assert!(export.profiles.iter().all(|p| p.id != b_id));
        assert!(!export.settings.profile_order.contains(&b_id));

        let copy = engine.duplicate_profile(b_id).unwrap();
        assert!(
            engine.sync_export().local.contains(&copy),
            "a copy of a local profile is local too"
        );

        engine.edited.clear();
        engine.set_profile_local(b_id, false).unwrap();
        let export = engine.sync_export();
        assert!(export.profiles.iter().any(|p| p.id == b_id));
        assert!(
            export.edited.contains_key(&b_id),
            "synced again on purpose, so it goes up as new"
        );
        assert!(matches!(
            engine.set_profile_local(Uuid::new_v4(), true),
            Err(CommandError::UnknownProfile(_))
        ));
    }
}
