//! The plugins as one piece: the packages found, the host they talk to and their processes.

use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};

use log::{info, warn};
use serde::Serialize;

use super::catalog::{Catalog, CatalogError, CatalogPlugin};
use super::desktop::Desktop;
use super::hub::{HubSetup, PluginHost, PluginInput};
use super::install::{self, InstallError};
use super::launcher::{Launcher, RunState};
use super::manifest::{Protocol, Runtime};
use super::node;
use super::pages::{self, Pages};
use super::python;
use super::registry::{Plugin, Registry};
use super::server;
use super::ulanzi::protocol::settings_object;

pub struct PluginsSetup {
    pub data_dir: PathBuf,
    /// The app's resources, where its Node and its Python are.
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
    catalog: Catalog,
    pages: Pages,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PluginCatalog {
    /// Where the host listens; the pages of the plugins are served from it.
    pub port: u16,
    /// The Node that runs plugins, if one was found.
    pub node: Option<String>,
    /// The Python that runs plugins, if one was found.
    pub python: Option<String>,
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
    /// The package folder, ending in `.ulanziPlugin` or `.sdPlugin`.
    pub folder: String,
    /// How it talks to the host.
    pub protocol: PluginProtocol,
    /// What runs its main service.
    pub runtime: PluginRuntime,
    pub run: RunState,
    /// Its main service is connected to the host.
    pub connected: bool,
    /// Where to read about it: its own website, or its page in the store it came from.
    pub website: Option<String>,
    pub actions: Vec<PluginActionInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PluginProtocol {
    /// Made for Ulanzi Studio.
    Ulanzi,
    /// Made for the Stream Deck or for Mirabox's Stream Dock.
    StreamDeck,
}

impl From<Protocol> for PluginProtocol {
    fn from(protocol: Protocol) -> Self {
        match protocol {
            Protocol::Ulanzi => Self::Ulanzi,
            Protocol::StreamDeck => Self::StreamDeck,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PluginRuntime {
    Node,
    Html,
    Python,
    /// A program of its own.
    Native,
    /// Keyboard shortcuts the host presses, in Stream Dock plugins that run nothing.
    Shortcuts,
    /// Nothing runs: images only.
    None,
}

impl From<&Runtime> for PluginRuntime {
    fn from(runtime: &Runtime) -> Self {
        match runtime {
            Runtime::Node(_) => Self::Node,
            Runtime::Html(_) => Self::Html,
            Runtime::Python(_) => Self::Python,
            Runtime::Native(_) => Self::Native,
            Runtime::None => Self::None,
        }
    }
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
    /// What a new key of it starts with.
    #[specta(type = specta_typescript::Any)]
    pub settings: serde_json::Value,
    /// Offered to put on keys. Plugins hide those only the profiles they bring use, which keys
    /// may still have.
    pub visible: bool,
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
        if let Err(err) = super::elgato::system_events::watch(host.clone(), Arc::clone(&registry)) {
            warn!("plugins will not hear of apps and sleep: {err}");
        }
        let node = node::find(setup.resources.as_deref());
        match &node {
            Some(node) => info!(
                "plugins run with Node {} ({})",
                node.version,
                node.path.display()
            ),
            None => warn!("the bundled Node is missing: plugins written in JavaScript cannot run"),
        }
        let python = python::find(setup.resources.as_deref());
        match &python {
            Some(python) => info!(
                "plugins run with Python {} ({})",
                python.version,
                python.path.display()
            ),
            None => warn!("the bundled Python is missing: plugins written in Python cannot run"),
        }
        let launcher = Launcher::new(
            node,
            python,
            setup.desktop,
            port,
            setup.language.clone(),
            &setup.data_dir,
        );
        let plugins = Self {
            host,
            launcher,
            registry,
            pages: Pages::open(setup.data_dir.join("plugin-pages.json")),
            folder,
            catalog: Catalog::new(),
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
                // The host presses what these plugins hold: there is nothing to wait for.
                let shortcuts = plugin.protocol == Protocol::StreamDeck
                    && plugin.runtime == Runtime::None
                    && m.actions
                        .iter()
                        .any(|a| super::mirabox::shortcut::from_settings(&a.settings).is_some());
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
                    protocol: PluginProtocol::from(plugin.protocol),
                    runtime: if shortcuts {
                        PluginRuntime::Shortcuts
                    } else {
                        PluginRuntime::from(&plugin.runtime)
                    },
                    run: if shortcuts {
                        RunState::Running
                    } else {
                        self.launcher.state(plugin)
                    },
                    connected: shortcuts || connected.contains(&m.uuid),
                    website: m
                        .url
                        .as_deref()
                        .map(str::trim)
                        .filter(|u| pages::web_page(u))
                        .map(str::to_owned)
                        .or_else(|| self.pages.get(&m.uuid)),
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
                            settings: settings_object(&a.settings),
                            visible: a.visible_in_actions_list,
                        })
                        .collect(),
                }
            })
            .collect();
        PluginCatalog {
            port: self.host.port(),
            node: self.launcher.node().map(|n| n.version.clone()),
            python: self.launcher.python().map(|p| p.version.clone()),
            language: crate::i18n::current().tag().into(),
            plugins,
        }
    }

