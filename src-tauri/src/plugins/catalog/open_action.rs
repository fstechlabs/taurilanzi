//! The plugins of the OpenAction Marketplace, OpenDeck's store: plugins of its protocol and open
//! Stream Deck ones, each with its repository on GitHub, read from the catalog it publishes.
//! The package is the one the catalog names, or else the one in the latest release of the
//! repository. Those that drive a device of their own are left out.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use ureq::Agent;

use super::super::registry::{Registry, valid_plugin_id};
use super::entry::{
    CATALOG_LIMIT, CatalogError, CatalogPlugin, CatalogSource, FRESH_FOR, HERE, Images, Security,
    System, agent, host_of, installed, newer, save_package,
};
use crate::render::ImageData;

pub const CATALOG_URL: &str = "https://openactionapi.github.io/plugins/catalogue.json";
/// Points the catalog elsewhere, so tests never reach the real one.
pub const CATALOG_ENV: &str = "TAURILANZI_OPENACTION_CATALOG";
pub const GITHUB: &str = "https://github.com";
pub const GITHUB_API: &str = "https://api.github.com";
/// Points GitHub, its pages and its API, at one server elsewhere, for the same reason.
pub const GITHUB_ENV: &str = "TAURILANZI_GITHUB";
/// The catalog keys plugins by their id, which other stores may list too.
pub const ID_PREFIX: &str = "openaction:";
/// The catalog has four sections, each sorted by repository, and says nothing else of them: a
/// repository that sorts before the one above starts the next. The third is of plugins for
/// devices.
const DEVICE_SECTION: usize = 2;
const PACKAGE_HOSTS: &[&str] = &["github.com"];
const PACKAGE_HOST_SUFFIXES: &[&str] = &[".githubusercontent.com"];
const RELEASES_LISTED: u32 = 10;

#[derive(Debug, Clone, Deserialize)]
struct Raw {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone)]
struct Listed {
    id: String,
    raw: Raw,
}

/// The entries of the catalog in its order, which tells its sections apart.
struct InOrder(Vec<Listed>);

