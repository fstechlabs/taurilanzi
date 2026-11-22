//! Everything the window can configure, the assistant can too. Each command the window sends is
//! here with the tools that do the same, or with why the assistant must not; a new command that
//! is in neither fails the tests until it is decided.

use super::handler::Assistant;

enum Cover {
    Tools(&'static [&'static str]),
    Left(&'static str),
}

use Cover::{Left, Tools};

const COVERAGE: &[(&str, Cover)] = &[
    (
        "get_state",
        Tools(&["get_overview", "get_page", "get_key", "get_settings"]),
    ),
    ("set_key", Tools(&["set_key", "apply_changes"])),
    ("clear_key", Tools(&["set_key", "apply_changes"])),
    ("move_key", Tools(&["apply_changes"])),
    ("create_page", Tools(&["apply_changes"])),
    ("update_page", Tools(&["apply_changes"])),
    ("duplicate_page", Tools(&["apply_changes"])),
    ("delete_page", Tools(&["apply_changes"])),
    ("set_home_page", Tools(&["apply_changes"])),
    ("show_page", Tools(&["show_page"])),
    (
        "edit_page",
        Left("which page the editor shows, so its plugin keys draw previews"),
    ),
    ("set_device", Tools(&["set_device_settings"])),
    ("dock_models", Tools(&["get_overview"])),
    (
        "focus_device",
        Left("which dock the editor shows; every tool names the dock or page it is about"),
    ),
    ("rename_device", Tools(&["rename_dock"])),
    (
        "forget_device",
        Left("drops a dock and its profiles for good, past undo: only the user does it"),
    ),
    ("transfer_plan", Tools(&["apply_changes"])),
    ("transfer_profile", Tools(&["apply_changes"])),
    ("set_preferences", Tools(&["set_preferences"])),
    ("try_screensaver", Tools(&["preview_screensaver"])),
    ("set_paused", Tools(&["pause_dock"])),
    ("set_profile", Tools(&["switch_profile"])),
    ("fonts", Tools(&["list_fonts"])),
    ("press_key", Tools(&["press_key"])),
    ("undo", Tools(&["undo"])),
    ("redo", Tools(&["redo"])),
    (
        "accessibility_granted",
        Left("a permission of the system, which only the user grants"),
    ),
    (
        "request_accessibility",
        Left("a permission of the system, which only the user grants"),
    ),
    ("audio_outputs", Tools(&["list_audio_outputs"])),
    ("create_profile", Tools(&["apply_changes"])),
    ("rename_profile", Tools(&["apply_changes"])),
    ("duplicate_profile", Tools(&["apply_changes"])),
    ("delete_profile", Tools(&["apply_changes"])),
    ("move_profile", Tools(&["apply_changes"])),
    ("set_profile_local", Tools(&["set_profile_local"])),
    ("link_app", Tools(&["apply_changes"])),
    ("unlink_app", Tools(&["apply_changes"])),
    ("app_at_path", Tools(&["app_at_path"])),
    ("running_apps", Tools(&["list_running_apps"])),
    (
        "app_detection",
        Left("what the desktop lets the app know, set up by the user"),
    ),
    (
        "set_up_app_detection",
        Left("installs a desktop extension, which the user approves"),
    ),
    ("save_integration", Tools(&["save_integration"])),
    ("delete_integration", Tools(&["delete_integration"])),
    ("obs_connect", Tools(&["reconnect_integration"])),
    (
        "obs_set_password",
        Left("a secret: the user types it in the app"),
    ),
    ("obs_names", Tools(&["obs_names"])),
    ("hass_connect", Tools(&["reconnect_integration"])),
    (
        "hass_set_token",
        Left("a secret: the user types it in the app"),
    ),
    ("hass_catalog", Tools(&["hass_entities", "hass_services"])),
    (
        "hass_set_code",
        Left("a secret: the user types it in the app"),
    ),
    ("hass_code_saved", Left("about a secret")),
    (
        "try_http",
        Left("sends a request the user is writing; press_key runs a saved one"),
    ),
    (
        "plugin_catalog",
        Tools(&["list_plugins", "get_plugin_action"]),
    ),
    (
        "install_plugin",
        Left("installing runs new code: only the user installs"),
    ),
    (
        "uninstall_plugin",
        Left("only the user installs and uninstalls plugins"),
    ),
    (
        "reload_plugins",
        Left("reads the plugin folder again, after the user changed it by hand"),
    ),
    ("restart_plugin", Tools(&["restart_plugin"])),
    (
        "show_plugins_folder",
        Left("opens a window of the file manager for the user"),
    ),
    (
        "store_plugins",
        Tools(&["search_plugin_store", "recommend_plugin"]),
    ),
    (
        "install_store_plugin",
        Left("installing runs new code: only the user installs"),
    ),
    (
        "update_offer",
        Left("updates of the app are the user's call"),
    ),
    (
        "install_update",
        Left("updates of the app are the user's call"),
    ),
    (
        "remind_update_later",
        Left("updates of the app are the user's call"),
    ),
    (
        "cloud_status",
        Left("the cloud account is the user's, and the MCP server is local"),
    ),
    (
        "cloud_sign_in",
        Left("the cloud account is the user's, and the MCP server is local"),
    ),
    (
        "cloud_open_web",
        Left("the cloud account is the user's, and the MCP server is local"),
    ),
    (
        "cloud_cancel_sign_in",
        Left("the cloud account is the user's, and the MCP server is local"),
    ),
    (
        "cloud_sign_out",
        Left("the cloud account is the user's, and the MCP server is local"),
    ),
    (
        "cloud_sync_now",
        Left("the cloud account is the user's, and the MCP server is local"),
    ),
    (
        "cloud_set_paused",
        Left("the cloud account is the user's, and the MCP server is local"),
    ),
    (
        "cloud_join",
        Left("the cloud account is the user's, and the MCP server is local"),
    ),
    (
        "cloud_bring_back",
        Left("the cloud account is the user's, and the MCP server is local"),
    ),
    (
        "mcp_status",
        Left("the MCP server itself: only the user turns it on and off"),
    ),
    (
        "set_mcp_enabled",
        Left("the MCP server itself: only the user turns it on and off"),
    ),
    (
        "regenerate_mcp_token",
        Left("the MCP server itself: only the user turns it on and off"),
    ),
];

