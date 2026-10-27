use tauri::{AppHandle, Manager, WebviewWindowBuilder};

const MAIN: &str = "main";

/// Created by hand, once the engine is running, so the UI never loads before it can answer.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN)
        .expect("main window in tauri.conf.json");
    let builder = WebviewWindowBuilder::from_config(app, config)?.title(&app.package_info().name);
    // Test windows stay behind whatever the user is working on, never taking the focus, and keep
    // running there: macOS would otherwise freeze a covered window and the tests with it. They
    // open at the smallest size the app allows, so every screen, CI's included, gives the same
    // layout.
    #[cfg(feature = "e2e")]
    let builder = builder
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
        .focused(false)
        .always_on_bottom(true)
        .inner_size(
            config.min_width.unwrap_or(config.width),
            config.min_height.unwrap_or(config.height),
        );
    let _window = builder.build()?;
    #[cfg(all(feature = "e2e", target_os = "macos"))]
    keep_drawing_when_covered(&_window);
    Ok(())
}

/// Background throttling alone still stops animation frames and timers in a covered or hidden
/// window. This private WebKit switch turns that detection off.
#[cfg(target_os = "macos")]
pub fn keep_drawing_when_covered(window: &tauri::WebviewWindow) {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{msg_send, sel};
    let _ = window.with_webview(|webview| {
        let view = webview.inner().cast::<AnyObject>();
        // SAFETY: wry hands over a live WKWebView, and the selector is checked before use.
        unsafe {
            let view = &*view;
            let responds: bool =
                msg_send![view, respondsToSelector: sel!(_setWindowOcclusionDetectionEnabled:)];
            if responds {
                let _: () = msg_send![view, _setWindowOcclusionDetectionEnabled: Bool::NO];
            }
        }
    });
}

/// The app only shows in the Dock and the app switcher while its window is open.
pub fn show(app: &AppHandle) {
    // Test builds show their window without bringing the app forward.
    let forward = !cfg!(feature = "e2e");
    #[cfg(target_os = "macos")]
    if forward {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    }
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        if forward {
            let _ = window.set_focus();
        }
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}
