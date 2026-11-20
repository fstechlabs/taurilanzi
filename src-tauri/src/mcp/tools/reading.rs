//! Reading the configuration: the overview first, then pages and keys in full, settings, and
//! what the computer offers to point keys at.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::context::{Answer, engine_error, json, refused, text};
use super::ids::parse_id;
use crate::config::{Action, Config, Key, LayerConfig, Page};
use crate::mcp::handler::Assistant;
use crate::mcp::redaction::hide_in_actions;
use crate::mcp::schema;

const GUIDE: &str = include_str!("../guide.md");

/// How the keys sit on the D200, for the assistant to place them.
const LAYOUT: &str = "15 slots in 5 columns and 3 rows. Keys 0-4 are the top row, 5-9 the \
middle row and 10-12 the first three of the bottom row, left to right. Key 13 is the wide key \
(452x196 px): the last two slots of the bottom row. The other keys are 196x196 px.";

#[derive(Deserialize, JsonSchema)]
pub struct PageParams {
    /// Id of the page, in any profile.
    pub page: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct KeyParams {
    /// Id of the page, in any profile.
    pub page: String,
    /// 0 to 12, or 13 for the wide key.
    pub index: u8,
}

#[derive(Deserialize, JsonSchema)]
pub struct SchemaParams {
    /// The type to describe, with every type it uses: `Key` (default), `Action`, `Widget`,
    /// `Appearance`, `Preferences`, `DeviceSettings`, `Integration`, or any other type name.
    #[serde(default)]
    pub r#type: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct PathParams {
    /// Full path to an app (a `.app` on macOS, an `.exe` or shortcut on Windows, a `.desktop`
    /// file on Linux).
    pub path: String,
}

#[tool_router(router = reading, vis = "pub(in crate::mcp)")]
impl Assistant {
    #[tool(
        description = "Start here. Every profile with its pages, which keys each page has (with a short summary), the links between pages, the integrations (OBS, Home Assistant) and their status, the device and how its keys are laid out, and what the dock shows now.",
        annotations(read_only_hint = true)
    )]
    async fn get_overview(&self) -> Answer {
        let (config, snapshot) = match (
            self.tools.engine.config().await,
            self.tools.engine.snapshot().await,
        ) {
            (Ok(config), Ok(snapshot)) => (config, snapshot),
            (Err(err), _) | (_, Err(err)) => return engine_error(err),
        };
        let profiles: Vec<Value> = config
            .profiles
            .iter()
            .map(|profile| {
                json!({
                    "id": profile.id,
                    "name": profile.name,
                    "active": profile.id == config.active_profile,
                    "kept_on_this_computer_only": config.local_profiles.contains(&profile.id),
                    "home_page": profile.home_page,
                    "linked_apps": profile.apps,
                    "pages": profile.pages.iter().map(page_summary).collect::<Vec<_>>(),
                })
            })
            .collect();
        let integrations: Vec<Value> = config
            .integrations
            .iter()
            .map(|integration| {
                let id = integration.id();
                let mut value = serde_json::to_value(integration).unwrap_or_default();
                let status = snapshot
                    .obs
                    .get(&id)
                    .map(|info| serde_json::to_value(info).unwrap_or_default())
                    .or_else(|| {
                        snapshot
                            .hass
                            .get(&id)
                            .map(|info| serde_json::to_value(info).unwrap_or_default())
                    });
                value["status"] = status.unwrap_or(Value::Null);
                value
            })
            .collect();
        json(json!({
            "device": {
                "model": snapshot.device.as_ref().map(|d| d.model.clone()),
                "connected": snapshot.connected,
                "layout": LAYOUT,
            },
            "language": snapshot.language,
            "active_profile": config.active_profile,
            "page_on_the_dock": snapshot.path.last(),
            "profiles": profiles,
            "integrations": integrations,
            "can_undo": snapshot.can_undo,
            "can_redo": snapshot.can_redo,
        }))
    }

    #[tool(
        description = "A page in full: its name, place on the canvas and every key as JSON (the `Key` type of get_schema). Values that look like passwords or tokens read as a placeholder; send it back unchanged to keep them.",
        annotations(read_only_hint = true)
    )]
    async fn get_page(&self, Parameters(params): Parameters<PageParams>) -> Answer {
        let page = match parse_id(&params.page, "page") {
            Ok(page) => page,
            Err(err) => return refused(err),
        };
        let config = match self.tools.engine.config().await {
            Ok(config) => config,
            Err(err) => return engine_error(err),
        };
        let Some(profile) = config.page_owner(page) else {
            return refused(format!("page {page} does not exist"));
        };
        let page = profile.page(page).expect("the page of its owner");
        let mut value = serde_json::to_value(page).unwrap_or_default();
        hide_in_actions(&mut value);
        value["profile"] = json!(profile.id);
        value["is_home_page"] = json!(profile.home_page == page.id);
        json(value)
    }

    #[tool(
        description = "One key as JSON (the `Key` type of get_schema), or null if the slot is empty.",
        annotations(read_only_hint = true)
    )]
    async fn get_key(&self, Parameters(params): Parameters<KeyParams>) -> Answer {
        let page = match parse_id(&params.page, "page") {
            Ok(page) => page,
            Err(err) => return refused(err),
        };
        let config = match self.tools.engine.config().await {
            Ok(config) => config,
            Err(err) => return engine_error(err),
        };
        match key_of(&config, page, params.index) {
            Ok(key) => {
                let mut value = serde_json::to_value(key).unwrap_or_default();
                hide_in_actions(&mut value);
                json(value)
            }
            Err(err) => refused(err),
        }
    }

    #[tool(
        description = "The device settings (brightness, sleep) and the preferences: language, gesture timing, the look of new keys, the screensaver, and whether profiles follow the app in front.",
        annotations(read_only_hint = true)
    )]
    async fn get_settings(&self) -> Answer {
        match self.tools.engine.config().await {
            Ok(config) => json(json!({
                "device": config.device,
                "preferences": config.preferences,
            })),
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "The exact JSON shape of a configuration type, as TypeScript, with the comments that explain each field and every type it uses. Read it before writing keys, actions or widgets.",
        annotations(read_only_hint = true)
    )]
    async fn get_schema(&self, Parameters(params): Parameters<SchemaParams>) -> Answer {
        let name = params.r#type.unwrap_or_else(|| "Key".into());
        match schema::describe(&name) {
            Some(types) => text(types),
            None => refused(format!(
                "there is no type named {name}; try Key, Action, Widget, Appearance, Preferences, DeviceSettings or Integration"
            )),
        }
    }

    #[tool(
        description = "How to build good keys: the kinds of key and what each needs, recipes for common ones (icon and label, page links, widgets, two states, OBS, Home Assistant, plugins), and the rules every change must follow.",
        annotations(read_only_hint = true)
    )]
    async fn get_guide(&self) -> Answer {
        text(GUIDE)
    }

    #[tool(
        description = "Font families keys can use in text layers and widgets.",
        annotations(read_only_hint = true)
    )]
    async fn list_fonts(&self) -> Answer {
        match self.tools.engine.fonts().await {
            Ok(fonts) => json(fonts),
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "Names of the sound outputs a play_sound action can pick (`device`); none means the system default.",
        annotations(read_only_hint = true)
    )]
    async fn list_audio_outputs(&self) -> Answer {
        json(crate::actions::output_devices())
    }

    #[tool(
        description = "Apps running now, with the ids profiles link to (`linked_apps`). close_app takes the path of an app instead.",
        annotations(read_only_hint = true)
    )]
    async fn list_running_apps(&self) -> Answer {
        let apps = tokio::task::spawn_blocking(crate::foreground::running_apps)
            .await
            .unwrap_or_default();
        json(apps)
    }

    #[tool(
        description = "The app at a path, as profiles link to it: its id and name.",
        annotations(read_only_hint = true)
    )]
    async fn app_at_path(&self, Parameters(params): Parameters<PathParams>) -> Answer {
        let path = std::path::PathBuf::from(params.path);
        match tokio::task::spawn_blocking(move || crate::foreground::app_at(&path))
            .await
            .ok()
            .flatten()
        {
            Some(app) => json(app),
            None => refused("that is not an app"),
        }
    }
}

