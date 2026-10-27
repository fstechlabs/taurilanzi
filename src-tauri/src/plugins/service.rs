//! The plugins as one piece: the packages found, the host they talk to and their processes.

use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};

use log::{info, warn};
use serde::Serialize;

use super::community::{Community, CommunityError, CommunityPlugin};
use super::desktop::Desktop;
use super::hub::{HubSetup, PluginHost, PluginInput};
use super::install::{self, InstallError};
use super::launcher::{Launcher, RunState};
use super::node;
use super::registry::Registry;
use super::server;

pub struct PluginsSetup {
    pub data_dir: PathBuf,
    /// The app's resources, where its Node is.
    pub resources: Option<PathBuf>,
    pub language: String,
    pub notify: Box<dyn Fn(PluginInput) + Send>,
    pub desktop: Arc<dyn Desktop>,
}

pub struct Plugins {
    host: PluginHost,
    launcher: Launcher,
    registry: Arc<RwLock<Registry>>,
    folder: PathBuf,
    community: Community,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PluginCatalog {
    /// Where the host listens; the pages of the plugins are served from it.
    pub port: u16,
    /// The Node that runs plugins, if one was found.
    pub node: Option<String>,
    pub language: String,
    pub plugins: Vec<PluginInfo>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub version: String,
    /// The group of its actions in the catalog.
    pub category: String,
    /// Relative to the plugin folder.
    pub icon: Option<String>,
    /// The package folder, ending in `.ulanziPlugin`.
    pub folder: String,
    pub run: RunState,
    /// Its main service is connected to the host.
    pub connected: bool,
    pub actions: Vec<PluginActionInfo>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PluginActionInfo {
    pub uuid: String,
    pub name: String,
    pub tooltip: String,
    pub icon: Option<String>,
    /// Its property inspector page, relative to the plugin folder.
    pub inspector: Option<String>,
    /// Can go on a key of the D200: dials and other models are not supported yet.
    pub supported: bool,
}

impl Plugins {
    pub fn start(setup: PluginsSetup) -> std::io::Result<Self> {
        let folder = setup.data_dir.join("plugins");
        std::fs::create_dir_all(&folder)?;
        let registry = Arc::new(RwLock::new(Registry::scan(&folder, &setup.language)));
        let host = PluginHost::new(HubSetup {
            registry: Arc::clone(&registry),
            notify: setup.notify,
            desktop: Arc::clone(&setup.desktop),
            logs: setup.data_dir.join("plugin-logs"),
        });
        let port = server::start(host.clone())?;
        let node = node::find(setup.resources.as_deref());
        match &node {
            Some(node) => info!(
                "plugins run with Node {} ({})",
                node.version,
                node.path.display()
            ),
            None => warn!("the bundled Node is missing: plugins written in JavaScript cannot run"),
        }
        let launcher = Launcher::new(
            node,
            setup.desktop,
            port,
            setup.language.clone(),
            &setup.data_dir,
        );
        let plugins = Self {
            host,
            launcher,
            registry,
            folder,
            community: Community::new(),
        };
        plugins.start_all();
        Ok(plugins)
    }

    pub fn host(&self) -> PluginHost {
        self.host.clone()
    }

    pub fn registry(&self) -> Arc<RwLock<Registry>> {
        Arc::clone(&self.registry)
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    fn start_all(&self) {
        let registry = self.registry.read().unwrap_or_else(PoisonError::into_inner);
        for plugin in registry.plugins() {
            self.launcher.start(plugin);
        }
    }

    pub fn catalog(&self) -> PluginCatalog {
        let connected = self.host.connected_plugins();
        let registry = self.registry.read().unwrap_or_else(PoisonError::into_inner);
        let plugins = registry
            .plugins()
            .iter()
            .map(|plugin| {
                let m = &plugin.manifest;
                PluginInfo {
                    id: m.uuid.clone(),
                    name: m.name.clone(),
                    author: m.author.clone(),
                    description: m.description.clone(),
                    version: m.version.clone(),
                    category: Some(m.category.clone())
                        .filter(|c| !c.trim().is_empty())
                        .unwrap_or_else(|| m.name.clone()),
                    icon: Some(m.icon.clone()).filter(|i| !i.is_empty()),
                    folder: plugin.folder.clone(),
                    run: self.launcher.state(plugin),
                    connected: connected.contains(&m.uuid),
                    actions: m
                        .actions
                        .iter()
                        .map(|a| PluginActionInfo {
                            uuid: a.uuid.clone(),
                            name: a.name.clone(),
                            tooltip: a.tooltip.clone(),
                            icon: Some(a.icon.clone())
                                .filter(|i| !i.is_empty())
                                .or_else(|| a.state_image(0).map(str::to_owned)),
                            inspector: Some(a.property_inspector_path.clone())
                                .filter(|p| !p.is_empty()),
                            supported: a.supports_keys() && a.supports_device("D200"),
                        })
                        .collect(),
                }
            })
            .collect();
        PluginCatalog {
            port: self.host.port(),
            node: self.launcher.node().map(|n| n.version.clone()),
            language: crate::i18n::current().tag().into(),
            plugins,
        }
    }

    /// Reads the folders again and starts what changed. Plugins that stay are left running.
    pub fn reload(&self) {
        // Names and descriptions in the language shown now, which may have changed.
        let fresh = Registry::scan(&self.folder, crate::i18n::current().tag());
        let mut registry = self
            .registry
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        for old in registry.plugins() {
            let kept = fresh
                .plugin(&old.manifest.uuid)
                .is_some_and(|p| p.dir == old.dir && p.manifest.version == old.manifest.version);
            if !kept {
                self.launcher.stop(&old.manifest.uuid);
                self.host.drop_plugin(&old.manifest.uuid);
            }
        }
        let started: Vec<String> = registry
            .plugins()
            .iter()
            .map(|p| p.manifest.uuid.clone())
            .collect();
        for plugin in fresh.plugins() {
            let same = registry.plugin(&plugin.manifest.uuid).is_some_and(|old| {
                old.dir == plugin.dir && old.manifest.version == plugin.manifest.version
            });
            if !same || !started.contains(&plugin.manifest.uuid) {
                self.launcher.start(plugin);
            }
        }
        *registry = fresh;
    }

    pub fn install(&self, source: &Path) -> Result<String, InstallError> {
        let id = install::install(source, &self.folder)?;
        info!("plugin {id} installed");
        self.launcher.stop(&id);
        self.host.drop_plugin(&id);
        self.reload();
        Ok(id)
    }

    pub fn uninstall(&self, id: &str) -> std::io::Result<()> {
        let dir = {
            let registry = self.registry.read().unwrap_or_else(PoisonError::into_inner);
            registry.plugin(id).map(|p| p.dir.clone())
        };
        let dir = dir.ok_or_else(|| std::io::Error::other(tr!("plugins.notInstalled")))?;
        self.launcher.stop(id);
        self.host.drop_plugin(id);
        install::uninstall(&dir, &self.folder)?;
        info!("plugin {id} removed");
        self.reload();
        Ok(())
    }

    pub fn restart(&self, id: &str) {
        let registry = self.registry.read().unwrap_or_else(PoisonError::into_inner);
        if let Some(plugin) = registry.plugin(id) {
            self.host.drop_plugin(id);
            self.launcher.start(plugin);
        }
    }

    /// What the community catalog offers that runs here, and which of it is installed.
    pub fn community(&self, refresh: bool) -> Result<Vec<CommunityPlugin>, CommunityError> {
        let registry = self.registry.read().unwrap_or_else(PoisonError::into_inner);
        self.community
            .catalog(refresh, &registry, crate::i18n::current().tag())
    }

    /// Installs, or updates, a plugin of the community catalog. Returns its id.
    pub fn install_community(&self, id: &str) -> Result<String, String> {
        let zip = self
            .community
            .download(id, &std::env::temp_dir())
            .map_err(|err| err.to_string())?;
        let installed = self.install(&zip).map_err(|err| err.to_string());
        let _ = std::fs::remove_file(&zip);
        installed
    }

    pub fn community_image(&self, url: &str) -> Option<crate::render::ImageData> {
        self.community.image(url)
    }

    pub fn stop_all(&self) {
        self.launcher.stop_all();
    }
}