    /// Reads the folders again and starts what changed. Plugins that stay are left running.
    pub fn reload(&self) {
        // Names and descriptions in the language shown now, which may have changed.
        let fresh = Registry::scan(&self.folder, crate::i18n::current().tag());
        // The hub reads the registry while it holds itself: nothing here holds the registry while
        // it reaches the hub.
        let (gone, changed) = {
            let registry = self.registry.read().unwrap_or_else(PoisonError::into_inner);
            let same =
                |a: &Plugin, b: &Plugin| a.dir == b.dir && a.manifest.version == b.manifest.version;
            let gone: Vec<String> = registry
                .plugins()
                .iter()
                .filter(|old| {
                    !fresh
                        .plugin(&old.manifest.uuid)
                        .is_some_and(|p| same(p, old))
                })
                .map(|old| old.manifest.uuid.clone())
                .collect();
            let changed: Vec<Plugin> = fresh
                .plugins()
                .iter()
                .filter(|new| {
                    !registry
                        .plugin(&new.manifest.uuid)
                        .is_some_and(|p| same(p, new))
                })
                .cloned()
                .collect();
            (gone, changed)
        };
        for id in &gone {
            self.launcher.stop(id);
            self.host.drop_plugin(id);
        }
        *self
            .registry
            .write()
            .unwrap_or_else(PoisonError::into_inner) = fresh;
        for plugin in &changed {
            self.launcher.start(plugin);
        }
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
        self.pages.set(id, None);
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

    /// What the stores offer that runs here, and which of it is installed.
    pub fn store(&self, refresh: bool) -> Result<Vec<CatalogPlugin>, CatalogError> {
        let registry = self.registry.read().unwrap_or_else(PoisonError::into_inner);
        self.catalog
            .plugins(refresh, &registry, crate::i18n::current().tag())
    }

    /// Installs, or updates, a plugin of a store. Returns its id.
    pub fn install_from_store(&self, id: &str) -> Result<String, String> {
        let zip = self
            .catalog
            .download(id, crate::i18n::current().tag(), &std::env::temp_dir())
            .map_err(|err| err.to_string())?;
        let installed = self.install(&zip).map_err(|err| err.to_string());
        let _ = std::fs::remove_file(&zip);
        if let Ok(plugin) = &installed
            && let Some(entry) = self
                .store(false)
                .ok()
                .and_then(|store| store.into_iter().find(|entry| entry.id == id))
        {
            self.pages.set(plugin, entry.source_url);
        }
        installed
    }

    pub fn store_image(&self, url: &str) -> Option<crate::render::ImageData> {
        self.catalog.image(url, crate::i18n::current().tag())
    }

    pub fn stop_all(&self) {
        self.launcher.stop_all();
    }
}
