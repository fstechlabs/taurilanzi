//! A plugin as the catalogs offer it, whichever store lists it, and what the stores share.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;
use ureq::Agent;

use super::super::images;
use super::super::registry::{Plugin, Registry, protocol_of};
use crate::render::ImageData;

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error(
        "{}",
        trf!("plugins.catalog.read", reason = .0)
    )]
    Catalog(String),
    #[error(
        "{}",
        tr!("plugins.catalog.unknown")
    )]
    Unknown,
    #[error(
        "{}",
        trf!("plugins.catalog.host", host = .0)
    )]
    Host(String),
    #[error(
        "{}",
        trf!("plugins.catalog.download", reason = .0)
    )]
    Download(String),
    #[error(
        "{}",
        tr!("plugins.catalog.noPackage")
    )]
    NoPackage,
}

/// The store a plugin comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSource {
    /// Ulanzi's own Marketplace, the one in Ulanzi Studio.
    Ulanzi,
    /// The Ulanzi Community Store.
    Community,
    /// Mirabox Space, the store of Mirabox's Stream Dock.
    Mirabox,
    /// The OpenAction Marketplace, OpenDeck's.
    OpenAction,
    /// The archive OpenDeck keeps of the Elgato App Store, the Stream Deck's before its
    /// Marketplace.
    ElgatoArchive,
}

/// What a plugin is for, in the categories every store's are brought to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Tools,
    Streaming,
    Office,
    Creative,
    Lighting,
    Audio,
    Gaming,
    Social,
    SmartHome,
    Ai,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CatalogPlugin {
    /// Unique in every store: the folder of the package in the community one.
    pub id: String,
    pub source: CatalogSource,
    /// The id of the plugin: the one installed, or the one the store gives until then, as some
    /// packages are named apart from the plugin inside.
    pub plugin: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub long_description: String,
    /// As the store names it.
    pub category: String,
    pub kind: Option<PluginKind>,
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

#[derive(Debug, Clone, Default, Serialize, specta::Type)]
pub struct Security {
    /// What the store's scanner found: `clean`, `findings` or nothing known.
    pub status: String,
    pub critical: u32,
    pub high: u32,
    pub secrets: u32,
}

pub const CATALOG_LIMIT: u64 = 16 * 1024 * 1024;
const PACKAGE_LIMIT: u64 = 300 * 1024 * 1024;
const IMAGE_LIMIT: u64 = 16 * 1024 * 1024;
const IMAGES_KEPT: usize = 96;
pub const FRESH_FOR: Duration = Duration::from_secs(10 * 60);

pub fn agent() -> Agent {
    Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .timeout_connect(Some(Duration::from_secs(10)))
        .user_agent(crate::version::user_agent())
        .build()
        .into()
}

pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    Some(host.to_ascii_lowercase()).filter(|h| !h.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Mac,
    Windows,
    Linux,
}

pub const HERE: System = if cfg!(target_os = "windows") {
    System::Windows
} else if cfg!(target_os = "linux") {
    System::Linux
} else {
    System::Mac
};

/// The stores list macOS and Windows only. On Linux the plugins made for both are the ones that
/// need neither system's own tools.
pub fn runs_on(platforms: &[String], system: System) -> bool {
    let platforms: Vec<String> = platforms.iter().map(|p| p.to_ascii_lowercase()).collect();
    let listed = |prefix: &str| platforms.iter().any(|p| p.starts_with(prefix));
    platforms.is_empty()
        || match system {
            System::Mac => listed("mac"),
            System::Windows => listed("win"),
            System::Linux => listed("linux") || (listed("mac") && listed("win")),
        }
}

/// The installed plugin a store's entry is: by the folder of its package, or by its id.
pub fn installed<'a>(
    registry: &'a Registry,
    folder: Option<&str>,
    plugin: &str,
) -> Option<&'a Plugin> {
    let folder = folder.filter(|f| protocol_of(f).is_some());
    registry
        .plugins()
        .iter()
        .find(|p| folder.is_some_and(|f| p.folder == f))
        .or_else(|| {
            registry
                .plugins()
                .iter()
                .find(|p| p.manifest.uuid == plugin)
        })
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

/// Downloads a package of a store into `folder`, as a ZIP file.
pub fn save_package(agent: &Agent, url: &str, folder: &Path) -> Result<PathBuf, CatalogError> {
    let mut response = agent
        .get(url)
        .call()
        .map_err(|err| CatalogError::Download(err.to_string()))?;
    fs::create_dir_all(folder).map_err(|err| CatalogError::Download(err.to_string()))?;
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
        return Err(CatalogError::Download(err.to_string()));
    }
    Ok(file)
}

/// The icons and screenshots a store's UI shows, kept once fetched.
#[derive(Default)]
pub struct Images {
    kept: Mutex<HashMap<String, ImageData>>,
}

impl Images {
    pub fn kept(&self, url: &str) -> Option<ImageData> {
        self.kept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(url)
            .cloned()
    }

    /// Only for what the store lists: the caller checks it first.
    pub fn fetch(&self, agent: &Agent, url: &str) -> Option<ImageData> {
        self.fetch_as(agent, url, url)
    }

    /// Kept as `url`, fetched from `from`.
    pub fn fetch_as(&self, agent: &Agent, url: &str, from: &str) -> Option<ImageData> {
        let bytes = agent
            .get(from)
            .call()
            .and_then(|mut r| r.body_mut().with_config().limit(IMAGE_LIMIT).read_to_vec())
            .ok()?;
        let image = images::from_bytes(bytes)?;
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        if kept.len() >= IMAGES_KEPT {
            kept.clear();
        }
        kept.insert(url.to_owned(), image.clone());
        Some(image)
    }
}