impl<'de> Deserialize<'de> for InOrder {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries;
        impl<'de> Visitor<'de> for Entries {
            type Value = InOrder;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object of plugins by id")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<InOrder, A::Error> {
                let mut listed = Vec::new();
                while let Some((id, raw)) = map.next_entry::<String, Raw>()? {
                    listed.push(Listed { id, raw });
                }
                Ok(InOrder(listed))
            }
        }
        deserializer.deserialize_map(Entries)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Release {
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

pub struct OpenAction {
    url: String,
    api: String,
    web: String,
    agent: Agent,
    cached: Mutex<Option<(Instant, Arc<Vec<Listed>>)>>,
    /// The latest release of each repository, or none when it has none.
    releases: Mutex<HashMap<String, (Instant, Option<Release>)>>,
    /// The version of the latest release of each repository, which costs no call to the API.
    latest: Mutex<HashMap<String, (Instant, Option<String>)>>,
    images: Images,
}

impl OpenAction {
    pub fn new() -> Self {
        let env = |name: &str, default: &str| {
            std::env::var(name)
                .ok()
                .filter(|u| !u.trim().is_empty())
                .unwrap_or_else(|| default.into())
        };
        let github = std::env::var(GITHUB_ENV)
            .ok()
            .filter(|u| !u.trim().is_empty());
        Self::at(
            env(CATALOG_ENV, CATALOG_URL),
            github.clone().unwrap_or_else(|| GITHUB_API.into()),
            github.unwrap_or_else(|| GITHUB.into()),
        )
    }

    pub(super) fn at(url: String, api: String, web: String) -> Self {
        Self {
            url,
            api: api.trim_end_matches('/').to_owned(),
            web: web.trim_end_matches('/').to_owned(),
            latest: Mutex::new(HashMap::new()),
            agent: agent(),
            cached: Mutex::new(None),
            releases: Mutex::new(HashMap::new()),
            images: Images::default(),
        }
    }

    fn raw(&self, refresh: bool) -> Result<Arc<Vec<Listed>>, CatalogError> {
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
        let InOrder(listed) =
            serde_json::from_str(&text).map_err(|err| CatalogError::Catalog(err.to_string()))?;
        let plugins = Arc::new(
            without_devices(listed)
                .into_iter()
                .filter(|l| valid_plugin_id(&l.id))
                .collect::<Vec<_>>(),
        );
        *cached = Some((Instant::now(), Arc::clone(&plugins)));
        if refresh {
            self.releases
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
            self.latest
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
        }
        Ok(plugins)
    }

    /// Every plugin of the catalog but those for devices. It says nothing of systems or versions:
    /// the version is that of the latest release, looked up only for those installed, to tell
    /// whether they have an update.
    pub fn catalog(
        &self,
        refresh: bool,
        registry: &Registry,
    ) -> Result<Vec<CatalogPlugin>, CatalogError> {
        let raw = self.raw(refresh)?;
        let found: Vec<(&Listed, Option<String>)> = raw
            .iter()
            .map(|l| {
                let have = installed(registry, None, &l.id).map(|p| p.manifest.version.clone());
                (l, have)
            })
            .collect();
        let latest: Vec<Option<String>> = std::thread::scope(|scope| {
            let asked: Vec<_> = found
                .iter()
                .map(|(listed, have)| {
                    have.is_some()
                        .then(|| scope.spawn(|| self.latest_version(listed)))
                })
                .collect();
            asked
                .into_iter()
                .map(|a| a.and_then(|h| h.join().ok().flatten()))
                .collect()
        });
        Ok(found
            .into_iter()
            .zip(latest)
            .map(|((listed, have), version)| self.shape(listed, have, version))
            .collect())
    }

    pub fn download(&self, id: &str, folder: &Path) -> Result<PathBuf, CatalogError> {
        let raw = self.raw(false)?;
        let listed = raw
            .iter()
            .find(|l| entry_id(l) == id)
            .ok_or(CatalogError::Unknown)?;
        let named = listed
            .raw
            .download_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty());
        let Some(named) = named else {
            return self.save(&self.released_package(listed)?, folder);
        };
        // Authors move the file the catalog names: their latest release is tried then.
        self.save(named, folder).or_else(|err| match err {
            CatalogError::Download(_) => self
                .released_package(listed)
                .and_then(|url| self.save(&url, folder))
                .map_err(|_| err),
            other => Err(other),
        })
    }

    /// The API lists the files of the release, to pick the right one. When it refuses, as it
    /// does after a few dozen calls an hour, the file named after the plugin is the usual one.
    fn released_package(&self, listed: &Listed) -> Result<String, CatalogError> {
        match self.release(listed) {
            Ok(release) => {
                Ok(
                    package_of(&release.ok_or(CatalogError::NoPackage)?, &listed.id, HERE)
                        .ok_or(CatalogError::NoPackage)?
                        .browser_download_url
                        .clone(),
                )
            }
            Err(CatalogError::Download(_)) => {
                let repository = listed
                    .raw
                    .repository
                    .as_deref()
                    .and_then(github_repository)
                    .ok_or(CatalogError::NoPackage)?;
                Ok(format!(
                    "{}/{repository}/releases/latest/download/{}.streamDeckPlugin",
                    self.web, listed.id
                ))
            }
            Err(err) => Err(err),
        }
    }

    /// GitHub answers the page of the latest release by sending to that of its tag.
    fn latest_version(&self, listed: &Listed) -> Option<String> {
        let repository = listed
            .raw
            .repository
            .as_deref()
            .and_then(github_repository)?;
        if let Some((at, version)) = self
            .latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&repository)
            && at.elapsed() < FRESH_FOR
        {
            return version.clone();
        }
        let response = self
            .agent
            .get(format!("{}/{repository}/releases/latest", self.web))
            .config()
            .max_redirects(0)
            .build()
            .call()
            .ok()?;
        let version = response
            .headers()
            .get("location")
            .and_then(|l| l.to_str().ok())
            .and_then(|l| l.split_once("/releases/tag/"))
            .map(|(_, tag)| version_of(tag))
            .filter(|v| !v.is_empty());
        self.latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(repository, (Instant::now(), version.clone()));
        version
    }

    fn save(&self, url: &str, folder: &Path) -> Result<PathBuf, CatalogError> {
        if !self.package_host(url) {
            return Err(CatalogError::Host(host_of(url).unwrap_or_default()));
        }
        save_package(&self.agent, url, folder)
    }

    /// The icon of a plugin of the catalog, never anything else.
    pub fn image(&self, url: &str) -> Option<ImageData> {
        if let Some(found) = self.images.kept(url) {
            return Some(found);
        }
        let listed = self
            .raw(false)
            .ok()?
            .iter()
            .any(|l| self.icon(&l.id) == url);
        if !listed {
            return None;
        }
        self.images.fetch(&self.agent, url)
    }

    /// Kept next to the catalog, named after the plugin.
    fn icon(&self, id: &str) -> String {
        let base = self.url.rsplit_once('/').map_or("", |(base, _)| base);
        format!("{base}/icons/{id}.png")
    }

    /// The latest release of the plugin's repository that is not a draft, and not a preview
    /// unless all are.
    fn release(&self, listed: &Listed) -> Result<Option<Release>, CatalogError> {
        let repository = listed
            .raw
            .repository
            .as_deref()
            .and_then(github_repository)
            .ok_or(CatalogError::NoPackage)?;
        if let Some((at, release)) = self
            .releases
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&repository)
            && at.elapsed() < FRESH_FOR
        {
            return Ok(release.clone());
        }
        let text = self
            .agent
            .get(format!(
                "{}/repos/{repository}/releases?per_page={RELEASES_LISTED}",
                self.api
            ))
            .header("accept", "application/vnd.github+json")
            .call()
            .and_then(|mut r| {
                r.body_mut()
                    .with_config()
                    .limit(CATALOG_LIMIT)
                    .read_to_string()
            })
            .map_err(|err| CatalogError::Download(err.to_string()))?;
        let releases: Vec<Release> =
            serde_json::from_str(&text).map_err(|err| CatalogError::Download(err.to_string()))?;
        let published: Vec<Release> = releases.into_iter().filter(|r| !r.draft).collect();
        let release = published
            .iter()
            .find(|r| !r.prerelease)
            .or(published.first())
            .cloned();
        self.releases
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(repository, (Instant::now(), release.clone()));
        Ok(release)
    }

    fn package_host(&self, url: &str) -> bool {
        let Some(host) = host_of(url) else {
            return false;
        };
        if [&self.url, &self.api, &self.web]
            .into_iter()
            .any(|u| host_of(u).as_ref() == Some(&host))
        {
            return true;
        }
        url.starts_with("https://")
            && (PACKAGE_HOSTS.contains(&host.as_str())
                || PACKAGE_HOST_SUFFIXES.iter().any(|s| host.ends_with(s)))
    }

    fn shape(
        &self,
        listed: &Listed,
        installed: Option<String>,
        latest: Option<String>,
    ) -> CatalogPlugin {
        let text = |value: &Option<String>| value.clone().unwrap_or_default().trim().to_owned();
        let raw = &listed.raw;
        let version = latest.unwrap_or_default();
        let update = installed
            .as_deref()
            .is_some_and(|have| newer(&version, have));
        CatalogPlugin {
            id: entry_id(listed),
            source: CatalogSource::OpenAction,
            plugin: listed.id.clone(),
            name: Some(text(&raw.name))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| listed.id.clone()),
            author: text(&raw.author),
            version,
            description: text(&raw.description),
            long_description: String::new(),
            category: String::new(),
            kind: None,
            icon: Some(self.icon(&listed.id)),
            screenshots: Vec::new(),
            tags: Vec::new(),
            changelog: String::new(),
            published_at: None,
            downloads: 0,
            stars: 0,
            source_url: raw.repository.clone().filter(|r| r.starts_with("https://")),
            security: Security::default(),
            dials: false,
            installed,
            update,
        }
    }
}

