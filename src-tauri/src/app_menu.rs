//! The menu bar while the window is open. Undo and redo are our own items: the default Edit menu
//! sends them only to text being typed.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Wry};
use tauri_specta::Event;

use crate::ipc::HistoryShortcut;

const UNDO: &str = "edit:undo";
const REDO: &str = "edit:redo";

pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let name = &app.package_info().name;
    let separator = || PredefinedMenuItem::separator(app);
    let main = Submenu::with_items(
        app,
        name,
        true,
        &[
            &PredefinedMenuItem::about(
                app,
                Some(trf!("menu.app.about", name = name).as_str()),
                None,
            )?,
            &separator()?,
            &PredefinedMenuItem::services(app, Some(tr!("menu.app.services")))?,
            &separator()?,
            &PredefinedMenuItem::hide(app, Some(trf!("menu.app.hide", name = name).as_str()))?,
            &PredefinedMenuItem::hide_others(app, Some(tr!("menu.app.hideOthers")))?,
            &PredefinedMenuItem::show_all(app, Some(tr!("menu.app.showAll")))?,
            &separator()?,
            &PredefinedMenuItem::quit(app, Some(trf!("menu.app.quit", name = name).as_str()))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        tr!("menu.edit.title"),
        true,
        &[
            &MenuItem::with_id(app, UNDO, tr!("menu.edit.undo"), true, Some("CmdOrCtrl+Z"))?,
            &MenuItem::with_id(
                app,
                REDO,
                tr!("menu.edit.redo"),
                true,
                Some("CmdOrCtrl+Shift+Z"),
            )?,
            &separator()?,
            &PredefinedMenuItem::cut(app, Some(tr!("menu.edit.cut")))?,
            &PredefinedMenuItem::copy(app, Some(tr!("menu.edit.copy")))?,
            &PredefinedMenuItem::paste(app, Some(tr!("menu.edit.paste")))?,
            &PredefinedMenuItem::select_all(app, Some(tr!("menu.edit.selectAll")))?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        tr!("menu.window.title"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(tr!("menu.window.minimize")))?,
            &PredefinedMenuItem::close_window(app, Some(tr!("menu.window.close")))?,
        ],
    )?;
    Menu::with_items(app, &[&main, &edit, &window])
}

pub fn on_menu(app: &AppHandle, id: &str) {
    if id == UNDO || id == REDO {
        let shortcut = HistoryShortcut { redo: id == REDO };
        if let Err(err) = shortcut.emit(app) {
            log::warn!("could not pass {id} to the window: {err}");
        }
    }
}
