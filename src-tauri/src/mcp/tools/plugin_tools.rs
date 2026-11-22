//! Plugins: the assistant sets up the keys of those installed, and recommends others from the
//! stores, which only the user installs.

use std::path::Path;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::context::{Answer, Notice, engine_error, json, refused, text};
use super::ids::parse_id;
use super::reading::key_of;
use crate::config::Action;
use crate::mcp::handler::Assistant;
use crate::mcp::redaction::{HIDDEN, find_hidden, hide_settings, restore_settings};

/// Enough of an inspector to see what its settings mean, without flooding the conversation.
const MAX_INSPECTOR_BYTES: usize = 120_000;
const DEFAULT_LIMIT: usize = 20;
const EXAMPLES: usize = 3;

#[derive(Deserialize, JsonSchema)]
pub struct ActionParams {
    /// The action's id in its plugin's manifest, from list_plugins.
    pub action: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct StoreParams {
    /// Words to find in the name, description, category or tags. Empty lists the most
    /// downloaded.
    #[serde(default)]
    pub search: String,
    /// At most this many; 20 by default.
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RecommendParams {
    /// The `id` of the plugin in the store, from search_plugin_store.
    pub id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct KeySettingsParams {
    pub page: String,
    pub index: u8,
    /// The settings of the plugin action on that key, whole. Hidden values sent back unchanged
    /// keep what they had.
    pub settings: Value,
}

#[derive(Deserialize, JsonSchema)]
pub struct PluginParams {
    /// The plugin's id, from list_plugins.
    pub plugin: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct GlobalSettingsParams {
    pub plugin: String,
    /// What the plugin shares between its keys, whole.
    pub settings: Value,
}

#[tool_router(router = plugin_tools, vis = "pub(in crate::mcp)")]
impl Assistant {
    #[tool(
        description = "Plugins installed, whether they run, and their actions: the id to put in a plugin action, the dock models whose keys it fits (`models`, ids as in the layout of each dock in get_overview), and the settings a new key starts with.",
        annotations(read_only_hint = true)
    )]
    async fn list_plugins(&self) -> Answer {
        let plugins = match self.tools.plugins() {
            Ok(plugins) => plugins,
            Err(err) => return refused(err),
        };
        let catalog = plugins.catalog();
        let list: Vec<Value> = catalog
            .plugins
            .iter()
            .map(|plugin| {
                json!({
                    "id": plugin.id,
                    "name": plugin.name,
                    "description": plugin.description,
                    "version": plugin.version,
                    "protocol": plugin.protocol,
                    "running": plugin.connected,
                    "actions": plugin.actions.iter().filter(|a| a.visible).map(|action| json!({
                        "action": action.uuid,
                        "name": action.name,
                        "tooltip": action.tooltip,
                        "models": action.models,
                        "has_inspector": action.inspector.is_some(),
                        "default_settings": action.settings,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        json(list)
    }

    #[tool(
        description = "What the settings of a plugin action mean: its property inspector (the HTML and scripts where the user would set it up), the settings new keys start with, and the settings of keys that already have it. Read it before writing settings for a plugin key.",
        annotations(read_only_hint = true)
    )]
    async fn get_plugin_action(&self, Parameters(params): Parameters<ActionParams>) -> Answer {
        let plugins = match self.tools.plugins() {
            Ok(plugins) => plugins,
            Err(err) => return refused(err),
        };
        let catalog = plugins.catalog();
        let Some((plugin, action)) = catalog.plugins.iter().find_map(|plugin| {
            plugin
                .actions
                .iter()
                .find(|a| a.uuid == params.action)
                .map(|action| (plugin, action))
        }) else {
            return refused(format!(
                "no installed plugin has the action {}",
                params.action
            ));
        };
        let folder = plugins.folder().join(&plugin.folder);
        let inspector = action
            .inspector
            .as_deref()
            .map(|page| inspector_source(&folder, page));
        let config = match self.tools.engine.config().await {
            Ok(config) => config,
            Err(err) => return engine_error(err),
        };
        let examples: Vec<Value> = config
            .profiles
            .iter()
            .flat_map(|p| &p.pages)
            .flat_map(|page| page.keys.values())
            .flat_map(|key| &key.actions)
            .filter_map(|a| match a {
                Action::Plugin {
                    action, settings, ..
                } if *action == params.action => {
                    let mut settings = settings.clone();
                    hide_settings(&mut settings);
                    Some(settings)
                }
                _ => None,
            })
            .take(EXAMPLES)
            .collect();
        json(json!({
            "plugin": { "id": plugin.id, "name": plugin.name, "protocol": plugin.protocol },
            "action": action.uuid,
            "name": action.name,
            "tooltip": action.tooltip,
            "models": action.models,
            "default_settings": action.settings,
            "settings_of_existing_keys": examples,
            "inspector": inspector,
        }))
    }

    #[tool(
        description = "Writes the settings of the plugin action on a key, as its property inspector would, and tells the plugin. Not undoable: settings belong to the plugin.",
        annotations(destructive_hint = true)
    )]
    async fn set_plugin_key_settings(
        &self,
        Parameters(params): Parameters<KeySettingsParams>,
    ) -> Answer {
        let page = match parse_id(&params.page, "page") {
            Ok(page) => page,
            Err(err) => return refused(err),
        };
        let config = match self.tools.engine.config().await {
            Ok(config) => config,
            Err(err) => return engine_error(err),
        };
        let key = match key_of(&config, page, params.index) {
            Ok(Some(key)) => key,
            Ok(None) => return refused("that slot is empty"),
            Err(err) => return refused(err),
        };
        let Some((placement, old)) = key.actions.iter().find_map(|a| match a {
            Action::Plugin { id, settings, .. } => Some((*id, settings.clone())),
            _ => None,
        }) else {
            return refused("that key has no plugin action");
        };
        let mut settings = params.settings;
        if !settings.is_object() {
            return refused("settings are an object");
        }
        if let Err(err) = restore_settings(&mut settings, &old) {
            return refused(err);
        }
        if let Some(path) = find_hidden(&settings) {
            return refused(format!(
                "{path} still reads {HIDDEN:?}: the user must enter it in the app"
            ));
        }
        match self
            .tools
            .engine
            .set_plugin_settings(placement, settings)
            .await
        {
            Ok(()) => {
                self.tools.edited(false);
                text("saved and sent to the plugin")
            }
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "What a plugin shares between all its keys (accounts, addresses…), with values that look like secrets hidden.",
        annotations(read_only_hint = true)
    )]
    async fn get_plugin_global_settings(
        &self,
        Parameters(params): Parameters<PluginParams>,
    ) -> Answer {
        match self.tools.engine.config().await {
            Ok(config) => {
                let mut settings = config
                    .plugin_settings
                    .get(&params.plugin)
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                hide_settings(&mut settings);
                json(settings)
            }
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "Writes what a plugin shares between all its keys and tells the plugin. Hidden values sent back unchanged keep what they had; new secrets must be typed by the user in the plugin's own settings.",
        annotations(destructive_hint = true)
    )]
    async fn set_plugin_global_settings(
        &self,
        Parameters(params): Parameters<GlobalSettingsParams>,
    ) -> Answer {
        let mut settings = params.settings;
        if !settings.is_object() {
            return refused("settings are an object");
        }
        let old = match self.tools.engine.config().await {
            Ok(config) => config
                .plugin_settings
                .get(&params.plugin)
                .cloned()
                .unwrap_or_else(|| json!({})),
            Err(err) => return engine_error(err),
        };
        if let Err(err) = restore_settings(&mut settings, &old) {
            return refused(err);
        }
        if let Some(path) = find_hidden(&settings) {
            return refused(format!(
                "{path} still reads {HIDDEN:?}: the user must enter it in the app"
            ));
        }
        match self
            .tools
            .engine
            .set_global_plugin_settings(params.plugin, settings)
            .await
        {
            Ok(()) => {
                self.tools.edited(false);
                text("saved and sent to the plugin")
            }
            Err(err) => engine_error(err),
        }
    }

    #[tool(description = "Starts a plugin again, as when it misbehaves.")]
    async fn restart_plugin(&self, Parameters(params): Parameters<PluginParams>) -> Answer {
        let plugins = match self.tools.plugins() {
            Ok(plugins) => plugins.clone(),
            Err(err) => return refused(err),
        };
        let _ = tokio::task::spawn_blocking(move || plugins.restart(&params.plugin)).await;
        text("restarted")
    }

    #[tool(
        description = "Free plugins of the stores (Ulanzi's Marketplace, the Ulanzi Community Store, Mirabox Space and OpenDeck's) that run here, to recommend one. You cannot install them: recommend_plugin shows one to the user, who installs it.",
        annotations(read_only_hint = true)
    )]
    async fn search_plugin_store(&self, Parameters(params): Parameters<StoreParams>) -> Answer {
        let plugins = match self.tools.plugins() {
            Ok(plugins) => plugins.clone(),
            Err(err) => return refused(err),
        };
        let store = match tokio::task::spawn_blocking(move || plugins.store(false)).await {
            Ok(Ok(store)) => store,
            Ok(Err(err)) => return refused(format!("the stores did not answer: {err}")),
            Err(err) => return refused(err.to_string()),
        };
        let words: Vec<String> = params
            .search
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let mut found: Vec<_> = store
            .iter()
            .filter(|plugin| {
                let haystack = format!(
                    "{} {} {} {}",
                    plugin.name,
                    plugin.description,
                    plugin.category,
                    plugin.tags.join(" ")
                )
                .to_lowercase();
                words.iter().all(|word| haystack.contains(word))
            })
            .collect();
        found.sort_by_key(|plugin| std::cmp::Reverse(plugin.downloads));
        let list: Vec<Value> = found
            .into_iter()
            .take(params.limit.unwrap_or(DEFAULT_LIMIT))
            .map(|plugin| {
                json!({
                    "id": plugin.id,
                    "name": plugin.name,
                    "author": plugin.author,
                    "description": plugin.description,
                    "category": plugin.category,
                    "store": plugin.source,
                    "installed": plugin.installed,
                    "downloads": plugin.downloads,
                    "also_has_dials": plugin.dials,
                    "security_scan": plugin.security.status,
                })
            })
            .collect();
        json(list)
    }

    #[tool(
        description = "Shows the user a plugin of the stores in the app, ready to install with one click. Tell the user why; once they install it, list_plugins shows its actions."
    )]
    async fn recommend_plugin(&self, Parameters(params): Parameters<RecommendParams>) -> Answer {
        let plugins = match self.tools.plugins() {
            Ok(plugins) => plugins.clone(),
            Err(err) => return refused(err),
        };
        let store = match tokio::task::spawn_blocking(move || plugins.store(false)).await {
            Ok(Ok(store)) => store,
            Ok(Err(err)) => return refused(format!("the stores did not answer: {err}")),
            Err(err) => return refused(err.to_string()),
        };
        let Some(plugin) = store
            .iter()
            .find(|p| p.id == params.id || p.plugin == params.id)
        else {
            return refused(format!("no store has a plugin {}", params.id));
        };
        if plugin.installed.is_some() && !plugin.update {
            return text(format!("{} is already installed", plugin.name));
        }
        (self.tools.notify)(Notice::Recommended {
            id: plugin.id.clone(),
            name: plugin.name.clone(),
        });
        text(format!(
            "the app now shows {} to the user, who can install it with one click",
            plugin.name
        ))
    }
}

/// The inspector page and the scripts of the plugin it loads, as text.
fn inspector_source(folder: &Path, page: &str) -> Value {
    let page = page.split(['?', '#']).next().unwrap_or(page);
    let Some(path) = inside(folder, &folder.join(page)) else {
        return json!({ "page": page, "missing": true });
    };
    let Ok(html) = std::fs::read_to_string(&path) else {
        return json!({ "page": page, "missing": true });
    };
    let base = path.parent().unwrap_or(folder);
    let mut budget = MAX_INSPECTOR_BYTES.saturating_sub(html.len());
    let mut scripts = Vec::new();
    for source in local_scripts(&html) {
        let Some(file) = inside(folder, &base.join(&source)) else {
            continue;
        };
        if let Ok(code) = std::fs::read_to_string(&file) {
            if code.len() > budget {
                scripts.push(json!({ "src": source, "left_out": "too long" }));
                continue;
            }
            budget -= code.len();
            scripts.push(json!({ "src": source, "code": code }));
        }
    }
    let html: String = html.chars().take(MAX_INSPECTOR_BYTES).collect();
    json!({ "page": page, "html": html, "scripts": scripts })
}

/// The file, if it really is inside the plugin's folder once `..` and links are followed: the
/// assistant reads nothing else of the computer through a plugin.
fn inside(folder: &Path, file: &Path) -> Option<std::path::PathBuf> {
    let folder = folder.canonicalize().ok()?;
    let file = file.canonicalize().ok()?;
    file.starts_with(&folder).then_some(file)
}

/// `src` of `<script>` tags that point inside the plugin, not to the web.
fn local_scripts(html: &str) -> Vec<String> {
    html.split("<script")
        .skip(1)
        .filter_map(|tag| {
            let tag = tag.split('>').next()?;
            let at = tag.find("src=")? + 4;
            let rest = &tag[at..];
            let quote = rest.chars().next()?;
            let source = if quote == '"' || quote == '\'' {
                rest[1..].split(quote).next()?
            } else {
                rest.split_whitespace().next()?
            };
            let source = source.split(['?', '#']).next()?;
            (!source.contains("://") && !source.starts_with("//")).then(|| source.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_outside_the_plugin_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let plugin = dir.path().join("x.ulanziPlugin");
        std::fs::create_dir_all(plugin.join("pi")).unwrap();
        std::fs::write(dir.path().join("secret.txt"), "s").unwrap();
        std::fs::write(plugin.join("pi/app.js"), "a").unwrap();
        assert!(inside(&plugin, &plugin.join("pi/../pi/app.js")).is_some());
        assert!(inside(&plugin, &plugin.join("pi/../../secret.txt")).is_none());
    }

    #[test]
    fn only_the_plugins_own_scripts_are_read() {
        let html = r#"<script src="js/app.js"></script><script src='../libs/sdk.js?v=2'></script>
            <script src="https://cdn.example.com/x.js"></script><script>inline()</script>"#;
        assert_eq!(local_scripts(html), ["js/app.js", "../libs/sdk.js"]);
    }
}
