use tauri::Wry;
use tauri::ipc::Invoke;

use super::{
    assets, commands, events, hass, http, integrations, obs, plugins, profiles, system, updates,
};

/// Typed commands and events, shared by the app and the TypeScript export.
pub fn builder() -> tauri_specta::Builder<Wry> {
    tauri_specta::Builder::<Wry>::new()
        .disable_serde_phases()
        .commands(tauri_specta::collect_commands![
            commands::get_state,
            commands::set_key,
            commands::clear_key,
            commands::move_key,
            commands::create_page,
            commands::update_page,
            commands::duplicate_page,
            commands::delete_page,
            commands::set_home_page,
            commands::show_page,
            commands::set_device,
            commands::set_preferences,
            commands::try_screensaver,
            commands::set_paused,
            commands::set_profile,
            commands::fonts,
            commands::press_key,
            commands::undo,
            commands::redo,
            system::accessibility_granted,
            system::request_accessibility,
            system::audio_outputs,
            profiles::create_profile,
            profiles::rename_profile,
            profiles::duplicate_profile,
            profiles::delete_profile,
            profiles::move_profile,
            profiles::link_app,
            profiles::unlink_app,
            profiles::app_at_path,
            profiles::running_apps,
            profiles::app_detection,
            profiles::set_up_app_detection,
            integrations::save_integration,
            integrations::delete_integration,
            obs::obs_connect,
            obs::obs_set_password,
            obs::obs_names,
            hass::hass_connect,
            hass::hass_set_token,
            hass::hass_catalog,
            hass::hass_set_code,
            hass::hass_code_saved,
            http::try_http,
            plugins::plugin_catalog,
            plugins::install_plugin,
            plugins::uninstall_plugin,
            plugins::reload_plugins,
            plugins::restart_plugin,
            plugins::show_plugins_folder,
            plugins::community_plugins,
            plugins::install_community_plugin,
            updates::update_offer,
            updates::install_update,
            updates::remind_update_later,
        ])
        .events(tauri_specta::collect_events![
            events::StateChanged,
            events::KeyChanged,
            events::KeyImageChanged,
            events::PluginToast,
            events::HistoryShortcut,
            events::UpdateOffered
        ])
}

/// The typed commands plus the raw upload, which takes bytes instead of JSON.
pub fn invoke_handler(
    builder: &tauri_specta::Builder<Wry>,
) -> impl Fn(Invoke<Wry>) -> bool + Send + Sync + 'static {
    let typed = builder.invoke_handler();
    move |invoke| {
        if invoke.message.command() == "put_asset" {
            let raw: &dyn Fn(Invoke<Wry>) -> bool = &tauri::generate_handler![assets::put_asset];
            raw(invoke)
        } else {
            typed(invoke)
        }
    }
}

#[cfg(test)]
mod tests {
    use specta_typescript::Typescript;

    /// Running the tests keeps the UI's copy of the types up to date.
    #[test]
    fn export_typescript_bindings() {
        super::builder()
            .export(
                Typescript::default()
                    .header("// Generated from the Rust types by `cargo test`. Do not edit.\n"),
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../src-nuxt/app/ipc/bindings.ts"
                ),
            )
            .expect("export the TypeScript bindings");
    }

    /// A command missing from `build.rs` or the `engine` permission set is only rejected at run
    /// time, by the UI.
    #[test]
    fn every_command_is_allowed() {
        let manifest = include_str!("../../build.rs");
        let permissions = include_str!("../../permissions/engine.toml");
        let listed = include_str!("bindings.rs")
            .split("collect_commands![")
            .nth(1)
            .and_then(|rest| rest.split(']').next())
            .expect("the list of commands");
        for path in listed.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let name = path.rsplit("::").next().unwrap_or(path);
            assert!(
                manifest.contains(&format!("\"{name}\"")),
                "{name} is missing from build.rs"
            );
            let permission = format!("\"allow-{}\"", name.replace('_', "-"));
            assert!(
                permissions.contains(&permission),
                "{permission} is missing from permissions/engine.toml"
            );
        }
    }
}
