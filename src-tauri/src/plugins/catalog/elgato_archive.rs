//! The archive OpenDeck keeps of the Elgato App Store, the Stream Deck's store until its
//! Marketplace: the last version of each of its plugins, all free and from before the
//! Marketplace encrypted its packages. The archive serves the packages itself, as the store's
//! own server is gone.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use serde::Deserialize;
use ureq::Agent;

use super::super::registry::{Registry, valid_plugin_id};
use super::entry::{
    CATALOG_LIMIT, CatalogError, CatalogPlugin, CatalogSource, FRESH_FOR, Images, Security, agent,
    host_of, installed, newer, save_package,
};
use crate::render::ImageData;

pub const ARCHIVE_URL: &str = "https://plugins.amankhanna.me/catalogue.json";
/// Points the archive elsewhere, so tests never reach the real one.
pub const ARCHIVE_ENV: &str = "TAURILANZI_ELGATO_ARCHIVE";
/// The archive keys plugins by their id, which other stores may list too.
pub const ID_PREFIX: &str = "elgato:";

#[derive(Debug, Clone, Deserialize)]
struct Raw {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    version: Option<String>,
    /// Where the store had the package; the archive lists without it what it does not keep.
    #[serde(default)]
    download: Option<String>,
}

pub struct ElgatoArchive {
    url: String,
    agent: Agent,
    cached: Mutex<Option<(Instant, Arc<Vec<Raw>>)>>,
    images: Images,
}

impl ElgatoArchive {
    pub fn new() -> Self {
        Self::at(
            std::env::var(ARCHIVE_ENV)
                .ok()
                .filter(|u| !u.trim().is_empty())
                .unwrap_or_else(|| ARCHIVE_URL.into()),
        )
    }

    pub(super) fn at(url: String) -> Self {
        Self {
            url,
            agent: agent(),
            cached: Mutex::new(None),
            images: Images::default(),
        }
    }

    fn raw(&self, refresh: bool) -> Result<Arc<Vec<Raw>>, CatalogError> {
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((at, plugins)) = cached.as_ref()
            && !refresh
            && at.elapsed() < FRESH_FOR
        {
            return Ok(Arc::clone(plugins));
        }
        let text = self
            .agent
            .get(&self.url)
            .call()
            .and_then(|mut r| {
                r.body_mut()
                    .with_config()
                    .limit(CATALOG_LIMIT)
                    .read_to_string()
            })
            .map_err(|err| CatalogError::Catalog(err.to_string()))?;
        let plugins: Vec<Raw> =
            serde_json::from_str(&text).map_err(|err| CatalogError::Catalog(err.to_string()))?;
        let plugins = Arc::new(plugins.into_iter().filter(kept).collect::<Vec<_>>());
        *cached = Some((Instant::now(), Arc::clone(&plugins)));
        Ok(plugins)
    }

    /// Every plugin kept. The archive says nothing of systems: installing refuses those that
    /// are a program for another.
    pub fn catalog(
        &self,
        refresh: bool,
        registry: &Registry,
    ) -> Result<Vec<CatalogPlugin>, CatalogError> {
        let raw = self.raw(refresh)?;
        Ok(raw.iter().map(|p| self.shape(p, registry)).collect())
    }

    pub fn download(&self, id: &str, folder: &Path) -> Result<PathBuf, CatalogError> {
        let raw = self.raw(false)?;
        let plugin = raw
            .iter()
            .find(|p| entry_id(p) == id)
            .ok_or(CatalogError::Unknown)?;
        save_package(
            &self.agent,
            &self.beside(&format!("rezipped/{}.zip", plugin.id)),
            folder,
        )
    }

    /// The icon of a plugin of the archive, never anything else.
    pub fn image(&self, url: &str) -> Option<ImageData> {
        if let Some(found) = self.images.kept(url) {
            return Some(found);
        }
        let listed = self.raw(false).ok()?.iter().any(|p| self.icon(p) == url);
        if !listed {
            return None;
        }
        self.images.fetch(&self.agent, url)
    }

    /// The store's own icons are gone with it: the archive keeps them next to the list.
    fn icon(&self, raw: &Raw) -> String {
        self.beside(&format!("icons/{}.png", raw.id))
    }

    fn beside(&self, path: &str) -> String {
        let base = self.url.rsplit_once('/').map_or("", |(base, _)| base);
        format!("{base}/{path}")
    }

    fn shape(&self, raw: &Raw, registry: &Registry) -> CatalogPlugin {
        let text = |value: &Option<String>| value.clone().unwrap_or_default().trim().to_owned();
        let version = text(&raw.version);
        let installed = installed(registry, None, &raw.id).map(|p| p.manifest.version.clone());
        let update = installed
            .as_deref()
            .is_some_and(|have| newer(&version, have));
        let link = text(&raw.link);
        CatalogPlugin {
            id: entry_id(raw),
            source: CatalogSource::ElgatoArchive,
            plugin: raw.id.clone(),
            name: Some(text(&raw.name))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| raw.id.clone()),
            author: text(&raw.author),
            version,
            description: String::new(),
            long_description: String::new(),
            category: String::new(),
            kind: None,
            icon: Some(self.icon(raw)),
            screenshots: Vec::new(),
            tags: Vec::new(),
            changelog: String::new(),
            published_at: None,
            downloads: 0,
            stars: 0,
            source_url: (host_of(&link).as_deref() == Some("github.com")
                && link.starts_with("https://"))
            .then_some(link),
            security: Security::default(),
            dials: false,
            installed,
            update,
        }
    }
}

