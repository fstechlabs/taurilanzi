//! A key as the assistant writes it, made whole before the engine checks it: names resolved,
//! defaults filled in, and hidden values given back what they had.

use serde_json::{Value, json};
use uuid::Uuid;

use super::context::Tools;
use super::ids::Names;
use super::reading::key_of;
use crate::config::{Config, Key};
use crate::mcp::redaction::{HIDDEN, find_hidden, restore_http, restore_settings};

pub fn prepare_key(
    tools: &Tools,
    config: &Config,
    names: &Names,
    page: Uuid,
    index: u8,
    mut value: Value,
) -> Result<Key, String> {
    if !value.is_object() {
        return Err("a key is an object: see get_schema".into());
    }
    names.resolve_in(&mut value);
    if value.get("appearance").is_none() {
        value["appearance"] = json!({
            "background": { "type": "solid", "color": config.preferences.new_keys.background },
            "layers": [],
        });
    }
    complete_plugin_actions(tools, &mut value);
    // A page created in the same call has no keys yet.
    let old = key_of(config, page, index).ok().flatten();
    if let Some(old) = old {
        let old = serde_json::to_value(old).map_err(|err| err.to_string())?;
        restore_http(&mut value, &old)?;
        restore_plugin_settings(&mut value, &old)?;
    }
    restore_plugin_settings_elsewhere(&mut value, config)?;
    if let Some(path) = find_hidden(&value) {
        return Err(format!(
            "{path} still reads {HIDDEN:?}: there is no saved value for it, so the user must enter it in the app"
        ));
    }
    serde_json::from_value(value)
        .map_err(|err| format!("the key does not match the Key type: {err}"))
}

/// Plugin actions may leave out their placement id and settings: a new placement gets an id of
/// its own and the settings the plugin's manifest starts its keys with.
fn complete_plugin_actions(tools: &Tools, key: &mut Value) {
    let catalog = tools.plugins().ok().map(|plugins| plugins.catalog());
    let Some(actions) = key.get_mut("actions").and_then(Value::as_array_mut) else {
        return;
    };
    for action in actions {
        if action.get("type").and_then(Value::as_str) != Some("plugin") {
            continue;
        }
        if action.get("id").is_none_or(Value::is_null) {
            action["id"] = json!(Uuid::new_v4());
        }
        if action.get("settings").is_none_or(Value::is_null) {
            let uuid = action
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let defaults = catalog
                .iter()
                .flat_map(|catalog| &catalog.plugins)
                .flat_map(|plugin| &plugin.actions)
                .find(|info| info.uuid == uuid)
                .map(|info| info.settings.clone())
                .unwrap_or_else(|| json!({}));
            action["settings"] = defaults;
        }
    }
}

fn restore_plugin_settings(new: &mut Value, old: &Value) -> Result<(), String> {
    let olds: Vec<&Value> = plugin_actions(old).collect();
    for action in new
        .get_mut("actions")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let Some(id) = action.get("id").cloned() else {
            continue;
        };
        // Only the same action of the same plugin gets back what its placement had.
        let same =
            |old: &&&Value| old.get("id") == Some(&id) && old.get("action") == action.get("action");
        if let Some(before) = olds.iter().find(same) {
            for part in ["settings", "resources"] {
                if let (Some(new), Some(old)) = (action.get_mut(part), before.get(part)) {
                    restore_settings(new, old)?;
                }
            }
        }
    }
    Ok(())
}

/// A key moved or copied from another slot keeps the settings its placement already has.
fn restore_plugin_settings_elsewhere(new: &mut Value, config: &Config) -> Result<(), String> {
    let mut wanted = Vec::new();
    for action in plugin_actions(new) {
        if let Some(id) = action.get("id").and_then(Value::as_str) {
            wanted.push(id.to_owned());
        }
    }
    if wanted.is_empty() {
        return Ok(());
    }
    let saved = serde_json::to_value(
        config
            .profiles
            .iter()
            .flat_map(|p| &p.pages)
            .flat_map(|page| page.keys.values())
            .collect::<Vec<_>>(),
    )
    .map_err(|err| err.to_string())?;
    let olds: Vec<Value> = saved
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(plugin_actions)
        .filter(|action| {
            action
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| wanted.iter().any(|w| w == id))
        })
        .cloned()
        .collect();
    restore_plugin_settings(new, &json!({ "actions": olds }))
}

fn plugin_actions(key: &Value) -> impl Iterator<Item = &Value> {
    key.get("actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|action| action.get("type").and_then(Value::as_str) == Some("plugin"))
}
