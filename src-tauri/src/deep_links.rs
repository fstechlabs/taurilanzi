//! Links the Stream Deck opens for its plugins, `streamdeck://plugins/message/<plugin>/…`, which
//! plugins hand out to come back to themselves, as after signing in on a website.

use tauri::AppHandle;
use tauri_plugin_deep_link::DeepLinkExt;

use crate::plugins::PluginHost;

pub fn listen(app: &AppHandle, host: PluginHost) {
    // macOS learns the scheme from the bundle; the others are told as the app starts.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    if let Err(err) = app.deep_link().register_all() {
        log::warn!("the links of Stream Deck plugins will not open the app: {err}");
    }
    if let Ok(Some(urls)) = app.deep_link().get_current() {
        for url in urls {
            host.deep_link(url.as_str());
        }
    }
    app.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            host.deep_link(url.as_str());
        }
    });
}