fn entry_id(listed: &Listed) -> String {
    format!("{ID_PREFIX}{}", listed.id)
}

fn without_devices(listed: Vec<Listed>) -> Vec<Listed> {
    let repositories: Vec<String> = listed
        .iter()
        .map(|l| l.raw.repository.clone().unwrap_or_default().to_lowercase())
        .collect();
    let starts: Vec<usize> = (1..repositories.len())
        .filter(|&i| repositories[i] < repositories[i - 1])
        .collect();
    // Without its sections in view, installing refuses the plugins for devices.
    let (Some(&from), Some(&to)) = (starts.get(DEVICE_SECTION - 1), starts.get(DEVICE_SECTION))
    else {
        return listed;
    };
    listed
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !(from..to).contains(i))
        .map(|(_, l)| l)
        .collect()
}

/// `owner/name` of a repository on GitHub.
fn github_repository(url: &str) -> Option<String> {
    let rest = url
        .trim()
        .strip_prefix("https://github.com/")?
        .trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = rest.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    let fine = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    (fine(owner) && fine(name) && parts.next().is_none()).then(|| format!("{owner}/{name}"))
}

fn version_of(tag: &str) -> String {
    tag.trim().trim_start_matches(['v', 'V']).to_owned()
}

/// The package of a release, as authors name them: a few releases also carry debug builds,
/// sources or a package for each system.
fn package_of<'a>(release: &'a Release, plugin: &str, system: System) -> Option<&'a Asset> {
    let words_for = |s: System| -> &[&str] {
        match s {
            System::Mac => &["mac", "macos", "osx", "darwin", "apple"],
            System::Windows => &["win", "windows", "win32", "win64"],
            System::Linux => &["linux"],
        }
    };
    let others: Vec<&str> = [System::Mac, System::Windows, System::Linux]
        .into_iter()
        .filter(|&s| s != system)
        .flat_map(words_for)
        .copied()
        .collect();
    release
        .assets
        .iter()
        .filter(|a| {
            let name = a.name.to_ascii_lowercase();
            name.ends_with(".streamdeckplugin") || name.ends_with(".zip")
        })
        .min_by_key(|a| {
            let name = a.name.to_ascii_lowercase();
            let words: Vec<&str> = name
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|w| !w.is_empty())
                .collect();
            let has = |list: &[&str]| words.iter().any(|w| list.contains(w));
            let mut cost = 0;
            if has(&["debug", "dsym", "source", "src", "symbols"]) {
                cost += 100;
            }
            if has(words_for(system)) {
                cost -= 10;
            } else if has(&others) {
                cost += 10;
            }
            if !name.ends_with(".streamdeckplugin") {
                cost += 2;
            }
            if !name.starts_with(&plugin.to_ascii_lowercase()) {
                cost += 1;
            }
            cost
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{self, Write};

    use serde_json::{Value, json};

    use super::super::fake_store::{FakeStore, SVG, answer, serve};
    use super::*;

    impl FakeStore {
        /// The catalog, GitHub's pages and its API, all on the fake store.
        fn open_action(&self) -> OpenAction {
            OpenAction::at(
                format!("{}/plugins/catalogue.json", self.base),
                self.base.clone(),
                self.base.clone(),
            )
        }
    }

    /// The catalog as OpenAction writes it: one object keyed by id, its sections in order.
    fn catalogue(entries: &[(&str, Value)]) -> Vec<u8> {
        let mut text = String::from("{");
        for (i, (id, entry)) in entries.iter().enumerate() {
            if i > 0 {
                text.push(',');
            }
            text.push_str(&format!("{}:{entry}", json!(id)));
        }
        text.push('}');
        text.into_bytes()
    }

    fn entry(name: &str, repository: &str) -> Value {
        json!({ "name": name, "author": "Someone", "repository": repository, "description": format!("{name} plugin") })
    }

    fn package(id: &str, version: &str, manifest: Value) -> Vec<u8> {
        let mut manifest = manifest;
        let object = manifest.as_object_mut().unwrap();
        object.insert("UUID".into(), json!(id));
        object.insert("Version".into(), json!(version));
        object.insert("SDKVersion".into(), json!(2));
        object.entry("CodePath").or_insert(json!("app.js"));
        let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        zip.start_file(
            format!("{id}.sdPlugin/manifest.json"),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn release(tag: &str, assets: &[(&str, &str)]) -> Value {
        json!({
            "tag_name": tag,
            "assets": assets.iter().map(|(name, url)| json!({ "name": name, "browser_download_url": url })).collect::<Vec<_>>(),
        })
    }

    fn install(store: &OpenAction, id: &str) -> Result<String, String> {
        let dir = tempfile::tempdir().unwrap();
        let zip = store.download(id, dir.path()).map_err(|e| e.to_string())?;
        super::super::super::install::install(&zip, &dir.path().join("plugins"))
            .map_err(|e| e.to_string())
    }

    #[test]
    fn lists_every_section_but_the_one_for_devices_in_its_order() {
        let server = serve(|_| {
            HashMap::from([(
                "/plugins/catalogue.json".to_owned(),
                catalogue(&[
                    (
                        "com.oa.official",
                        entry("Official", "https://github.com/oa/b"),
                    ),
                    (
                        "com.oa.native",
                        entry("Native", "https://github.com/a/native"),
                    ),
                    ("bad id!", entry("Bad", "https://github.com/a/z")),
                    (
                        "com.oa.device",
                        entry("Device", "https://github.com/a/device"),
                    ),
                    (
                        "com.oa.device2",
                        entry("Device 2", "https://github.com/b/device"),
                    ),
                    (
                        "com.oa.elgato",
                        entry("Elgato", "https://github.com/a/elgato"),
                    ),
                ]),
            )])
        });
        let store = server.open_action();
        let found = store.catalog(false, &Registry::default()).unwrap();
        let ids: Vec<&str> = found.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "openaction:com.oa.official",
                "openaction:com.oa.native",
                "openaction:com.oa.elgato"
            ],
            "the third section is for devices; ids that are not valid are left out"
        );
        let native = &found[1];
        assert_eq!(native.plugin, "com.oa.native");
        assert_eq!(native.name, "Native");
        assert_eq!(native.description, "Native plugin");
        assert_eq!(native.source, CatalogSource::OpenAction);
        assert_eq!(native.version, "", "only looked up for those installed");
        assert_eq!(
            native.source_url.as_deref(),
            Some("https://github.com/a/native")
        );
        assert_eq!(
            native.icon.as_deref(),
            Some(format!("{}/plugins/icons/com.oa.native.png", server.base).as_str()),
            "next to the catalog, named after the plugin"
        );
    }

    #[test]
    fn without_its_sections_in_view_every_plugin_is_listed() {
        let listed = |repositories: &[&str]| {
            without_devices(
                repositories
                    .iter()
                    .enumerate()
                    .map(|(i, r)| Listed {
                        id: format!("a.b{i}"),
                        raw: serde_json::from_value(json!({ "repository": r })).unwrap(),
                    })
                    .collect(),
            )
            .len()
        };
        assert_eq!(listed(&["a", "b", "c"]), 3);
        assert_eq!(listed(&["b", "a", "c"]), 3, "only one section starts");
        assert_eq!(listed(&["b", "a", "c", "b", "c", "a"]), 4);
    }

    #[test]
    fn installs_from_the_catalogs_link_or_the_latest_release() {
        let server = serve(|base| {
            let mut files = HashMap::from([
                (
                    "/plugins/catalogue.json".to_owned(),
                    catalogue(&[
                        (
                            "com.oa.linked",
                            json!({ "name": "Linked", "repository": "https://github.com/a/linked", "download_url": format!("{base}/a/linked/raw/linked.streamDeckPlugin") }),
                        ),
                        (
                            "com.oa.moved",
                            json!({ "name": "Moved", "repository": "https://github.com/a/moved", "download_url": format!("{base}/a/moved/raw/gone.streamDeckPlugin") }),
                        ),
                        ("com.oa.released", entry("Released", "https://github.com/a/released")),
                        ("com.oa.none", entry("None", "https://github.com/a/none")),
                        ("com.oa.far", entry("Far", "https://gitlab.com/a/far")),
                    ]),
                ),
                (
                    "/a/linked/raw/linked.streamDeckPlugin".to_owned(),
                    package("com.oa.linked", "1.0.0", json!({})),
                ),
                (
                    "/repos/a/moved/releases".to_owned(),
                    json!([release("v2.0.0", &[("com.oa.moved.streamDeckPlugin", &format!("{base}/dl/moved.zip"))])]).to_string().into_bytes(),
                ),
                ("/dl/moved.zip".to_owned(), package("com.oa.moved", "2.0.0", json!({}))),
                (
                    "/repos/a/released/releases".to_owned(),
                    json!([
                        { "tag_name": "v3.0.0-draft", "draft": true, "assets": [] },
                        { "tag_name": "v3.0.0-beta", "prerelease": true, "assets": [{ "name": "com.oa.released.streamDeckPlugin", "browser_download_url": "https://example.com/beta.zip" }] },
                        release("v2.0.0", &[
                            ("com.oa.released-debug.streamDeckPlugin", "https://example.com/debug.zip"),
                            ("source.zip", "https://example.com/source.zip"),
                            ("com.oa.released.streamDeckPlugin", &format!("{base}/dl/released.zip")),
                        ]),
                    ])
                    .to_string()
                    .into_bytes(),
                ),
                ("/dl/released.zip".to_owned(), package("com.oa.released", "2.0.0", json!({}))),
                ("/repos/a/none/releases".to_owned(), b"[]".to_vec()),
            ]);
            files.insert(
                "/plugins/icons/com.oa.linked.png".into(),
                SVG.as_bytes().to_vec(),
            );
            files
        });
        let store = server.open_action();
        assert_eq!(
            install(&store, "openaction:com.oa.linked").unwrap(),
            "com.oa.linked"
        );
        assert_eq!(
            install(&store, "openaction:com.oa.moved").unwrap(),
            "com.oa.moved",
            "the latest release when the link is gone"
        );
        assert_eq!(
            install(&store, "openaction:com.oa.released").unwrap(),
            "com.oa.released",
            "the latest published release, without debug builds or sources"
        );
        assert_eq!(
            install(&store, "openaction:com.oa.none").unwrap_err(),
            CatalogError::NoPackage.to_string()
        );
        assert_eq!(
            install(&store, "openaction:com.oa.far").unwrap_err(),
            CatalogError::NoPackage.to_string(),
            "only repositories on GitHub have releases to look up"
        );
        assert_eq!(
            install(&store, "openaction:com.oa.missing").unwrap_err(),
            CatalogError::Unknown.to_string()
        );

        let icon = format!("{}/plugins/icons/com.oa.linked.png", server.base);
        assert!(store.image(&icon).is_some());
        assert!(
            store
                .image(&format!("{}/plugins/catalogue.json", server.base))
                .is_none(),
            "only the icons of the catalog"
        );
    }

    #[test]
    fn when_the_api_refuses_the_file_named_after_the_plugin_is_tried() {
        let server = serve(|_| {
            HashMap::from([
                (
                    "/plugins/catalogue.json".to_owned(),
                    catalogue(&[(
                        "com.oa.limited",
                        entry("Limited", "https://github.com/a/limited"),
                    )]),
                ),
                (
                    "/repos/a/limited/releases".to_owned(),
                    answer("403 Forbidden", &[]),
                ),
                (
                    "/a/limited/releases/latest/download/com.oa.limited.streamDeckPlugin"
                        .to_owned(),
                    package("com.oa.limited", "1.0.0", json!({})),
                ),
            ])
        });
        assert_eq!(
            install(&server.open_action(), "openaction:com.oa.limited").unwrap(),
            "com.oa.limited"
        );
    }

    #[test]
    fn installed_plugins_learn_their_latest_version_from_where_github_sends_them() {
        let server = serve(|base| {
            HashMap::from([
                (
                    "/plugins/catalogue.json".to_owned(),
                    catalogue(&[
                        ("com.oa.old", entry("Old", "https://github.com/a/old")),
                        ("com.oa.other", entry("Other", "https://github.com/a/other")),
                    ]),
                ),
                (
                    "/a/old/releases/latest".to_owned(),
                    answer(
                        "302 Found",
                        &[("location", &format!("{base}/a/old/releases/tag/v1.2.0"))],
                    ),
                ),
                (
                    "/repos/a/old/releases".to_owned(),
                    json!([release(
                        "v1.0.0",
                        &[("com.oa.old.streamDeckPlugin", &format!("{base}/dl/old.zip"))]
                    )])
                    .to_string()
                    .into_bytes(),
                ),
                (
                    "/dl/old.zip".to_owned(),
                    package("com.oa.old", "1.0.0", json!({})),
                ),
            ])
        });
        let store = server.open_action();
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        let zip = store.download("openaction:com.oa.old", dir.path()).unwrap();
        super::super::super::install::install(&zip, &plugins).unwrap();
        let registry = Registry::scan(&plugins, "es");

        let found = store.catalog(false, &registry).unwrap();
        let old = found.iter().find(|p| p.plugin == "com.oa.old").unwrap();
        assert_eq!(old.installed.as_deref(), Some("1.0.0"));
        assert_eq!(old.version, "1.2.0");
        assert!(old.update);
        let other = found.iter().find(|p| p.plugin == "com.oa.other").unwrap();
        assert_eq!(other.version, "", "not installed: not looked up");

        let before = server.hits();
        store.catalog(false, &registry).unwrap();
        assert_eq!(server.hits(), before, "kept while fresh");
    }

    #[test]
    fn picks_the_package_of_a_release_for_this_system() {
        let assets = |names: &[&str]| Release {
            draft: false,
            prerelease: false,
            assets: names
                .iter()
                .map(|n| Asset {
                    name: (*n).to_owned(),
                    browser_download_url: format!("https://github.com/x/{n}"),
                })
                .collect(),
        };
        let pick = |names: &[&str], system| {
            package_of(&assets(names), "com.a.b", system).map(|a| a.name.clone())
        };
        let systems = [
            "com.a.b-linux.streamDeckPlugin",
            "com.a.b.mac.streamDeckPlugin",
            "com.a.b-win64.streamDeckPlugin",
        ];
        assert_eq!(pick(&systems, System::Linux).unwrap(), systems[0]);
        assert_eq!(pick(&systems, System::Mac).unwrap(), systems[1]);
        assert_eq!(pick(&systems, System::Windows).unwrap(), systems[2]);
        assert_eq!(
            pick(
                &["sdmicmute.dSYM.zip", "com.a.b.streamDeckPlugin"],
                System::Mac
            )
            .unwrap(),
            "com.a.b.streamDeckPlugin"
        );
        assert_eq!(
            pick(&["plugin.zip", "plugin.streamDeckPlugin"], System::Mac).unwrap(),
            "plugin.streamDeckPlugin"
        );
        assert_eq!(pick(&["notes.txt"], System::Mac), None);
    }

    #[test]
    fn reads_repositories_on_github_only() {
        assert_eq!(
            github_repository("https://github.com/a-b/c.d/").as_deref(),
            Some("a-b/c.d")
        );
        assert_eq!(
            github_repository("https://github.com/a/b.git").as_deref(),
            Some("a/b")
        );
        assert_eq!(github_repository("https://github.com/a"), None);
        assert_eq!(github_repository("https://github.com/a/b/tree/main"), None);
        assert_eq!(github_repository("https://gitlab.com/a/b"), None);
        assert_eq!(github_repository("https://github.com/a/b?x=1"), None);
        assert_eq!(version_of("v1.2.3"), "1.2.3");
    }

    #[test]
    #[ignore = "reaches the real catalog and GitHub, and downloads plugins"]
    fn reads_the_real_catalog() {
        let store = OpenAction::at(CATALOG_URL.into(), GITHUB_API.into(), GITHUB.into());
        let plugins = store.catalog(false, &Registry::default()).unwrap();
        assert!(plugins.len() > 100, "{}", plugins.len());
        assert!(
            !plugins
                .iter()
                .any(|p| p.plugin == "com.glmagalhaes.ulanzi.d200"),
            "no plugins for devices"
        );
        let icon = plugins[0].icon.clone().unwrap();
        assert!(store.image(&icon).is_some(), "{icon}");
        for id in [
            "openaction:me.amankhanna.oacounter",
            "openaction:com.gabe565.stopwatch",
        ] {
            install(&store, id).unwrap();
        }
    }
}
