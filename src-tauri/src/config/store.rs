use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use super::assets::AssetStore;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::integrations;
use super::model::{Config, SCHEMA_VERSION};
use super::navigation::Navigation;

const CONFIG_FILE: &str = "config.json";
const NAVIGATION_FILE: &str = "navigation.json";
const COUNTERS_FILE: &str = "counters.json";
const ASSETS_DIR: &str = "assets";

pub struct Store {
    root: PathBuf,
    pub assets: AssetStore,
}

impl Store {
    pub fn open(root: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&root)?;
        let assets = AssetStore::open(root.join(ASSETS_DIR))?;
        Ok(Self { root, assets })
    }

    /// Older schemas are read as the current one; the next save writes them in the new format.
    pub fn load(&self) -> io::Result<Option<Config>> {
        let Some(raw) = self.read::<serde_json::Value>(CONFIG_FILE)? else {
            return Ok(None);
        };
        let legacy_obs = serde_json::from_value(raw["preferences"]["obs"].clone()).ok();
        let mut config: Config = serde_json::from_value(raw).map_err(io::Error::other)?;
        config.schema = config.schema.max(SCHEMA_VERSION);
        integrations::migrate(&mut config, legacy_obs);
        Ok(Some(config))
    }

    /// Bumps the revision and writes atomically, so a crash never leaves a half-written file.
    pub fn save(&self, config: &mut Config) -> io::Result<()> {
        config.revision += 1;
        self.write(CONFIG_FILE, config)
    }

    /// Starts empty when the file is missing or unreadable: losing it only means starting at
    /// each profile's home page.
    pub fn load_navigation(&self) -> Navigation {
        self.read(NAVIGATION_FILE)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    pub fn save_navigation(&self, navigation: &Navigation) -> io::Result<()> {
        self.write(NAVIGATION_FILE, navigation)
    }

    /// Counters of keys, by `page:index`; state like the navigation, not configuration.
    pub fn load_counters(&self) -> BTreeMap<String, i64> {
        self.read(COUNTERS_FILE).ok().flatten().unwrap_or_default()
    }

    pub fn save_counters(&self, counters: &BTreeMap<String, i64>) -> io::Result<()> {
        self.write(COUNTERS_FILE, counters)
    }

    fn read<T: DeserializeOwned>(&self, name: &str) -> io::Result<Option<T>> {
        match fs::read(self.root.join(name)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(io::Error::other),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn write(&self, name: &str, value: &impl Serialize) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
        let tmp = self.root.join(format!(".{name}.tmp"));
        fs::write(&tmp, json)?;
        fs::rename(tmp, self.root.join(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::defaults;

    #[test]
    fn missing_config_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().to_path_buf()).unwrap();
        assert!(store.load().unwrap().is_none());
    }

    #[test]
    fn saved_config_loads_back_with_new_revision() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().to_path_buf()).unwrap();
        let mut config = defaults::first_run(&store.assets).unwrap();
        let before = config.revision;
        store.save(&mut config).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded, config);
        assert_eq!(loaded.revision, before + 1);
    }
}
