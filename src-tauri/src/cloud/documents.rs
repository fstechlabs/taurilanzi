//! What travels to the cloud: each synced profile on its own, and the settings every computer
//! shares, with the passwords and tokens of the servers.

use std::collections::{BTreeMap, BTreeSet};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::config::{
    Config, DeviceSettings, Integration, Preferences, Profile, SCHEMA_VERSION, ScreensaverImage,
    assets_in,
};
use crate::i18n::LanguageChoice;

/// The document of the settings, one per account.
pub const SETTINGS_ID: Uuid = Uuid::nil();
/// Where this computer keeps its token for the cloud, which never travels.
pub const TOKEN_ACCOUNT: &str = "cloud:token";

pub fn is_synced_account(account: &str) -> bool {
    account != TOKEN_ACCOUNT
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Profile,
    Settings,
}

/// The settings every computer of the account shares. The language stays each computer's own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedSettings {
    pub device: DeviceSettings,
    pub preferences: Preferences,
    /// The synced profiles, in the order of the sidebar.
    pub profile_order: Vec<Uuid>,
    #[serde(default)]
    pub plugin_settings: BTreeMap<String, Value>,
    #[serde(default)]
    pub integrations: Vec<Integration>,
}

impl SharedSettings {
    pub fn of(config: &Config) -> Self {
        Self {
            device: config.device.clone(),
            preferences: Preferences {
                language: LanguageChoice::default(),
                ..config.preferences.clone()
            },
            profile_order: config
                .profiles
                .iter()
                .map(|p| p.id)
                .filter(|id| !config.local_profiles.contains(id))
                .collect(),
            plugin_settings: config.plugin_settings.clone(),
            integrations: config.integrations.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileContent {
    pub schema: u32,
    pub profile: Profile,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SettingsContent {
    pub schema: u32,
    #[serde(flatten)]
    pub shared: SharedSettings,
    #[serde(default)]
    pub secrets: BTreeMap<String, String>,
}

/// What the engine has to sync: the synced profiles and settings, and when each was last edited.
#[derive(Debug, Clone)]
pub struct SyncExport {
    pub profiles: Vec<Profile>,
    pub local: BTreeSet<Uuid>,
    pub settings: SharedSettings,
    /// By document; missing when it has not changed since the app started.
    pub edited: BTreeMap<Uuid, Timestamp>,
}

/// What came from the cloud, for the engine to apply.
#[derive(Debug, Default)]
pub struct SyncChanges {
    /// New or changed profiles.
    pub profiles: Vec<Profile>,
    pub deleted: Vec<Uuid>,
    pub settings: Option<SharedSettings>,
    pub secrets: Option<BTreeMap<String, String>>,
    /// When each document was edited, so a change from the cloud does not count as this
    /// computer's.
    pub edited: BTreeMap<Uuid, Timestamp>,
}

impl SyncChanges {
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
            && self.deleted.is_empty()
            && self.settings.is_none()
            && self.secrets.is_none()
    }
}

/// A document as it is on this computer.
#[derive(Debug)]
pub struct LocalDoc {
    pub kind: Kind,
    pub content: Value,
    pub hash: String,
    pub edited_at: Option<Timestamp>,
    pub assets: Vec<String>,
}

pub fn local_docs(
    export: &SyncExport,
    secrets: BTreeMap<String, String>,
) -> serde_json::Result<BTreeMap<Uuid, LocalDoc>> {
    let mut docs = BTreeMap::new();
    for profile in &export.profiles {
        let content = serde_json::to_value(ProfileContent {
            schema: SCHEMA_VERSION,
            profile: profile.clone(),
        })?;
        let mut assets: Vec<String> = assets_in(std::slice::from_ref(profile))
            .into_iter()
            .map(str::to_owned)
            .collect();
        assets.sort();
        docs.insert(
            profile.id,
            LocalDoc {
                kind: Kind::Profile,
                hash: hash(&content)?,
                content,
                edited_at: export.edited.get(&profile.id).copied(),
                assets,
            },
        );
    }
    let mut assets = Vec::new();
    if let ScreensaverImage::Custom { asset, source, .. } =
        &export.settings.preferences.screensaver.image
    {
        assets.push(asset.clone());
        assets.extend(source.clone().filter(|s| s != asset));
    }
    let content = serde_json::to_value(SettingsContent {
        schema: SCHEMA_VERSION,
        shared: export.settings.clone(),
        secrets,
    })?;
    docs.insert(
        SETTINGS_ID,
        LocalDoc {
            kind: Kind::Settings,
            hash: hash(&content)?,
            content,
            edited_at: export.edited.get(&SETTINGS_ID).copied(),
            assets,
        },
    );
    Ok(docs)
}

fn hash(content: &Value) -> serde_json::Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(content)?)))
}

