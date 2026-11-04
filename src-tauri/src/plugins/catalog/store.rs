//! Every store the app installs plugins from, as one catalog. A store that cannot be read leaves
//! the others; a plugin listed in two is offered once, from the one with its latest version.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use log::warn;

use super::super::registry::Registry;
use super::community::Community;
use super::entry::{CatalogError, CatalogPlugin, CatalogSource, newer};
use super::marketplace::{ID_PREFIX, Marketplace};
use crate::render::ImageData;

pub struct Catalog {
    marketplace: Marketplace,
    community: Community,
}

impl Catalog {
    pub fn new() -> Self {
        Self {
            marketplace: Marketplace::new(),
            community: Community::new(),
        }
    }

    pub fn plugins(
        &self,
        refresh: bool,
        registry: &Registry,
        language: &str,
    ) -> Result<Vec<CatalogPlugin>, CatalogError> {
        let read = [
            self.marketplace.catalog(refresh, registry, language),
            self.community.catalog(refresh, registry, language),
        ];
        let mut failure = None;
        let mut found = Vec::new();
        for result in read {
            match result {
                Ok(plugins) => found.extend(plugins),
                Err(err) => {
                    warn!("a plugin store could not be read: {err}");
                    failure.get_or_insert(err);
                }
            }
        }
        if found.is_empty()
            && let Some(err) = failure
        {
            return Err(err);
        }
        let mut plugins = once_each(found);
        plugins.sort_by(|a, b| b.downloads.cmp(&a.downloads).then(a.name.cmp(&b.name)));
        Ok(plugins)
    }

    /// Downloads the package of a plugin of any store into `folder`, as a ZIP file.
    pub fn download(
        &self,
        id: &str,
        language: &str,
        folder: &Path,
    ) -> Result<PathBuf, CatalogError> {
        if id.starts_with(ID_PREFIX) {
            self.marketplace.download(id, language, folder)
        } else {
            self.community.download(id, folder)
        }
    }

    pub fn image(&self, url: &str, language: &str) -> Option<ImageData> {
        self.marketplace
            .image(url, language)
            .or_else(|| self.community.image(url))
    }
}

/// The entry of each plugin with the latest version; Ulanzi's own when both have it.
fn once_each(plugins: Vec<CatalogPlugin>) -> Vec<CatalogPlugin> {
    let mut chosen: Vec<CatalogPlugin> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for plugin in plugins {
        let key = plugin.plugin.to_ascii_lowercase();
        let Some(&index) = at.get(&key).filter(|_| !key.is_empty()) else {
            at.insert(key, chosen.len());
            chosen.push(plugin);
            continue;
        };
        let kept = &chosen[index];
        let better = newer(&plugin.version, &kept.version)
            || (!newer(&kept.version, &plugin.version)
                && plugin.source == CatalogSource::Ulanzi
                && kept.source != CatalogSource::Ulanzi);
        if better {
            chosen[index] = plugin;
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::super::entry::Security;
    use super::super::fake_store::serve;
    use super::*;

    fn offered(id: &str, source: CatalogSource, plugin: &str, version: &str) -> CatalogPlugin {
        CatalogPlugin {
            id: id.into(),
            source,
            plugin: plugin.into(),
            name: id.into(),
            author: String::new(),
            version: version.into(),
            description: String::new(),
            long_description: String::new(),
            category: String::new(),
            kind: None,
            icon: None,
            screenshots: Vec::new(),
            tags: Vec::new(),
            changelog: String::new(),
            published_at: None,
            downloads: 0,
            stars: 0,
            source_url: None,
            security: Security::default(),
            dials: false,
            installed: None,
            update: false,
        }
    }

    #[test]
    fn offers_each_plugin_once_from_the_store_with_its_latest_version() {
        let ids = |plugins: Vec<CatalogPlugin>| -> Vec<String> {
            once_each(plugins).into_iter().map(|p| p.id).collect()
        };
        assert_eq!(
            ids(vec![
                offered("a.ulanziPlugin", CatalogSource::Community, "com.a", "1.1"),
                offered("ulanzi:1", CatalogSource::Ulanzi, "COM.A", "1.0"),
            ]),
            ["a.ulanziPlugin"],
            "the newer one"
        );
        assert_eq!(
            ids(vec![
                offered("a.ulanziPlugin", CatalogSource::Community, "com.a", "1.0"),
                offered("ulanzi:1", CatalogSource::Ulanzi, "com.a", "1.0.0"),
            ]),
            ["ulanzi:1"],
            "Ulanzi's own for the same version"
        );
        assert_eq!(
            ids(vec![
                offered("x.ulanziPlugin", CatalogSource::Community, "", "1.0"),
                offered("ulanzi:2", CatalogSource::Ulanzi, "", "1.0"),
                offered("b.ulanziPlugin", CatalogSource::Community, "com.b", "1.0"),
            ]),
            ["x.ulanziPlugin", "ulanzi:2", "b.ulanziPlugin"],
            "entries without an id are never merged"
        );
    }

    #[test]
    fn a_store_that_cannot_be_read_leaves_the_others() {
        let server = serve(|_| {
            HashMap::from([(
                "/catalog.json".to_owned(),
                json!({ "plugins": [{ "id": "a.ulanziPlugin", "name": "A", "version": "1.0" }] })
                    .to_string()
                    .into_bytes(),
            )])
        });
        let catalog = Catalog {
            marketplace: Marketplace::at(format!("{}/missing", server.base)),
            community: Community::at(format!("{}/catalog.json", server.base)),
        };
        let found = catalog.plugins(false, &Registry::default(), "en").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "a.ulanziPlugin");

        let nowhere = Catalog {
            marketplace: Marketplace::at(format!("{}/missing", server.base)),
            community: Community::at(format!("{}/missing.json", server.base)),
        };
        assert!(matches!(
            nowhere.plugins(false, &Registry::default(), "en"),
            Err(CatalogError::Catalog(_))
        ));
    }
}
