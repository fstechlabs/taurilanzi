//! The settings of each dock and the app's preferences.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::context::{Answer, engine_error, json, refused, text};
use super::ids::parse_id;
use crate::config::{DeviceSettings, Preferences};
use crate::mcp::handler::Assistant;

#[derive(Deserialize, JsonSchema)]
pub struct DeviceParams {
    /// The dock, by an id from get_overview; the one the app shows if left out.
    pub dock: Option<String>,
    /// 0 to 100.
    pub brightness: Option<u8>,
    /// Minutes without use before the dock sleeps: one of the times its firmware offers (see
    /// get_settings), 0 never.
    pub sleep_minutes: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RenameDockParams {
    /// The dock, by an id from get_overview.
    pub dock: String,
    /// Empty goes back to the name of its model.
    pub name: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct PreferencesParams {
    /// Only the fields to change, shaped as `Preferences` (see get_schema): nested objects
    /// merge with what there is.
    pub changes: Value,
}

#[derive(Deserialize, JsonSchema)]
pub struct OnParams {
    pub on: bool,
}

#[tool_router(router = settings, vis = "pub(in crate::mcp)")]
impl Assistant {
    #[tool(description = "Changes a dock's brightness and how long it waits before sleeping.")]
    async fn set_device_settings(&self, Parameters(params): Parameters<DeviceParams>) -> Answer {
        let dock = match params.dock.map(|d| parse_id(&d, "dock")).transpose() {
            Ok(dock) => dock,
            Err(err) => return refused(err),
        };
        let (config, snapshot) = match (
            self.tools.engine.config().await,
            self.tools.engine.snapshot().await,
        ) {
            (Ok(config), Ok(snapshot)) => (config, snapshot),
            (Err(err), _) | (_, Err(err)) => return engine_error(err),
        };
        let dock = dock.unwrap_or(snapshot.device_id);
        let Some(current) = config.device(dock).map(|d| d.settings.clone()) else {
            return refused(format!("dock {dock} does not exist"));
        };
        let settings = DeviceSettings {
            brightness: params.brightness.unwrap_or(current.brightness),
            sleep_minutes: params.sleep_minutes.unwrap_or(current.sleep_minutes),
        };
        match self.tools.engine.set_device(dock, settings).await {
            Ok(()) => {
                self.tools.edited(false);
                text("saved")
            }
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "Renames a dock, as the app and its plugins show it; an empty name goes back to the model's."
    )]
    async fn rename_dock(&self, Parameters(params): Parameters<RenameDockParams>) -> Answer {
        let dock = match parse_id(&params.dock, "dock") {
            Ok(dock) => dock,
            Err(err) => return refused(err),
        };
        match self.tools.engine.rename_device(dock, params.name).await {
            Ok(()) => {
                self.tools.edited(false);
                text("renamed")
            }
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "Changes preferences: language, gesture timing, the look of new keys, the screensaver, and whether profiles follow the app in front. Answers the preferences as saved."
    )]
    async fn set_preferences(&self, Parameters(params): Parameters<PreferencesParams>) -> Answer {
        if !params.changes.is_object() {
            return refused("changes is an object with the fields of Preferences to change");
        }
        let current = match self.tools.engine.config().await {
            Ok(config) => config.preferences,
            Err(err) => return engine_error(err),
        };
        let mut merged = serde_json::to_value(&current).unwrap_or_default();
        merge(&mut merged, params.changes);
        let preferences: Preferences = match serde_json::from_value(merged) {
            Ok(preferences) => preferences,
            Err(err) => return refused(format!("those preferences do not match the type: {err}")),
        };
        let language = crate::i18n::current();
        if let Err(err) = self.tools.engine.set_preferences(preferences).await {
            return engine_error(err);
        }
        if crate::i18n::current() != language
            && let Some(plugins) = self.tools.plugins.clone()
        {
            // Plugins name their actions in the user's language, as the window's change does.
            let _ = tokio::task::spawn_blocking(move || plugins.reload()).await;
        }
        self.tools.edited(false);
        match self.tools.engine.config().await {
            Ok(config) => json(config.preferences),
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "Shows the screensaver on the dock for a while to see it, or stops showing it."
    )]
    async fn preview_screensaver(&self, Parameters(params): Parameters<OnParams>) -> Answer {
        match self.tools.engine.try_screensaver(params.on).await {
            Ok(()) => text("done"),
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "Pauses the dock (dark, keys do nothing) or resumes it, as the tray menu does."
    )]
    async fn pause_dock(&self, Parameters(params): Parameters<OnParams>) -> Answer {
        match self.tools.engine.set_paused(params.on).await {
            Ok(()) => text("done"),
            Err(err) => engine_error(err),
        }
    }
}

fn merge(base: &mut Value, changes: Value) {
    match (base, changes) {
        (Value::Object(base), Value::Object(changes)) => {
            for (name, change) in changes {
                match base.get_mut(&name) {
                    Some(field) if field.is_object() && change.is_object() => merge(field, change),
                    _ => {
                        base.insert(name, change);
                    }
                }
            }
        }
        (base, changes) => *base = changes,
    }
}
