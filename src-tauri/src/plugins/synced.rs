//! What sync with the cloud needs of the plugins: which are installed and where they came from,
//! and installing from the stores what the other computers have.

use std::collections::BTreeMap;

use super::service::Plugins;
use crate::cloud::PluginInstaller;

impl PluginInstaller for Plugins {
    fn installed(&self) -> BTreeMap<String, String> {
        self.catalog()
            .plugins
            .into_iter()
            .map(|plugin| (plugin.id, plugin.name))
            .collect()
    }

    fn catalog_of(&self, plugin: &str) -> Option<Option<String>> {
        let store = self.store(false).ok()?;
        Some(
            store
                .into_iter()
                .find(|entry| entry.installed.is_some() && entry.plugin == plugin)
                .map(|entry| entry.id),
        )
    }

    fn install(&self, catalog: &str) -> Result<Option<String>, String> {
        let store = self.store(false).map_err(|err| err.to_string())?;
        if !store.iter().any(|entry| entry.id == catalog) {
            return Ok(None);
        }
        self.install_from_store(catalog).map(Some)
    }
}
