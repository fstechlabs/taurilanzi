//! What plugins ask of the desktop, done with Tauri: messages for the UI, their own windows, file
//! dialogs and the hidden pages that run plugins made of HTML.

use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::webview::{NewWindowFeatures, NewWindowResponse};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
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

/// Pages of plugins open windows of their own with `window.open`, as property inspectors do to
/// pick from long lists, and answer through `window.opener`: those open as the Stream Deck opens
/// them. Any other address goes to the browser.
pub fn new_window_handler(
    app: AppHandle,
) -> impl Fn(tauri::Url, NewWindowFeatures) -> NewWindowResponse<tauri::Wry> + Send + 'static {
    move |url, features| {
        if !plugin_page(&url) {
            if url.scheme() == "https" {
                let _ = app.opener().open_url(url.as_str(), None::<&str>);
            }
            return NewWindowResponse::Deny;
        }
        let label = label("plugin-popup", &uuid::Uuid::new_v4().simple().to_string());
        let built = WebviewWindowBuilder::new(
            &app,
            label,
            WebviewUrl::External("about:blank".parse().expect("a valid address")),
        )
        .title("")
        .inner_size(520.0, 640.0)
        .center()
        .window_features(features)
        .on_document_title_changed(|window, title| {
            let _ = window.set_title(&title);
        })
        .build();
        match built {
            Ok(window) => NewWindowResponse::Create { window },
            Err(err) => {
                log::warn!("could not open the window of a plugin: {err}");
                NewWindowResponse::Deny
            }
        }
    }
}

/// What the plugin host serves: `http://127.0.0.1:<port>/plugins/…`.
fn plugin_page(url: &tauri::Url) -> bool {
    url.scheme() == "http"
        && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        && url.path().starts_with("/plugins/")
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
        .on_new_window(new_window_handler(self.app.clone()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pages_of_the_plugin_host_open_in_a_window_of_their_own() {
        let page = |url: &str| plugin_page(&url.parse().unwrap());
        assert!(page(
            "http://127.0.0.1:3906/plugins/com.a.sdPlugin/picker.html"
        ));
        assert!(page(
            "http://localhost:3906/plugins/com.a.sdPlugin/picker.html?x=1"
        ));
        assert!(!page("http://127.0.0.1:3906/__taurilanzi/log"));
        assert!(!page("https://127.0.0.1:3906/plugins/a.html"));
        assert!(!page("http://evil.example/plugins/a.html"));
        assert!(!page("https://github.com/a/b"));
    }
}
