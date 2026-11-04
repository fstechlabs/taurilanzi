//! The plugins of Ulanzi's own Marketplace, the one in Ulanzi Studio, read from the list it
//! publishes. Only what the list names is fetched: its packages and its images.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use serde::Deserialize;
use serde_json::Value;
use ureq::Agent;

use super::super::launcher::HOST_VERSION;
use super::super::registry::Registry;
use super::entry::{
    CATALOG_LIMIT, CatalogError, CatalogPlugin, CatalogSource, FRESH_FOR, HERE, Images, PluginKind,
    Security, System, agent, host_of, installed, newer, runs_on, save_package,
};
use crate::render::ImageData;

pub const MARKETPLACE_URL: &str = "https://www.ulanzistudio.com";
/// Points the Marketplace elsewhere, so tests never reach the real one.
pub const MARKETPLACE_ENV: &str = "TAURILANZI_ULANZI_MARKETPLACE";
/// The Marketplace numbers its entries; the prefix keeps them apart from the community's ids.
pub const ID_PREFIX: &str = "ulanzi:";
/// Where its packages, icons and banners are.
const HOST_SUFFIX: &str = ".ulanzistudio.com";
/// What the list calls plugins, apart from profiles and icon packs.
const PLUGINS: u8 = 1;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Raw {
    id: u64,
    #[serde(default)]
    os: Option<Vec<String>>,
    #[serde(default)]
    product_name: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    popular: Option<u32>,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    file_url: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    content_type: Option<String>,
    /// The group of `content_type`, which comes in the language of the list.
    #[serde(default)]
    classify: Option<u32>,
    #[serde(default)]
    device: Option<Vec<String>>,
    /// A picture or a list of them; the list leaves it empty for now.
    #[serde(default)]
    banner: Option<Value>,
    #[serde(default)]
    detail_desc: Option<String>,
    #[serde(default)]
    update_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawList {
    #[serde(default)]
    succ: Option<bool>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<Vec<Raw>>,
}

struct Cached {
    at: Instant,
    language: String,
    plugins: Arc<Vec<Raw>>,
}

pub struct Marketplace {
    url: String,
    agent: Agent,
    cached: Mutex<Option<Cached>>,
    images: Images,
}

impl Marketplace {
    pub fn new() -> Self {
        Self::at(
            std::env::var(MARKETPLACE_ENV)
                .ok()
                .filter(|u| !u.trim().is_empty())
                .unwrap_or_else(|| MARKETPLACE_URL.into()),
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

    /// The list names and describes its plugins in the language it is asked for.
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
        let mut plugins = self.list(&language)?;
        // The list in a language leaves out the plugins not translated to it.
        if language != "en"
            && let Ok(english) = self.list("en")
        {
            let missing: Vec<Raw> = english
                .into_iter()
                .filter(|p| !plugins.iter().any(|have| have.id == p.id))
                .collect();
            plugins.extend(missing);
        }
        let plugins = Arc::new(plugins);
        *cached = Some(Cached {
            at: Instant::now(),
            language,
            plugins: Arc::clone(&plugins),
        });
        Ok(plugins)
    }

    fn list(&self, language: &str) -> Result<Vec<Raw>, CatalogError> {
        // Windows' list for Linux too, where only what also runs on macOS is kept.
        let os = if HERE == System::Mac { 2 } else { 1 };
        let url = format!(
            "{}/product/list?os={os}&type={PLUGINS}&lang={language}&ver={HOST_VERSION}",
            self.url
        );
        let text = self
            .agent
            .get(&url)
            .call()
            .and_then(|mut r| {
                r.body_mut()
                    .with_config()
                    .limit(CATALOG_LIMIT)
                    .read_to_string()
            })
            .map_err(|err| CatalogError::Catalog(err.to_string()))?;
        let list: RawList =
            serde_json::from_str(&text).map_err(|err| CatalogError::Catalog(err.to_string()))?;
        if list.succ == Some(false) {
            return Err(CatalogError::Catalog(list.message.unwrap_or_default()));
        }
        Ok(list.data.unwrap_or_default())
    }

    /// The plugins that run here: on this system and on a D200.
    pub fn catalog(
        &self,
        refresh: bool,
        registry: &Registry,
        language: &str,
    ) -> Result<Vec<CatalogPlugin>, CatalogError> {
        let raw = self.raw(refresh, language)?;
        Ok(raw
            .iter()
            .filter(|p| runs_here(p))
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
            .find(|p| entry_id(p) == id && runs_here(p))
            .ok_or(CatalogError::Unknown)?;
        let url = plugin.file_url.as_deref().ok_or(CatalogError::Unknown)?;
        if !self.own_host(url) {
            return Err(CatalogError::Host(host_of(url).unwrap_or_default()));
        }
        save_package(&self.agent, url, folder)
    }

    /// An icon or a banner of the list, never anything else.
    pub fn image(&self, url: &str, language: &str) -> Option<ImageData> {
        if let Some(found) = self.images.kept(url) {
            return Some(found);
        }
        let listed = self
            .raw(false, language)
            .ok()?
            .iter()
            .any(|p| p.icon_url.as_deref() == Some(url) || banners(p).iter().any(|b| b == url));
        if !listed || !self.own_host(url) {
            return None;
        }
        self.images.fetch(&self.agent, url)
    }

    /// Ulanzi's own servers, through https, or the one the list comes from.
    fn own_host(&self, url: &str) -> bool {
        let Some(host) = host_of(url) else {
            return false;
        };
        host_of(&self.url).as_ref() == Some(&host)
            || (url.starts_with("https://") && host.ends_with(HOST_SUFFIX))
    }
}

fn entry_id(raw: &Raw) -> String {
    format!("{ID_PREFIX}{}", raw.id)
}

/// The list knows Ulanzi Studio's languages: `es`, `en`, `zh_CN`…
fn list_language(language: &str) -> String {
    let full = language.replace('-', "_");
    let short = full
        .split('_')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match short.as_str() {
        "" => "en".into(),
        "zh" => full,
        _ => short,
    }
}

fn runs_here(raw: &Raw) -> bool {
    runs_on(raw.os.as_deref().unwrap_or_default(), HERE)
        && raw.file_url.as_deref().is_some_and(|u| !u.is_empty())
        && raw.device.as_deref().is_none_or(|devices| {
            devices.is_empty()
                || devices
                    .iter()
                    .any(|d| d.to_ascii_uppercase().starts_with("D200"))
        })
}

fn kind_of(classify: u32) -> Option<PluginKind> {
    match classify {
        0 => Some(PluginKind::Streaming),
        1 => Some(PluginKind::Creative),
        2 => Some(PluginKind::Lighting),
        3 => Some(PluginKind::Office),
        4 => Some(PluginKind::Tools),
        13 => Some(PluginKind::Ai),
        _ => None,
    }
}

fn banners(raw: &Raw) -> Vec<String> {
    match &raw.banner {
        Some(Value::String(one)) if !one.is_empty() => vec![one.clone()],
        Some(Value::Array(many)) => many
            .iter()
            .filter_map(Value::as_str)
            .filter(|b| !b.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn shape(raw: &Raw, registry: &Registry) -> CatalogPlugin {
    let text = |value: &Option<String>| value.clone().unwrap_or_default().trim().to_owned();
    let plugin = text(&raw.uuid);
    let version = text(&raw.version);
    let found = installed(registry, None, &plugin);
    let installed = found.map(|p| p.manifest.version.clone());
    let update = installed
        .as_deref()
        .is_some_and(|have| newer(&version, have));
    // The details come as HTML for Ulanzi Studio's own view; plain text is shown as it is.
    let details = text(&raw.detail_desc);
    CatalogPlugin {
        id: entry_id(raw),
        source: CatalogSource::Ulanzi,
        plugin,
        name: text(&raw.product_name),
        author: text(&raw.author),
        version,
        description: text(&raw.overview),
        long_description: if details.contains('<') {
            String::new()
        } else {
            details
        },
        category: text(&raw.content_type),
        kind: raw.classify.and_then(kind_of),
        icon: raw.icon_url.clone().filter(|i| !i.is_empty()),
        screenshots: banners(raw),
        tags: Vec::new(),
        changelog: String::new(),
        published_at: raw.update_time.clone(),
        downloads: raw.popular.unwrap_or_default(),
        stars: 0,
        source_url: None,
        security: Security::default(),
        dials: false,
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
        fn marketplace(&self) -> Marketplace {
            Marketplace::at(format!("{}/", self.base))
        }
    }

    fn list_path(language: &str) -> String {
        let os = if HERE == System::Mac { 2 } else { 1 };
        format!("/product/list?os={os}&type=1&lang={language}&ver={HOST_VERSION}")
    }

    fn list(plugins: Value) -> Vec<u8> {
        json!({ "succ": true, "data": plugins })
            .to_string()
            .into_bytes()
    }

    fn here() -> &'static str {
        match HERE {
            System::Mac => "mac",
            System::Windows => "windows",
            System::Linux => "linux",
        }
    }

    fn product(id: u64, base: &str, extra: Value) -> Value {
        let mut entry = json!({
            "id": id,
            "os": [here(), "mac", "windows"],
            "productName": format!("Plugin {id}"),
            "uuid": format!("com.a.p{id}"),
            "version": "1.0.0",
            "fileUrl": format!("{base}/p{id}.zip"),
        });
        entry
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        entry
    }

    fn package_zip() -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        zip.start_file(
            "com.a.p1.ulanziPlugin/manifest.json",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(br#"{ "UUID": "com.a.p1", "Version": "1.0.0" }"#)
            .unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn lists_what_runs_on_this_system_and_a_d200_in_the_users_language() {
        let server = serve(|base| {
            HashMap::from([
                (
                    list_path("es"),
                    list(json!([
                        product(
                            1,
                            base,
                            json!({ "popular": 5, "classify": 4, "device": ["D200", "D200H"] })
                        ),
                        product(2, base, json!({ "device": ["D100H"] })),
                        product(3, base, json!({ "os": ["android"] })),
                        product(4, base, json!({ "fileUrl": "" })),
                        product(
                            5,
                            base,
                            json!({ "classify": 13, "detailDesc": "<p>HTML</p>" })
                        ),
                    ])),
                ),
                (
                    list_path("en"),
                    list(json!([
                        product(1, base, json!({ "productName": "English" })),
                        product(
                            6,
                            base,
                            json!({ "productName": "Only in English", "detailDesc": "Plain" })
                        ),
                    ])),
                ),
            ])
        });
        let marketplace = server.marketplace();
        let found = marketplace
            .catalog(false, &Registry::default(), "es-ES")
            .unwrap();
        let names: Vec<(&str, &str)> = found
            .iter()
            .map(|p| (p.id.as_str(), p.name.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("ulanzi:1", "Plugin 1"),
                ("ulanzi:5", "Plugin 5"),
                ("ulanzi:6", "Only in English"),
            ],
            "without other docks, systems or packages, and the untranslated ones in English"
        );
        assert_eq!(found[0].source, CatalogSource::Ulanzi);
        assert_eq!(found[0].plugin, "com.a.p1");
        assert_eq!(found[0].kind, Some(PluginKind::Tools));
        assert_eq!(found[0].downloads, 5);
        assert_eq!(found[1].kind, Some(PluginKind::Ai));
        assert_eq!(found[1].long_description, "", "HTML details are left out");
        assert_eq!(found[2].long_description, "Plain");

        let hits = server.hits();
        marketplace
            .catalog(false, &Registry::default(), "es")
            .unwrap();
        assert_eq!(server.hits(), hits, "read once while fresh");
        marketplace
            .catalog(false, &Registry::default(), "en")
            .unwrap();
        assert_eq!(server.hits(), hits + 1, "read again in another language");
    }

    #[test]
    fn installs_what_the_list_names_and_fetches_only_its_images() {
        let server = serve(|base| {
            HashMap::from([
                (
                    list_path("en"),
                    list(json!([
                        product(
                            1,
                            base,
                            json!({ "version": "1.2.0", "iconUrl": format!("{base}/icon.svg") })
                        ),
                        product(2, base, json!({ "fileUrl": "https://example.com/far.zip" })),
                    ])),
                ),
                ("/p1.zip".to_owned(), package_zip()),
                ("/icon.svg".to_owned(), SVG.as_bytes().to_vec()),
                ("/other.svg".to_owned(), SVG.as_bytes().to_vec()),
            ])
        });
        let marketplace = server.marketplace();
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");

        let zip = marketplace.download("ulanzi:1", "en", dir.path()).unwrap();
        let id = super::super::super::install::install(&zip, &plugins).unwrap();
        assert_eq!(id, "com.a.p1");

        let registry = Registry::scan(&plugins, "en");
        let found = marketplace.catalog(false, &registry, "en").unwrap();
        let installed = found.iter().find(|p| p.id == "ulanzi:1").unwrap();
        assert_eq!(installed.installed.as_deref(), Some("1.0.0"));
        assert!(installed.update);

        assert!(matches!(
            marketplace.download("ulanzi:2", "en", dir.path()),
            Err(CatalogError::Host(host)) if host == "example.com"
        ));
        assert!(matches!(
            marketplace.download("ulanzi:9", "en", dir.path()),
            Err(CatalogError::Unknown)
        ));

        let icon = format!("{}/icon.svg", server.base);
        assert!(marketplace.image(&icon, "en").is_some());
        let before = server.hits();
        assert!(marketplace.image(&icon, "en").is_some());
        assert!(
            marketplace
                .image(&format!("{}/other.svg", server.base), "en")
                .is_none(),
            "not in the list"
        );
        assert_eq!(server.hits(), before);
    }

    #[test]
    fn trusts_ulanzis_own_hosts_through_https_only() {
        let marketplace = Marketplace::at(MARKETPLACE_URL.into());
        assert!(marketplace.own_host("https://file.ulanzistudio.com/a.zip"));
        assert!(marketplace.own_host("https://www.ulanzistudio.com/a.png"));
        assert!(!marketplace.own_host("http://file.ulanzistudio.com/a.zip"));
        assert!(!marketplace.own_host("https://ulanzistudio.com.evil.com/a.zip"));
        assert!(!marketplace.own_host("https://evilulanzistudio.com/a.zip"));
    }

    #[test]
    fn asks_for_the_languages_ulanzi_studio_knows() {
        assert_eq!(list_language("es-ES"), "es");
        assert_eq!(list_language("zh-CN"), "zh_CN");
        assert_eq!(list_language("EN"), "en");
        assert_eq!(list_language(""), "en");
    }

    #[test]
    #[ignore = "reaches the real Marketplace and downloads a plugin"]
    fn reads_the_real_marketplace() {
        let marketplace = Marketplace::at(MARKETPLACE_URL.into());
        let plugins = marketplace
            .catalog(false, &Registry::default(), "es-ES")
            .unwrap();
        assert!(plugins.len() > 10);
        assert!(plugins.iter().all(|p| p.id.starts_with(ID_PREFIX)));
        let icon = plugins.iter().find_map(|p| p.icon.clone()).unwrap();
        assert!(marketplace.image(&icon, "es-ES").is_some(), "{icon}");
        let dir = tempfile::tempdir().unwrap();
        let zip = marketplace
            .download(&plugins[0].id, "es-ES", dir.path())
            .unwrap();
        super::super::super::install::install(&zip, &dir.path().join("plugins")).unwrap();
    }
}
