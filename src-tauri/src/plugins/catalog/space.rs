//! The plugins of Mirabox Space, the store of Mirabox's Stream Dock, where Mirabox and other
//! authors publish them: read from the list its website shows, which needs no account. Only
//! free plugins are offered, and only what the list names is fetched.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::Deserialize;
use serde_json::json;
use ureq::Agent;

use super::super::registry::{Registry, STREAM_DECK_SUFFIX, valid_plugin_id};
use super::entry::{
    CATALOG_LIMIT, CatalogError, CatalogPlugin, CatalogSource, FRESH_FOR, HERE, Images, PluginKind,
    Security, System, agent, host_of, installed, newer, save_package,
};
use crate::render::ImageData;

pub const SPACE_URL: &str = "https://space.key123.vip";
/// Points the store elsewhere, so tests never reach the real one.
pub const SPACE_ENV: &str = "TAURILANZI_MIRABOX_SPACE";
/// The store numbers its entries; the prefix keeps them apart from the other stores' ids.
pub const ID_PREFIX: &str = "mirabox:";
/// Where packages are when the list gives only their name.
const FILES: &str = "https://cdn1.key123.vip/marketplace/streamDock/plugin/files";
/// Where its packages, icons and screenshots are.
const HOST_SUFFIXES: [&str; 2] = [".key123.vip", ".mirabox.net"];
const IMAGE_BUCKET: &str = "mirabox.oss-cn-shenzhen.aliyuncs.com";
/// Mirabox's own store, of the several the same service runs.
const TENANT: &str = "10000000";
/// Stream Dock plugins, apart from icon packs, scenes and what is for Mirabox's keyboards.
const PLUGINS: &str = "1";
/// The whole list in one page: it holds a few hundred.
const PAGE: u32 = 1000;
/// Names in the list may have spaces and other letters a URL cannot carry as they are.
const URL_PATH: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'|')
    .add(b'\\')
    .add(b'^');

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Raw {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    head_url: Option<String>,
    /// Pictures separated by `;`.
    #[serde(default)]
    gallery: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    /// What changed in this version.
    #[serde(default)]
    news: Option<String>,
    #[serde(default)]
    version: Option<String>,
    /// The ids of its categories, separated by commas.
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    types: Vec<RawType>,
    #[serde(default)]
    dial_support: Option<u8>,
    /// The package: its name in the store's folder, or its whole address.
    #[serde(default)]
    download: Option<String>,
    #[serde(default)]
    mac: Option<u8>,
    #[serde(default)]
    win: Option<u8>,
    #[serde(default)]
    downloads: Option<u32>,
    #[serde(default)]
    thumb_up: Option<u32>,
    #[serde(default)]
    update_time: Option<String>,
    #[serde(default)]
    price: Option<f64>,
    #[serde(default)]
    price_rmb: Option<f64>,
    /// `"1"` when only members get it for free.
    #[serde(default)]
    free_membership: Option<String>,
    /// The ids of its actions, separated by commas.
    #[serde(default)]
    uuids: Option<String>,
    #[serde(default)]
    additional_links: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawType {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawPage {
    #[serde(default)]
    list: Vec<Raw>,
}

#[derive(Debug, Deserialize)]
struct RawAnswer {
    #[serde(default)]
    code: Option<u32>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<RawPage>,
}

struct Cached {
    at: Instant,
    language: &'static str,
    plugins: Arc<Vec<Raw>>,
}

pub struct Space {
    url: String,
    agent: Agent,
    cached: Mutex<Option<Cached>>,
    images: Images,
}

impl Space {
    pub fn new() -> Self {
        Self::at(
            std::env::var(SPACE_ENV)
                .ok()
                .filter(|u| !u.trim().is_empty())
                .unwrap_or_else(|| SPACE_URL.into()),
        )
    }

    pub(super) fn at(url: String) -> Self {
        Self {
            url: url.trim_end_matches('/').to_owned(),
            agent: agent(),
            cached: Mutex::new(None),
            images: Images::default(),
        }
    }

    fn raw(&self, refresh: bool, language: &str) -> Result<Arc<Vec<Raw>>, CatalogError> {
        let language = list_language(language);
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = cached.as_ref()
            && !refresh
            && found.language == language
            && found.at.elapsed() < FRESH_FOR
        {
            return Ok(Arc::clone(&found.plugins));
        }
        let plugins = Arc::new(self.list(language)?);
        *cached = Some(Cached {
            at: Instant::now(),
            language,
            plugins: Arc::clone(&plugins),
        });
        Ok(plugins)
    }

    fn list(&self, language: &str) -> Result<Vec<Raw>, CatalogError> {
        let body = json!({
            "pageNum": 1,
            "pageSize": PAGE,
            "productType": PLUGINS,
            "tenantId": TENANT,
            "orderField": "downloads",
            "order": "desc",
        });
        let text = self
            .agent
            .post(format!("{}/interface/user/productInfo/list", self.url))
            .header("content-type", "application/json")
            .header("lang", language)
            .send(body.to_string())
            .and_then(|mut r| {
                r.body_mut()
                    .with_config()
                    .limit(CATALOG_LIMIT)
                    .read_to_string()
            })
            .map_err(|err| CatalogError::Catalog(err.to_string()))?;
        let answer: RawAnswer =
            serde_json::from_str(&text).map_err(|err| CatalogError::Catalog(err.to_string()))?;
        if answer.code.is_some_and(|c| c != 200) {
            return Err(CatalogError::Catalog(answer.message.unwrap_or_default()));
        }
        Ok(answer.data.map(|d| d.list).unwrap_or_default())
    }

    /// The free plugins for this system.
    pub fn catalog(
        &self,
        refresh: bool,
        registry: &Registry,
        language: &str,
    ) -> Result<Vec<CatalogPlugin>, CatalogError> {
        let raw = self.raw(refresh, language)?;
        Ok(raw
            .iter()
            .filter(|p| offered(p))
            .map(|p| shape(p, registry))
            .collect())
    }

    pub fn download(
        &self,
        id: &str,
        language: &str,
        folder: &Path,
    ) -> Result<PathBuf, CatalogError> {
        let raw = self.raw(false, language)?;
        let plugin = raw
            .iter()
            .find(|p| entry_id(p) == id && offered(p))
            .ok_or(CatalogError::Unknown)?;
        let url = package_url(plugin).ok_or(CatalogError::Unknown)?;
        if !self.own_host(&url) {
            return Err(CatalogError::Host(host_of(&url).unwrap_or_default()));
        }
        save_package(&self.agent, &encoded(&url), folder)
    }

    /// An icon or a screenshot of the list, never anything else.
    pub fn image(&self, url: &str, language: &str) -> Option<ImageData> {
        if let Some(found) = self.images.kept(url) {
            return Some(found);
        }
        let listed = self
            .raw(false, language)
            .ok()?
            .iter()
            .any(|p| p.head_url.as_deref() == Some(url) || gallery(p).iter().any(|g| g == url));
        if !listed || !self.own_host(url) {
            return None;
        }
        self.images.fetch_as(&self.agent, url, &encoded(url))
    }

    /// Mirabox's servers, through https, or the one the list comes from.
    fn own_host(&self, url: &str) -> bool {
        let Some(host) = host_of(url) else {
            return false;
        };
        host_of(&self.url).as_ref() == Some(&host)
            || (url.starts_with("https://")
                && (host == IMAGE_BUCKET || HOST_SUFFIXES.iter().any(|s| host.ends_with(s))))
    }
}

fn entry_id(raw: &Raw) -> String {
    format!("{ID_PREFIX}{}", raw.id)
}

/// The list comes in English or Chinese, and has some plugins in only one of them.
fn list_language(language: &str) -> &'static str {
    if language.to_ascii_lowercase().starts_with("zh") {
        "zh-CN"
    } else {
        "en-US"
    }
}

/// Free for everyone, with a package, and for this system. Linux gets those made for both
/// others, the ones that are not a program of either.
fn offered(raw: &Raw) -> bool {
    let free = raw.price.unwrap_or_default() <= 0.0
        && raw.price_rmb.unwrap_or_default() <= 0.0
        && raw.free_membership.as_deref() != Some("1");
    let (mac, win) = (raw.mac == Some(1), raw.win == Some(1));
    let here = match HERE {
        System::Mac => mac,
        System::Windows => win,
        System::Linux => mac && win,
    };
    free && here && package_url(raw).is_some()
}

fn package_url(raw: &Raw) -> Option<String> {
    let download = raw.download.as_deref()?.trim();
    if download.is_empty() {
        return None;
    }
    Some(if download.starts_with("http") {
        download.to_owned()
    } else {
        format!("{FILES}/{}", download.trim_start_matches('/'))
    })
}

fn encoded(url: &str) -> String {
    match url.split_once("://") {
        Some((scheme, rest)) => {
            let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
            format!("{scheme}://{host}{}", utf8_percent_encode(path, URL_PATH))
        }
        None => url.to_owned(),
    }
}

/// The id of the plugin: its package is named after it, as Stream Dock names the folder; or
/// what its actions start with.
fn plugin_id(raw: &Raw) -> String {
    let file = raw
        .download
        .as_deref()
        .unwrap_or_default()
        .rsplit('/')
        .next()
        .unwrap_or_default();
    let mut name = file.trim();
    let suffixes = [".zip", STREAM_DECK_SUFFIX];
    while let Some(shorter) = suffixes.iter().find_map(|s| {
        name.len()
            .checked_sub(s.len())
            .filter(|&at| name.is_char_boundary(at) && name[at..].eq_ignore_ascii_case(s))
            .map(|at| &name[..at])
    }) {
        name = shorter;
    }
    if valid_plugin_id(name) {
        return name.to_owned();
    }
    raw.uuids
        .as_deref()
        .and_then(|u| u.split(',').next())
        .and_then(|a| a.trim().rsplit_once('.'))
        .map(|(prefix, _)| prefix.to_owned())
        .unwrap_or_else(|| name.to_owned())
}

/// The store's categories, by their ids, as the catalog groups them.
fn kind_of(id: &str) -> Option<PluginKind> {
    match id.trim() {
        "10" => Some(PluginKind::Streaming),
        "7" | "14" | "2" => Some(PluginKind::Office),
        "4" => Some(PluginKind::Lighting),
        "11" | "13" | "5" => Some(PluginKind::Tools),
        "12" | "16" => Some(PluginKind::Creative),
        "6" | "15" => Some(PluginKind::Audio),
        "3" => Some(PluginKind::Gaming),
        "9" | "1" => Some(PluginKind::Social),
        "8" => Some(PluginKind::SmartHome),
        _ => None,
    }
}

fn gallery(raw: &Raw) -> Vec<String> {
    raw.gallery
        .as_deref()
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(str::to_owned)
        .collect()
}

fn shape(raw: &Raw, registry: &Registry) -> CatalogPlugin {
    let text = |value: &Option<String>| value.clone().unwrap_or_default().trim().to_owned();
    let plugin = plugin_id(raw);
    let version = text(&raw.version);
    let installed = installed(registry, None, &plugin).map(|p| p.manifest.version.clone());
    let update = installed
        .as_deref()
        .is_some_and(|have| newer(&version, have));
    let links = text(&raw.additional_links);
    CatalogPlugin {
        id: entry_id(raw),
        source: CatalogSource::Mirabox,
        plugin,
        name: text(&raw.name),
        author: text(&raw.author),
        version,
        description: text(&raw.overview),
        long_description: String::new(),
        category: raw
            .types
            .iter()
            .filter_map(|t| t.name.clone())
            .collect::<Vec<_>>()
            .join(", "),
        kind: raw
            .kind
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .find_map(kind_of),
        icon: raw.head_url.clone().filter(|i| !i.trim().is_empty()),
        screenshots: gallery(raw),
        tags: Vec::new(),
        changelog: text(&raw.news),
        published_at: raw.update_time.clone(),
        downloads: raw.downloads.unwrap_or_default(),
        stars: raw.thumb_up.unwrap_or_default(),
        source_url: Some(links)
            .filter(|l| l.starts_with("https://") && !l.contains(char::is_whitespace)),
        security: Security::default(),
        dials: raw.dial_support == Some(1),
        installed,
        update,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{self, Write};

    use serde_json::{Value, json};

    use super::super::fake_store::{FakeStore, SVG, serve};
    use super::*;

    impl FakeStore {
        fn space(&self) -> Space {
            Space::at(self.base.clone())
        }
    }

    const LIST: &str = "/interface/user/productInfo/list";

    fn here() -> Value {
        match HERE {
            System::Mac => json!({ "mac": 1, "win": 0 }),
            System::Windows => json!({ "mac": 0, "win": 1 }),
            System::Linux => json!({ "mac": 1, "win": 1 }),
        }
    }

    fn product(id: &str, base: &str, extra: Value) -> Value {
        let mut entry = json!({
            "id": id,
            "name": format!("Plugin {id}"),
            "author": "Someone",
            "version": "1.0.0",
            "download": format!("{base}/files/com.a.p{id}.sdPlugin"),
            "price": 0.0,
            "priceRmb": 0.0,
            "freeMembership": "0",
            "downloads": 3,
        });
        let object = entry.as_object_mut().unwrap();
        object.extend(here().as_object().unwrap().clone());
        object.extend(extra.as_object().unwrap().clone());
        entry
    }

    fn list(plugins: Value) -> Vec<u8> {
        json!({ "code": 200, "message": "ok", "data": { "list": plugins } })
            .to_string()
            .into_bytes()
    }

    /// As Stream Dock packages come: no id in the manifest, the folder names the plugin.
    fn package_zip() -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        zip.start_file(
            "com.a.p1.sdPlugin/manifest.json",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(br#"{ "Version": "1.0.0", "Actions": [{ "UUID": "com.a.p1.act" }] }"#)
            .unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn lists_the_free_plugins_for_this_system() {
        let server = serve(|base| {
            HashMap::from([(
                LIST.to_owned(),
                list(json!([
                    product(
                        "1",
                        base,
                        json!({
                            "type": "11,6",
                            "types": [{ "name": "Utilities" }, { "name": "Music" }],
                            "dialSupport": 1,
                            "news": "Fixed",
                            "thumbUp": 9,
                            "headUrl": format!("{base}/a b.png"),
                            "gallery": format!("{base}/s1.png;{base}/s2.png;"),
                        })
                    ),
                    product("2", base, json!({ "price": 6.0 })),
                    product("3", base, json!({ "freeMembership": "1" })),
                    product("4", base, json!({ "mac": 0, "win": 0 })),
                    product("5", base, json!({ "download": "" })),
                    product(
                        "6",
                        base,
                        json!({
                            "download": "/dev.tattn.switchbot.sdPlugin.SDPlugin",
                            "uuids": "x.y.z.act",
                        })
                    ),
                    product(
                        "7",
                        base,
                        json!({ "download": "/12345.zip", "uuids": "com.b.c.act,com.b.c.other" })
                    ),
                ])),
            )])
        });
        let space = server.space();
        let found = space.catalog(false, &Registry::default(), "es-ES").unwrap();
        let ids: Vec<(&str, &str)> = found
            .iter()
            .map(|p| (p.id.as_str(), p.plugin.as_str()))
            .collect();
        assert_eq!(
            ids,
            vec![
                ("mirabox:1", "com.a.p1"),
                ("mirabox:6", "dev.tattn.switchbot"),
                ("mirabox:7", "com.b.c"),
            ],
            "neither paid, for members, for other systems nor without a package"
        );
        let first = &found[0];
        assert_eq!(first.source, CatalogSource::Mirabox);
        assert_eq!(first.kind, Some(PluginKind::Tools));
        assert_eq!(first.category, "Utilities, Music");
        assert_eq!(first.changelog, "Fixed");
        assert_eq!(first.stars, 9);
        assert!(first.dials);
        assert_eq!(first.screenshots.len(), 2);

        let hits = server.hits();
        space.catalog(false, &Registry::default(), "en").unwrap();
        assert_eq!(server.hits(), hits, "read once while fresh");
    }

    #[test]
    fn installs_what_the_list_names_and_fetches_only_its_images() {
        let server = serve(|base| {
            HashMap::from([
                (
                    LIST.to_owned(),
                    list(json!([
                        product(
                            "1",
                            base,
                            json!({ "version": "1.1.0", "headUrl": format!("{base}/an icon.svg") })
                        ),
                        product(
                            "2",
                            base,
                            json!({ "download": "https://example.com/far.sdPlugin" })
                        ),
                    ])),
                ),
                ("/files/com.a.p1.sdPlugin".to_owned(), package_zip()),
                ("/an%20icon.svg".to_owned(), SVG.as_bytes().to_vec()),
                ("/other.svg".to_owned(), SVG.as_bytes().to_vec()),
            ])
        });
        let space = server.space();
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");

        let zip = space.download("mirabox:1", "en", dir.path()).unwrap();
        let id = super::super::super::install::install(&zip, &plugins).unwrap();
        assert_eq!(id, "com.a.p1", "named by its folder");

        let registry = Registry::scan(&plugins, "en");
        let found = space.catalog(false, &registry, "en").unwrap();
        let installed = found.iter().find(|p| p.id == "mirabox:1").unwrap();
        assert_eq!(installed.installed.as_deref(), Some("1.0.0"));
        assert!(installed.update);

        assert!(matches!(
            space.download("mirabox:2", "en", dir.path()),
            Err(CatalogError::Host(host)) if host == "example.com"
        ));
        assert!(matches!(
            space.download("mirabox:9", "en", dir.path()),
            Err(CatalogError::Unknown)
        ));

        let icon = format!("{}/an icon.svg", server.base);
        assert!(space.image(&icon, "en").is_some(), "with its space encoded");
        let before = server.hits();
        assert!(space.image(&icon, "en").is_some());
        assert!(
            space
                .image(&format!("{}/other.svg", server.base), "en")
                .is_none(),
            "not in the list"
        );
        assert_eq!(server.hits(), before);
    }

    #[test]
    fn trusts_mirabox_hosts_through_https_only() {
        let space = Space::at(SPACE_URL.into());
        assert!(space.own_host("https://cdn1.key123.vip/marketplace/x.sdPlugin"));
        assert!(space.own_host("https://space.mirabox.net/a.png"));
        assert!(space.own_host("https://mirabox.oss-cn-shenzhen.aliyuncs.com/a.png"));
        assert!(!space.own_host("http://cdn1.key123.vip/x.sdPlugin"));
        assert!(!space.own_host("https://other.oss-cn-shenzhen.aliyuncs.com/a.png"));
        assert!(!space.own_host("https://key123.vip.evil.com/x"));
        assert_eq!(
            encoded("https://cdn1.key123.vip/a/Space 用的.png"),
            "https://cdn1.key123.vip/a/Space%20%E7%94%A8%E7%9A%84.png"
        );
        assert_eq!(list_language("zh-CN"), "zh-CN");
        assert_eq!(list_language("es"), "en-US");
    }

    #[test]
    #[ignore = "reaches the real Mirabox Space and downloads a plugin"]
    fn reads_the_real_store() {
        let space = Space::at(SPACE_URL.into());
        let plugins = space.catalog(false, &Registry::default(), "es-ES").unwrap();
        assert!(plugins.len() > 50, "{}", plugins.len());
        assert!(plugins.iter().all(|p| p.id.starts_with(ID_PREFIX)));
        let icon = plugins.iter().find_map(|p| p.icon.clone()).unwrap();
        assert!(space.image(&icon, "es-ES").is_some(), "{icon}");
        let dir = tempfile::tempdir().unwrap();
        let zip = space.download(&plugins[0].id, "es-ES", dir.path()).unwrap();
        let id = super::super::super::install::install(&zip, &dir.path().join("plugins")).unwrap();
        assert_eq!(id, plugins[0].plugin);
    }
}
