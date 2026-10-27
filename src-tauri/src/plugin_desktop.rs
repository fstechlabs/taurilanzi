//! What plugins ask of the desktop, done with Tauri: messages for the UI, their own windows, file
//! dialogs and the hidden pages that run plugins made of HTML.

use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

use crate::ipc::PluginToast;
use crate::plugins::Desktop;

pub struct TauriDesktop {
    app: AppHandle,
}

impl TauriDesktop {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

/// Window labels only take letters, digits and a few signs.
fn label(prefix: &str, id: &str) -> String {
    let clean: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{prefix}-{clean}")
}

fn parse(url: &str) -> Option<tauri::Url> {
    url.parse().ok()
}

impl Desktop for TauriDesktop {
    fn toast(&self, plugin: &str, message: &str) {
        let toast = PluginToast {
            plugin: plugin.to_owned(),
            message: message.to_owned(),
        };
        if let Err(err) = toast.emit(&self.app) {
            log::warn!("could not show a message of {plugin}: {err}");
        }
    }

    fn open_view(&self, url: &str, width: f64, height: f64) {
        let Some(parsed) = parse(url) else {
            return;
        };
        let label = label("plugin-view", &uuid::Uuid::new_v4().simple().to_string());
        let built = WebviewWindowBuilder::new(&self.app, label, WebviewUrl::External(parsed))
            .title("")
            .inner_size(width.max(120.0), height.max(80.0))
            .center()
            .resizable(true)
            .build();
        if let Err(err) = built {
            log::warn!("could not open the window of a plugin: {err}");
        }
    }

    fn select(&self, kind: &str, filter: &str) -> Option<String> {
        let mut dialog = self.app.dialog().file();
        if kind == "folder" {
            return dialog.blocking_pick_folder().map(|p| p.to_string());
        }
        if let Some((name, extensions)) = qt_filter(filter) {
            let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
            dialog = dialog.add_filter(name, &extensions);
        }
        dialog.blocking_pick_file().map(|p| p.to_string())
    }

    fn run_page(&self, plugin: &str, url: &str) {
        let Some(parsed) = parse(url) else {
            return;
        };
        self.stop_page(plugin);
        let built = WebviewWindowBuilder::new(
            &self.app,
            label("plugin", plugin),
            WebviewUrl::External(parsed),
        )
        .title(plugin)
        .visible(false)
        .focused(false)
        .skip_taskbar(true)
        .inner_size(400.0, 300.0)
        .background_throttling(BackgroundThrottlingPolicy::Disabled)
        .build();
        match built {
            #[cfg(target_os = "macos")]
            Ok(window) => crate::window::keep_drawing_when_covered(&window),
            #[cfg(not(target_os = "macos"))]
            Ok(_) => {}
            Err(err) => log::warn!("could not run plugin {plugin}: {err}"),
        }
    }

    fn stop_page(&self, plugin: &str) {
        if let Some(window) = self.app.get_webview_window(&label("plugin", plugin)) {
            let _ = window.destroy();
        }
    }
}

/// A Qt style filter, `image(*.png *.jpg)`, as a name and its extensions.
fn qt_filter(filter: &str) -> Option<(String, Vec<String>)> {
    let (name, rest) = filter.split_once('(')?;
    let extensions: Vec<String> = rest
        .trim_end_matches(')')
        .split_whitespace()
        .filter_map(|p| p.strip_prefix("*."))
        .map(str::to_owned)
        .collect();
    (!extensions.is_empty()).then(|| (name.trim().to_owned(), extensions))
}