/// Files and the plugins' settings come through their own paths, not through commands.
const OTHER_PATHS: &[(&str, &[&str])] = &[
    (
        "put_asset",
        &["import_image", "import_icon", "import_sound"],
    ),
    (
        "a plugin inspector saving settings",
        &["set_plugin_key_settings", "set_plugin_global_settings"],
    ),
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use super::*;
    use crate::mcp::tools::Tools;

    fn commands() -> Vec<&'static str> {
        include_str!("../ipc/bindings.rs")
            .split("collect_commands![")
            .nth(1)
            .and_then(|rest| rest.split(']').next())
            .expect("the list of commands")
            .split(',')
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(|path| path.rsplit("::").next().unwrap_or(path))
            .collect()
    }

    fn tools() -> BTreeSet<String> {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::config::Store::open(dir.path().to_path_buf()).unwrap();
        let (inputs, _) = std::sync::mpsc::channel();
        let (events, _) = tokio::sync::broadcast::channel(1);
        let engine = crate::engine::EngineHandle::new(
            inputs,
            events,
            Default::default(),
            Default::default(),
        );
        let tools = Tools {
            engine,
            plugins: None,
            assets: store.assets.clone(),
            notify: Arc::new(|_| {}),
        };
        Assistant::new(Arc::new(tools))
            .tool_names()
            .into_iter()
            .collect()
    }

    #[test]
    fn every_command_of_the_window_is_decided_for_the_assistant() {
        let decided: BTreeSet<&str> = COVERAGE.iter().map(|(command, _)| *command).collect();
        for (command, cover) in COVERAGE {
            if let Left(why) = cover {
                assert!(!why.trim().is_empty(), "say why {command} is left out");
            }
        }
        for command in commands() {
            assert!(
                decided.contains(command),
                "{command} is new: give the assistant a tool that does the same (src/mcp/tools), \
                 or say in src/mcp/coverage.rs why it must not"
            );
        }
        let listed: BTreeSet<&str> = commands().into_iter().collect();
        for command in &decided {
            assert!(listed.contains(command), "{command} is no longer a command");
        }
    }

    #[test]
    fn the_tools_named_exist_and_every_tool_is_named() {
        let tools = tools();
        let mut named = BTreeSet::new();
        let covers = COVERAGE
            .iter()
            .filter_map(|(_, cover)| match cover {
                Tools(names) => Some(*names),
                Left(_) => None,
            })
            .chain(OTHER_PATHS.iter().map(|(_, names)| *names));
        for names in covers {
            for name in names {
                assert!(tools.contains(*name), "there is no tool {name}");
                named.insert(*name);
            }
        }
        let read_only = [
            "get_schema",
            "get_guide",
            "render_key",
            "render_page",
            "search_icons",
            "get_plugin_global_settings",
        ];
        for tool in &tools {
            assert!(
                named.contains(tool.as_str()) || read_only.contains(&tool.as_str()),
                "{tool} does what no command of the window does: add it to the coverage"
            );
        }
    }
}
