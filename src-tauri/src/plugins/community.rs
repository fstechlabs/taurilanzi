//! The plugins of the Ulanzi Community Store, read from the catalog it publishes, without its app.
//! Only what the catalog lists is fetched: its downloads and its images, nothing the UI asks for.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ureq::Agent;

use super::images;
use super::registry::{PACKAGE_SUFFIX, Registry};
use crate::render::ImageData;

pub const CATALOG_URL: &str = "https://narlei.github.io/ulanzicommunitystore/catalog.json";
/// Points the catalog elsewhere, so tests never reach the real one.
pub const CATALOG_ENV: &str = "TAURILANZI_COMMUNITY_CATALOG";

const FRESH_FOR: Duration = Duration::from_secs(10 * 60);
const CATALOG_LIMIT: u64 = 16 * 1024 * 1024;
const PACKAGE_LIMIT: u64 = 300 * 1024 * 1024;
const IMAGE_LIMIT: u64 = 16 * 1024 * 1024;
const IMAGES_KEPT: usize = 96;
/// Where the store itself accepts packages from; the catalog's own host is added to these.
const PACKAGE_HOSTS: &[&str] = &["github.com"];
const PACKAGE_HOST_SUFFIXES: &[&str] = &[".ulanzistudio.com"];

#[derive(Debug, thiserror::Error)]
pub enum CommunityError {
    #[error(
        "{}",
        trf!("plugins.community.catalog", reason = .0)
    )]
    Catalog(String),
    #[error(
        "{}",
        tr!("plugins.community.unknown")
    )]
    Unknown,
    #[error(
        "{}",
        trf!("plugins.community.host", host = .0)
    )]
    Host(String),
    #[error(
        "{}",
        trf!("plugins.community.download", reason = .0)
    )]
    Download(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Raw {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    long_description: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    screenshots: Option<Vec<String>>,
    #[serde(default)]
    device_types: Option<Vec<String>>,
    #[serde(default)]
    platforms: Option<Vec<String>>,
    #[serde(default)]
    i18n: Option<HashMap<String, Texts>>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    changelog: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    downloads: Option<u32>,
    #[serde(default)]
    stars: Option<u32>,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    security: Option<RawSecurity>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Texts {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawSecurity {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    critical: Option<u32>,
    #[serde(default)]
    high: Option<u32>,
    #[serde(default)]
    secrets: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RawCatalog {
    #[serde(default)]
    plugins: Vec<Raw>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CommunityPlugin {
    /// As the catalog names it: the folder of the package.
    pub id: String,
    /// The id of the plugin: the one installed, or a guess from the catalog's until then, as
    /// some packages are named apart from the plugin inside.
    pub plugin: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub long_description: String,
    pub category: String,
    pub icon: Option<String>,
    pub screenshots: Vec<String>,
    pub tags: Vec<String>,
    pub changelog: String,
    pub published_at: Option<String>,
    pub downloads: u32,
    pub stars: u32,
    pub source_url: Option<String>,
    pub security: Security,
    /// Also has dials, which the app does not support yet.
    pub dials: bool,
    /// The version installed in the app, if it is.
    pub installed: Option<String>,
    pub update: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct Security {
    /// What the store's scanner found: `clean`, `findings` or nothing known.
    pub status: String,
    pub critical: u32,
    pub high: u32,
    pub secrets: u32,
}

pub struct Community {
    url: String,
    agent: Agent,
    cached: Mutex<Option<(Instant, Arc<Vec<Raw>>)>>,
    images: Mutex<HashMap<String, ImageData>>,
}

impl Community {
    pub fn new() -> Self {
        Self::at(
            std::env::var(CATALOG_ENV)
                .ok()
                .filter(|u| !u.trim().is_empty())
                .unwrap_or_else(|| CATALOG_URL.into()),
        )
    }

    fn at(url: String) -> Self {
        let agent: Agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(120)))
            .timeout_connect(Some(Duration::from_secs(10)))
            .user_agent(crate::version::user_agent())
            .build()
            .into();
        Self {
            url,
            agent,
            cached: Mutex::new(None),
            images: Mutex::default(),
        }
    }

    fn raw(&self, refresh: bool) -> Result<Arc<Vec<Raw>>, CommunityError> {
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
            .map_err(|err| CommunityError::Catalog(err.to_string()))?;
        let catalog: RawCatalog =
            serde_json::from_str(&text).map_err(|err| CommunityError::Catalog(err.to_string()))?;
        let plugins = Arc::new(catalog.plugins);
        *cached = Some((Instant::now(), Arc::clone(&plugins)));
        Ok(plugins)
    }

    /// The plugins that can run here: on this system and on keys.
    pub fn catalog(
        &self,
        refresh: bool,
        registry: &Registry,
        language: &str,
    ) -> Result<Vec<CommunityPlugin>, CommunityError> {
        let raw = self.raw(refresh)?;
        let mut plugins: Vec<CommunityPlugin> = raw
            .iter()
            .filter(|p| runs_here(p))
            .map(|p| shape(p, registry, language))
            .collect();
        plugins.sort_by(|a, b| b.downloads.cmp(&a.downloads).then(a.name.cmp(&b.name)));
        Ok(plugins)
    }

    /// Downloads the package of a plugin of the catalog into `folder`, as a ZIP file.
    pub fn download(&self, id: &str, folder: &Path) -> Result<PathBuf, CommunityError> {
        let raw = self.raw(false)?;
        let plugin = raw
            .iter()
            .find(|p| p.id == id && runs_here(p))
            .ok_or(CommunityError::Unknown)?;
        let url = plugin
            .download_url
            .as_deref()
            .ok_or(CommunityError::Unknown)?;
        if !self.package_host(url) {
            return Err(CommunityError::Host(host_of(url).unwrap_or_default()));
        }
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|err| CommunityError::Download(err.to_string()))?;
        fs::create_dir_all(folder).map_err(|err| CommunityError::Download(err.to_string()))?;
        let file = folder.join(format!(".download-{}.zip", uuid::Uuid::new_v4().simple()));
        let written = (|| -> io::Result<()> {
            let mut reader = response
                .body_mut()
                .with_config()
                .limit(PACKAGE_LIMIT)
                .reader();
            let mut out = fs::File::create(&file)?;
            io::copy(&mut reader, &mut out)?;
            Ok(())
        })();
        if let Err(err) = written {
            let _ = fs::remove_file(&file);
            return Err(CommunityError::Download(err.to_string()));
        }
        Ok(file)
    }

    /// An image of the catalog: icons and screenshots, never anything else.
    pub fn image(&self, url: &str) -> Option<ImageData> {
        if let Some(found) = self
            .images
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(url)
        {
            return Some(found.clone());
        }
        let listed = self.raw(false).ok()?.iter().any(|p| {
            p.icon.as_deref() == Some(url) || p.screenshots.iter().flatten().any(|s| s == url)
        });
        if !listed || !url.starts_with("https://") && !self.same_host(url) {
            return None;
        }
        let bytes = self
            .agent
            .get(url)
            .call()
            .and_then(|mut r| r.body_mut().with_config().limit(IMAGE_LIMIT).read_to_vec())
            .ok()?;
        let image = images::from_bytes(bytes)?;
        let mut kept = self.images.lock().unwrap_or_else(PoisonError::into_inner);
        if kept.len() >= IMAGES_KEPT {
            kept.clear();
        }
        kept.insert(url.to_owned(), image.clone());
        Some(image)
    }

    fn same_host(&self, url: &str) -> bool {
        host_of(url).is_some() && host_of(url) == host_of(&self.url)
    }

    fn package_host(&self, url: &str) -> bool {
        if self.same_host(url) {
            return true;
        }
        let Some(host) = host_of(url).filter(|_| url.starts_with("https://")) else {
            return false;
        };
        PACKAGE_HOSTS.contains(&host.as_str())
            || PACKAGE_HOST_SUFFIXES.iter().any(|s| host.ends_with(s))
    }
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    Some(host.to_ascii_lowercase()).filter(|h| !h.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum System {
    Mac,
    Windows,
    Linux,
}

const HERE: System = if cfg!(target_os = "windows") {
    System::Windows
} else if cfg!(target_os = "linux") {
    System::Linux
} else {
    System::Mac
};

fn runs_here(plugin: &Raw) -> bool {
    runs_on(plugin, HERE)
}

/// The catalog lists macOS and Windows only. On Linux the plugins made for both are the ones
/// that need neither system's own tools.
fn runs_on(plugin: &Raw, system: System) -> bool {
    let platforms: Vec<String> = plugin
        .platforms
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|p| p.to_ascii_lowercase())
        .collect();
    let listed = |prefix: &str| platforms.iter().any(|p| p.starts_with(prefix));
    let devices = plugin.device_types.as_deref().unwrap_or_default();
    let platform = platforms.is_empty()
        || match system {
            System::Mac => listed("mac"),
            System::Windows => listed("win"),
            System::Linux => listed("linux") || (listed("mac") && listed("win")),
        };
    platform && (devices.is_empty() || devices.iter().any(|d| d.eq_ignore_ascii_case("deck")))
}

/// The catalog names packages by folder, which is usually the plugin id and the suffix.
fn plugin_id(id: &str) -> String {
    id.strip_suffix(PACKAGE_SUFFIX).unwrap_or(id).to_owned()
}

fn shape(raw: &Raw, registry: &Registry, language: &str) -> CommunityPlugin {
    let texts = localized(raw, language);
    let guess = plugin_id(&raw.id);
    let version = raw.version.clone().unwrap_or_default();
    let found = registry
        .plugins()
        .iter()
        .find(|p| p.folder == raw.id)
        .or_else(|| registry.plugins().iter().find(|p| p.manifest.uuid == guess));
    let plugin = found.map_or(guess, |p| p.manifest.uuid.clone());
    let installed = found.map(|p| p.manifest.version.clone());
    let update = installed
        .as_deref()
        .is_some_and(|have| newer(&version, have));
    let security = raw.security.clone().unwrap_or_default();
    let text = |value: &Option<String>| value.clone().unwrap_or_default();
    CommunityPlugin {
        id: raw.id.clone(),
        plugin,
        name: texts.name.unwrap_or_else(|| text(&raw.name)),
        author: text(&raw.author),
        version,
        description: texts.description.unwrap_or_else(|| text(&raw.description)),
        long_description: text(&raw.long_description),
        category: text(&raw.category),
        icon: raw.icon.clone().filter(|i| !i.is_empty()),
        screenshots: raw.screenshots.clone().unwrap_or_default(),
        tags: raw.tags.clone().unwrap_or_default(),
        changelog: text(&raw.changelog),
        published_at: raw.published_at.clone(),
        downloads: raw.downloads.unwrap_or_default(),
        stars: raw.stars.unwrap_or_default(),
        source_url: raw.source_url.clone(),
        security: Security {
            status: security.status.unwrap_or_default(),
            critical: security.critical.unwrap_or_default(),
            high: security.high.unwrap_or_default(),
            secrets: security.secrets.unwrap_or_default(),
        },
        dials: raw
            .device_types
            .iter()
            .flatten()
            .any(|d| d.eq_ignore_ascii_case("dial")),
        installed,
        update,
    }
}

/// `es-ES` → `es_ES`, then `es`, then whatever starts with `es`.
fn localized(raw: &Raw, language: &str) -> Texts {
    let Some(all) = raw.i18n.as_ref() else {
        return Texts::default();
    };
    let full = language.replace('-', "_");
    let short = full.split('_').next().unwrap_or_default().to_owned();
    let found = all
        .get(&full)
        .or_else(|| all.get(&short))
        .or_else(|| {
            let mut keys: Vec<&String> = all.keys().filter(|k| k.starts_with(&short)).collect();
            keys.sort();
            keys.first().and_then(|k| all.get(*k))
        })
        .cloned()
        .unwrap_or_default();
    let blank = |value: Option<String>| value.filter(|v| !v.trim().is_empty());
    Texts {
        name: blank(found.name),
        description: blank(found.description),
    }
}

/// Whether `offered` is a later version than `installed`, comparing the numbers in order.
pub fn newer(offered: &str, installed: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.trim()
            .trim_start_matches(['v', 'V'])
            .split(['.', '-', '+'])
            .map_while(|p| p.parse().ok())
            .collect()
    };
    let (a, b) = (parts(offered), parts(installed));
    if a.is_empty() {
        return false;
    }
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde_json::{Value, json};

    use super::*;

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"/>"#;

    struct Server {
        base: String,
        hits: Arc<AtomicUsize>,
    }

    impl Server {
        fn hits(&self) -> usize {
            self.hits.load(Ordering::SeqCst)
        }

        fn community(&self) -> Community {
            Community::at(format!("{}/catalog.json", self.base))
        }
    }

    /// Serves the files `files` makes from the server's address, and counts the requests.
    fn serve(files: impl FnOnce(&str) -> HashMap<String, Vec<u8>>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let files = files(&base);
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                counter.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let path = line.split(' ').nth(1).unwrap_or_default().to_owned();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) <= 2 {
                        break;
                    }
                }
                let (status, body) = match files.get(&path) {
                    Some(body) => ("200 OK", body.as_slice()),
                    None => ("404 Not Found", &[][..]),
                };
                let mut stream = &stream;
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(body);
            }
        });
        Server { base, hits }
    }

    fn catalog(plugins: Value) -> HashMap<String, Vec<u8>> {
        HashMap::from([(
            "/catalog.json".to_owned(),
            json!({ "plugins": plugins }).to_string().into_bytes(),
        )])
    }

    fn entry(id: &str, extra: Value) -> Value {
        let mut entry = json!({ "id": id, "name": id, "version": "1.0.0" });
        entry
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        entry
    }

    /// A package whose folder is not named after the plugin inside, as happens in the catalog.
    fn package_zip() -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        zip.start_file(
            "com.a.b.ulanziPlugin/manifest.json",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(br#"{ "UUID": "com.a.b.plugin", "Version": "1.0.0" }"#)
            .unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn compares_versions_by_their_numbers() {
        assert!(newer("1.10.0", "1.9.9"));
        assert!(newer("v2", "1.9"));
        assert!(newer("1.0.1", "1.0"));
        assert!(!newer("1.0", "1.0.0"));
        assert!(!newer("1.2.0-beta", "1.2.0"));
        assert!(!newer("", "1.0.0"));
        assert!(!newer("0.9", "1.0"));
    }

    #[test]
    fn downloads_only_from_the_stores_hosts() {
        assert_eq!(
            host_of("https://GitHub.com/a/b.zip").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            host_of("https://user@evil.com:8080/x").as_deref(),
            Some("evil.com")
        );
        assert_eq!(host_of("not a url"), None);
        let community = Community::at(CATALOG_URL.into());
        assert!(community.package_host("https://github.com/a/b/releases/latest/download/x.zip"));
        assert!(community.package_host("https://cdn.ulanzistudio.com/x.zip"));
        assert!(community.package_host("https://narlei.github.io/x.zip"));
        assert!(
            !community.package_host("http://github.com/x.zip"),
            "only https"
        );
        assert!(!community.package_host("https://github.com.evil.com/x.zip"));
        assert!(!community.package_host("https://evilulanzistudio.com/x.zip"));
    }

    #[test]
    fn keeps_what_runs_here_with_keys_in_the_users_language() {
        let server = serve(|_| {
            catalog(json!([
                entry(
                    "com.a.few.ulanziPlugin",
                    json!({ "downloads": 1, "platforms": ["macos", "windows"], "deviceTypes": ["deck"] })
                ),
                entry(
                    "com.a.many.ulanziPlugin",
                    json!({
                        "downloads": 9,
                        "description": "English",
                        "deviceTypes": ["deck", "dial"],
                        "i18n": { "en": { "name": "Many" }, "es_ES": { "name": "Muchos", "description": " " } },
                        "security": { "status": "findings", "high": 2 },
                    })
                ),
                entry(
                    "com.a.elsewhere.ulanziPlugin",
                    json!({ "platforms": ["android"] })
                ),
                entry(
                    "com.a.dial.ulanziPlugin",
                    json!({ "deviceTypes": ["dial"] })
                ),
            ]))
        });
        let community = server.community();
        let found = community
            .catalog(false, &Registry::default(), "es-ES")
            .unwrap();
        let names: Vec<(&str, &str)> = found
            .iter()
            .map(|p| (p.id.as_str(), p.name.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("com.a.many.ulanziPlugin", "Muchos"),
                ("com.a.few.ulanziPlugin", "com.a.few.ulanziPlugin"),
            ],
            "by downloads, without those for other systems or dials only"
        );
        let many = &found[0];
        assert_eq!(
            many.description, "English",
            "blank translations are skipped"
        );
        assert_eq!(many.plugin, "com.a.many");
        assert!(many.dials);
        assert_eq!(
            (many.security.status.as_str(), many.security.high),
            ("findings", 2)
        );
        assert_eq!(many.installed, None);

        community
            .catalog(false, &Registry::default(), "es")
            .unwrap();
        assert_eq!(server.hits(), 1, "read once while fresh");
        community.catalog(true, &Registry::default(), "es").unwrap();
        assert_eq!(server.hits(), 2);
    }

    #[test]
    fn installs_what_the_catalog_lists_and_tells_when_it_is_old() {
        let server = serve(|base| {
            let mut files = catalog(json!([
                entry(
                    "com.a.b.ulanziPlugin",
                    json!({
                        "version": "1.2.0",
                        "icon": format!("{base}/icon.svg"),
                        "downloadUrl": format!("{base}/package.zip"),
                    })
                ),
                entry(
                    "com.a.far.ulanziPlugin",
                    json!({ "downloadUrl": "https://example.com/far.zip" })
                ),
            ]));
            files.insert("/icon.svg".into(), SVG.as_bytes().to_vec());
            files.insert("/other.svg".into(), SVG.as_bytes().to_vec());
            files.insert("/package.zip".into(), package_zip());
            files
        });
        let community = server.community();
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");

        let zip = community
            .download("com.a.b.ulanziPlugin", dir.path())
            .unwrap();
        let id = super::super::install::install(&zip, &plugins).unwrap();
        assert_eq!(id, "com.a.b.plugin");

        let registry = Registry::scan(&plugins, "es");
        let found = community.catalog(false, &registry, "es").unwrap();
        let installed = found
            .iter()
            .find(|p| p.id == "com.a.b.ulanziPlugin")
            .unwrap();
        assert_eq!(
            installed.plugin, "com.a.b.plugin",
            "the id inside the package"
        );
        assert_eq!(installed.installed.as_deref(), Some("1.0.0"));
        assert!(installed.update);

        assert!(matches!(
            community.download("com.a.far.ulanziPlugin", dir.path()),
            Err(CommunityError::Host(host)) if host == "example.com"
        ));
        assert!(matches!(
            community.download("com.a.missing.ulanziPlugin", dir.path()),
            Err(CommunityError::Unknown)
        ));

        let icon = community
            .image(&format!("{}/icon.svg", server.base))
            .unwrap();
        assert_eq!(icon.format, crate::render::ImageFormat::Svg);
        let before = server.hits();
        community
            .image(&format!("{}/icon.svg", server.base))
            .unwrap();
        assert_eq!(server.hits(), before, "kept once fetched");
        assert!(
            community
                .image(&format!("{}/other.svg", server.base))
                .is_none(),
            "not in the catalog"
        );
        assert_eq!(server.hits(), before);
    }

    #[test]
    #[ignore = "reaches the real catalog and downloads a plugin"]
    fn reads_the_real_catalog() {
        let community = Community::at(CATALOG_URL.into());
        let plugins = community
            .catalog(false, &Registry::default(), "es-ES")
            .unwrap();
        assert!(plugins.len() > 10);
        let icon = plugins.iter().find_map(|p| p.icon.clone()).unwrap();
        assert!(community.image(&icon).is_some(), "{icon}");
        let dir = tempfile::tempdir().unwrap();
        let zip = community.download(&plugins[0].id, dir.path()).unwrap();
        super::super::install::install(&zip, &dir.path().join("plugins")).unwrap();
    }

    #[test]
    fn each_system_keeps_the_plugins_made_for_it() {
        let listed = |platforms: &[&str]| {
            serde_json::from_value::<Raw>(json!({ "id": "x", "platforms": platforms })).unwrap()
        };
        let mac = listed(&["mac"]);
        let windows = listed(&["Windows"]);
        let both = listed(&["mac", "windows"]);
        let any = listed(&[]);
        assert!(runs_on(&mac, System::Mac) && !runs_on(&mac, System::Windows));
        assert!(runs_on(&windows, System::Windows) && !runs_on(&windows, System::Mac));
        assert!(!runs_on(&mac, System::Linux) && !runs_on(&windows, System::Linux));
        assert!(runs_on(&both, System::Linux));
        assert!(runs_on(&listed(&["linux"]), System::Linux));
        assert!(runs_on(&any, System::Linux) && runs_on(&any, System::Windows));
    }
}
