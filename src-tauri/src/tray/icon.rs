use std::sync::Arc;

use tauri::image::Image;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tokio::sync::broadcast::error::RecvError;

use super::{actions, menu, model::Model};
use crate::engine::{EngineEvent, EngineHandle};
use crate::state::AppState;

const TRAY: &str = "main";
/// Black and transparent, so macOS can draw it in the menu bar's own colors. Other systems show
/// tray icons as they are, on light or dark bars: there the app's own icon.
#[cfg(target_os = "macos")]
const ICON: &[u8] = include_bytes!("../../icons/tray.png");
#[cfg(not(target_os = "macos"))]
const ICON: &[u8] = include_bytes!("../../icons/128x128.png");

/// Adds the menu bar icon and keeps its menu in step with the engine.
pub fn create(app: &AppHandle, engine: &EngineHandle) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    let model = Model::new(None, &state.placement, state.updates.status());
    TrayIconBuilder::with_id(TRAY)
        .icon(Image::from_bytes(ICON)?)
        .icon_as_template(true)
        .tooltip(&app.package_info().name)
        .menu(&menu::build(app, &model)?)
        // On Windows a click opens the app and the menu is on the right button, as there.
        .show_menu_on_left_click(cfg!(not(target_os = "windows")))
        .on_menu_event(|app, event| actions::on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if cfg!(target_os = "windows")
                && let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
            {
                crate::window::show(tray.app_handle());
            }
        })
        .build(app)?;
    *state.menu.lock().expect("menu") = Some(model);

    let app = app.clone();
    let engine = engine.clone();
    tauri::async_runtime::spawn(async move {
        let mut events = engine.subscribe();
        if let Ok(snapshot) = engine.snapshot().await {
            on_state(&app, Arc::new(snapshot));
        }
        loop {
            match events.recv().await {
                Ok(EngineEvent::State(snapshot)) => on_state(&app, snapshot),
                Ok(EngineEvent::Key { .. } | EngineEvent::KeyImage { .. })
                | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
        }
    });
    Ok(())
}

fn on_state(app: &AppHandle, snapshot: Arc<crate::engine::Snapshot>) {
    *app.state::<AppState>().snapshot.lock().expect("snapshot") = Some(snapshot);
    refresh(app);
}

/// Rebuilds the menu if what it shows changed. Rebuilding an open menu closes it under the
/// pointer, so it must not happen otherwise.
pub fn refresh(app: &AppHandle) {
    let state = app.state::<AppState>();
    let model = Model::new(
        state.snapshot().as_deref(),
        &state.placement,
        state.updates.status(),
    );
    let new_language = {
        let mut shown = state.menu.lock().expect("menu");
        if shown.as_ref() == Some(&model) {
            return;
        }
        let language = |m: Option<&Model>| m.and_then(|m| m.dock.as_ref()).map(|d| d.language);
        let changed = language(shown.as_ref()) != language(Some(&model));
        *shown = Some(model.clone());
        changed
    };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.tray_by_id(TRAY) else {
            return;
        };
        #[cfg(not(target_os = "macos"))]
        let _ = new_language;
        #[cfg(target_os = "macos")]
        if new_language {
            match crate::app_menu::build(&handle) {
                Ok(menu) => {
                    let _ = handle.set_menu(menu);
                }
                Err(err) => log::warn!("app menu could not be built: {err}"),
            }
        }
        match menu::build(&handle, &model) {
            Ok(menu) => {
                let _ = tray.set_menu(Some(menu));
            }
            Err(err) => log::warn!("menu bar menu could not be built: {err}"),
        }
        let _ = tray.set_tooltip(Some(format!(
            "{} · {}",
            handle.package_info().name,
            model.status_line()
        )));
    });
}
