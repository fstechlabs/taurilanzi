//! What sync with the cloud needs of the plugins: which are installed and where they came from,
//! and installing from the community catalog what the other computers have.

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
        let community = self.community(false).ok()?;
        Some(
            community
                .into_iter()
                .find(|entry| entry.installed.is_some() && entry.plugin == plugin)
                .map(|entry| entry.id),
        )
    }

    fn install(&self, catalog: &str) -> Result<Option<String>, String> {
        let community = self.community(false).map_err(|err| err.to_string())?;
        if !community.iter().any(|entry| entry.id == catalog) {
            return Ok(None);
        }
        self.install_community(catalog).map(Some)
    }
}
