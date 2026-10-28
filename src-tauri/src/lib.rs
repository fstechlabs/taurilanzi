#[macro_use]
mod i18n;
mod actions;
#[cfg(target_os = "macos")]
mod app_menu;
mod clients;
mod cloud;
mod config;
mod driver;
mod engine;
mod foreground;
mod hass;
mod ipc;
mod login;
mod monitor;
mod obs;
mod os;
mod plugin_desktop;
mod plugins;
mod render;
mod screensaver;
mod secrets;
mod session;
mod setup;
mod state;
mod tray;
mod updates;
mod version;
mod window;

use login::BACKGROUND_ARG;
use tauri::{Manager, RunEvent, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let background = std::env::args().any(|arg| arg == BACKGROUND_ARG);
    let bindings = ipc::builder();
    let mut builder = tauri::Builder::default();
    // Tests run with their own data folder next to the user's app, so they skip this.
    if std::env::var_os(setup::DATA_DIR_ENV).is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
            window::show(app);
        }));
    }
    builder = builder.plugin(
        tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
    );
    // Tests start without taking the focus from the app the user is in.
    #[cfg(all(feature = "e2e", target_os = "macos"))]
    {
        builder = builder.activate_ignoring_other_apps(false);
    }
    // After the log plugin, which must be the one to install the logger.
    #[cfg(feature = "e2e")]
    {
        builder = builder
            .plugin(tauri_plugin_wdio::init())
            .plugin(tauri_plugin_wdio_webdriver::init());
    }
    builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build());
    // Windows and Linux keep menus inside windows, and this app's window has none of its own.
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .menu(app_menu::build)
            .on_menu_event(|app, event| app_menu::on_menu(app, event.id().as_ref()));
    }
    let app = builder
        .invoke_handler(ipc::invoke_handler(&bindings))
        .register_asynchronous_uri_scheme_protocol(ipc::SCHEME, ipc::serve_images)
        .setup(move |app| setup::setup(app, &bindings, background))
        .on_window_event(|window, event| {
            // Closing the window keeps the app in the menu bar; "Salir" quits it.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                window::hide(window.app_handle());
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        // What changed in the last minutes reaches the other computers before this one is off.
        if let RunEvent::Exit = event
            && let Some(cloud) = app.try_state::<cloud::Cloud>()
        {
            cloud.flush(std::time::Duration::from_secs(5));
        }
        if let RunEvent::Exit = event
            && let Some(plugins) = app.try_state::<Option<std::sync::Arc<plugins::Plugins>>>()
            && let Some(plugins) = plugins.inner()
        {
            plugins.stop_all();
        }
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen { .. } = event {
            window::show(app);
        }
        let _ = (app, event);
    });
}