/// Joining an account that already has settings keeps the servers, plugin settings and secrets
/// this computer had and the cloud did not.
pub fn merge_settings(
    mut remote: SharedSettings,
    mut remote_secrets: BTreeMap<String, String>,
    local: &SharedSettings,
    local_secrets: &BTreeMap<String, String>,
) -> (SharedSettings, BTreeMap<String, String>) {
    for integration in &local.integrations {
        if !remote
            .integrations
            .iter()
            .any(|i| i.id() == integration.id())
        {
            remote.integrations.push(integration.clone());
        }
    }
    for (plugin, settings) in &local.plugin_settings {
        remote
            .plugin_settings
            .entry(plugin.clone())
            .or_insert_with(|| settings.clone());
    }
    for (account, value) in local_secrets {
        remote_secrets
            .entry(account.clone())
            .or_insert_with(|| value.clone());
    }
    (remote, remote_secrets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AssetStore, first_run};

    fn config() -> Config {
        let dir = tempfile::tempdir().unwrap();
        let assets = AssetStore::open(dir.path().to_path_buf()).unwrap();
        first_run(&assets).unwrap()
    }

    fn export(config: &Config) -> SyncExport {
        SyncExport {
            profiles: config
                .profiles
                .iter()
                .filter(|p| !config.local_profiles.contains(&p.id))
                .cloned()
                .collect(),
            local: config.local_profiles.clone(),
            settings: SharedSettings::of(config),
            edited: BTreeMap::new(),
        }
    }

    fn docs(config: &Config) -> BTreeMap<Uuid, LocalDoc> {
        local_docs(&export(config), BTreeMap::new()).unwrap()
    }

    #[test]
    fn each_profile_and_the_settings_are_a_document() {
        let config = config();
        let docs = docs(&config);
        assert_eq!(docs.len(), config.profiles.len() + 1);
        assert_eq!(docs[&SETTINGS_ID].kind, Kind::Settings);
        let profile = &config.profiles[0];
        assert_eq!(docs[&profile.id].kind, Kind::Profile);
        let content: ProfileContent =
            serde_json::from_value(docs[&profile.id].content.clone()).unwrap();
        assert_eq!(content.schema, SCHEMA_VERSION);
        assert_eq!(content.profile, *profile);
        assert_eq!(
            docs[&profile.id].assets,
            {
                let mut ids: Vec<String> = assets_in(std::slice::from_ref(profile))
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                ids.sort();
                ids
            },
            "the images go with the profile"
        );
    }

    #[test]
    fn the_same_profile_has_the_same_hash_and_a_change_another() {
        let mut config = config();
        let id = config.profiles[0].id;
        let before = docs(&config)[&id].hash.clone();
        assert_eq!(docs(&config)[&id].hash, before);
        config.profiles[0].name = "Otro".into();
        assert_ne!(docs(&config)[&id].hash, before);
    }

    #[test]
    fn the_language_stays_on_each_computer() {
        let mut config = config();
        let before = docs(&config)[&SETTINGS_ID].hash.clone();
        config.preferences.language = LanguageChoice::En;
        assert_eq!(docs(&config)[&SETTINGS_ID].hash, before);
        config.device.brightness = 7;
        assert_ne!(docs(&config)[&SETTINGS_ID].hash, before);
    }

    #[test]
    fn profiles_kept_here_only_do_not_travel() {
        let mut config = config();
        let mut copy = config.profiles[0].clone();
        copy.id = Uuid::new_v4();
        config.profiles.push(copy.clone());
        config.local_profiles.insert(copy.id);
        let docs = docs(&config);
        assert!(!docs.contains_key(&copy.id));
        let settings: SettingsContent =
            serde_json::from_value(docs[&SETTINGS_ID].content.clone()).unwrap();
        assert_eq!(settings.shared.profile_order, vec![config.profiles[0].id]);
    }

    #[test]
    fn secrets_travel_with_the_settings() {
        let config = config();
        let secrets = BTreeMap::from([("obs:x".to_string(), "hunter2".to_string())]);
        let docs = local_docs(&export(&config), secrets.clone()).unwrap();
        let settings: SettingsContent =
            serde_json::from_value(docs[&SETTINGS_ID].content.clone()).unwrap();
        assert_eq!(settings.secrets, secrets);
        assert!(!is_synced_account(TOKEN_ACCOUNT));
        assert!(is_synced_account("obs:x"));
    }

    #[test]
    fn joining_keeps_what_only_this_computer_had() {
        let config = config();
        let mut local = SharedSettings::of(&config);
        let mut remote = local.clone();
        remote.device.brightness = 11;
        local.device.brightness = 99;
        local
            .plugin_settings
            .insert("here".into(), serde_json::json!({"a": 1}));
        local
            .plugin_settings
            .insert("both".into(), serde_json::json!("local"));
        remote
            .plugin_settings
            .insert("both".into(), serde_json::json!("remote"));
        let server = Integration::Obs {
            id: Uuid::new_v4(),
            name: "OBS".into(),
            host: "127.0.0.1".into(),
            port: 4455,
        };
        local.integrations.push(server.clone());
        let local_secrets = BTreeMap::from([
            ("obs:here".to_string(), "mine".to_string()),
            ("obs:both".to_string(), "local".to_string()),
        ]);
        let remote_secrets = BTreeMap::from([("obs:both".to_string(), "remote".to_string())]);

        let (merged, secrets) = merge_settings(remote, remote_secrets, &local, &local_secrets);
        assert_eq!(merged.device.brightness, 11, "the cloud's settings win");
        assert_eq!(merged.plugin_settings["both"], serde_json::json!("remote"));
        assert_eq!(merged.plugin_settings["here"], serde_json::json!({"a": 1}));
        assert_eq!(merged.integrations, vec![server]);
        assert_eq!(secrets["obs:both"], "remote");
        assert_eq!(secrets["obs:here"], "mine");
    }
}
