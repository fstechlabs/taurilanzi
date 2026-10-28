/// The app's own commands, so capabilities can decide which windows may call them.
const COMMANDS: &[&str] = &[
    "get_state",
    "set_key",
    "clear_key",
    "move_key",
    "create_page",
    "update_page",
    "duplicate_page",
    "delete_page",
    "set_home_page",
    "show_page",
    "set_device",
    "set_preferences",
    "try_screensaver",
    "set_paused",
    "set_profile",
    "fonts",
    "put_asset",
    "press_key",
    "accessibility_granted",
    "request_accessibility",
    "audio_outputs",
    "undo",
    "redo",
    "create_profile",
    "rename_profile",
    "duplicate_profile",
    "delete_profile",
    "move_profile",
    "link_app",
    "unlink_app",
    "app_at_path",
    "running_apps",
    "app_detection",
    "set_up_app_detection",
    "save_integration",
    "delete_integration",
    "obs_connect",
    "obs_set_password",
    "obs_names",
    "hass_connect",
    "hass_set_token",
    "hass_catalog",
    "hass_set_code",
    "hass_code_saved",
    "try_http",
    "plugin_catalog",
    "install_plugin",
    "uninstall_plugin",
    "reload_plugins",
    "restart_plugin",
    "show_plugins_folder",
    "community_plugins",
    "install_community_plugin",
    "update_offer",
    "install_update",
    "remind_update_later",
    "set_profile_local",
    "cloud_status",
    "cloud_sign_in",
    "cloud_open_web",
    "cloud_cancel_sign_in",
    "cloud_sign_out",
    "cloud_sync_now",
    "cloud_set_paused",
    "cloud_join",
    "cloud_bring_back",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build");
    embed_windows_manifest();
}

/// The app needs version 6 of the common controls, which only a manifest asks for. tauri-build
/// puts it in the app's program alone, so the test programs would not even start
/// (STATUS_ENTRYPOINT_NOT_FOUND); the linker puts it in every program instead.
fn embed_windows_manifest() {
    let target = |name: &str| std::env::var(name).unwrap_or_default();
    if target("CARGO_CFG_TARGET_OS") != "windows" || target("CARGO_CFG_TARGET_ENV") != "msvc" {
        return;
    }
    let manifest = std::path::Path::new(&target("CARGO_MANIFEST_DIR"))
        .join("packaging")
        .join("windows")
        .join("app.manifest");
    println!("cargo:rerun-if-changed=packaging/windows/app.manifest");
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