pub fn key_of(config: &Config, page: uuid::Uuid, index: u8) -> Result<Option<&Key>, String> {
    let profile = config
        .page_owner(page)
        .ok_or_else(|| format!("page {page} does not exist"))?;
    let page = profile.page(page).expect("the page of its owner");
    Ok(page.keys.get(&index))
}

fn page_summary(page: &Page) -> Value {
    let keys: Vec<Value> = page
        .keys
        .iter()
        .map(|(index, key)| {
            json!({
                "index": index,
                "kind": key.kind,
                "text": key.appearance.layers.iter().find_map(|layer| match layer {
                    LayerConfig::Text { content, .. } => Some(content.clone()),
                    _ => None,
                }),
                "widgets": key.appearance.layers.iter().filter_map(|layer| match layer {
                    LayerConfig::Widget { widget } => serde_json::to_value(widget)
                        .ok()
                        .and_then(|w| w.get("kind").cloned()),
                    _ => None,
                }).collect::<Vec<_>>(),
                "actions": key.actions.iter().map(action_name).collect::<Vec<_>>(),
                "double_press": key.double_press.iter().map(action_name).collect::<Vec<_>>(),
                "long_press": key.long_press.iter().map(action_name).collect::<Vec<_>>(),
                "two_states": key.second_state.is_some(),
            })
        })
        .collect();
    let links: Vec<Value> = page
        .keys
        .iter()
        .flat_map(|(index, key)| {
            key.actions.iter().filter_map(move |action| match action {
                Action::OpenPage { page, .. } => Some(json!({ "key": index, "to_page": page })),
                _ => None,
            })
        })
        .collect();
    json!({
        "id": page.id,
        "name": page.name,
        "position": page.position,
        "keys": keys,
        "links": links,
    })
}

fn action_name(action: &Action) -> Value {
    let value = serde_json::to_value(action).unwrap_or_default();
    match action {
        Action::Plugin { action, .. } => json!(format!("plugin:{action}")),
        _ => value.get("type").cloned().unwrap_or_default(),
    }
}