fn kept(raw: &Raw) -> bool {
    valid_plugin_id(&raw.id)
        && raw
            .download
            .as_deref()
            .is_some_and(|d| !d.trim().is_empty())
}

fn entry_id(raw: &Raw) -> String {
    format!("{ID_PREFIX}{}", raw.id)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{self, Write};

    use serde_json::json;

    use super::super::fake_store::{FakeStore, SVG, serve};
    use super::*;

    impl FakeStore {
        fn archive(&self) -> ElgatoArchive {
            ElgatoArchive::at(format!("{}/catalogue.json", self.base))
        }
    }

    /// Packages of the archive, rezipped with Windows' separators as many are.
    fn package(id: &str, version: &str) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        zip.start_file(
            format!("{id}.sdPlugin\\manifest.json"),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        let manifest = json!({ "Version": version, "SDKVersion": 2, "CodePath": "index.html" });
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn store() -> FakeStore {
        serve(|_| {
            let gone = |id: &str| {
                format!(
                    "https://appstore.elgato.com/streamDeckPlugin/{id}/1.0/{id}.streamDeckPlugin"
                )
            };
            HashMap::from([
                (
                    "/catalogue.json".to_owned(),
                    json!([
                        { "id": "com.a.clock", "name": "Clock", "author": "A", "link": "https://github.com/a/clock", "version": "1.1", "download": gone("com.a.clock") },
                        { "id": "com.a.site", "name": "Site", "author": "A", "link": "https://a.example.com", "version": "2.0", "download": gone("com.a.site") },
                        { "id": "com.a.kept-not", "name": "Not kept", "author": "A", "link": "https://a.example.com" },
                        { "id": "../bad", "name": "Bad", "download": gone("bad") },
                    ])
                    .to_string()
                    .into_bytes(),
                ),
                ("/rezipped/com.a.clock.zip".to_owned(), package("com.a.clock", "1.0")),
                ("/icons/com.a.clock.png".to_owned(), SVG.as_bytes().to_vec()),
            ])
        })
    }

    #[test]
    fn lists_what_it_keeps_with_its_own_icons() {
        let server = store();
        let found = server
            .archive()
            .catalog(false, &Registry::default())
            .unwrap();
        let ids: Vec<&str> = found.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            ["elgato:com.a.clock", "elgato:com.a.site"],
            "without what it does not keep or ids that are not valid"
        );
        let clock = &found[0];
        assert_eq!(clock.source, CatalogSource::ElgatoArchive);
        assert_eq!(
            (clock.name.as_str(), clock.version.as_str()),
            ("Clock", "1.1")
        );
        assert_eq!(
            clock.icon.as_deref(),
            Some(format!("{}/icons/com.a.clock.png", server.base).as_str())
        );
        assert_eq!(
            clock.source_url.as_deref(),
            Some("https://github.com/a/clock")
        );
        assert_eq!(found[1].source_url, None, "a website is not the source");
    }

    #[test]
    fn installs_the_package_it_keeps_and_tells_when_it_is_old() {
        let server = store();
        let archive = server.archive();
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        let zip = archive.download("elgato:com.a.clock", dir.path()).unwrap();
        let id = super::super::super::install::install(&zip, &plugins).unwrap();
        assert_eq!(id, "com.a.clock");

        let found = archive
            .catalog(false, &Registry::scan(&plugins, "es"))
            .unwrap();
        assert_eq!(found[0].installed.as_deref(), Some("1.0"));
        assert!(found[0].update);
        assert!(matches!(
            archive.download("elgato:com.a.kept-not", dir.path()),
            Err(CatalogError::Unknown)
        ));

        assert!(archive.image(&found[0].icon.clone().unwrap()).is_some());
        assert!(
            archive
                .image(&format!("{}/catalogue.json", server.base))
                .is_none()
        );
    }

    #[test]
    #[ignore = "reaches the real archive and downloads plugins"]
    fn reads_the_real_archive() {
        let archive = ElgatoArchive::at(ARCHIVE_URL.into());
        let plugins = archive.catalog(false, &Registry::default()).unwrap();
        assert!(plugins.len() > 300, "{}", plugins.len());
        let icon = plugins[0].icon.clone().unwrap();
        assert!(archive.image(&icon).is_some(), "{icon}");
        let dir = tempfile::tempdir().unwrap();
        let zip = archive
            .download("elgato:com.elgato.analogclock", dir.path())
            .unwrap();
        super::super::super::install::install(&zip, &dir.path().join("plugins")).unwrap();
    }
}
