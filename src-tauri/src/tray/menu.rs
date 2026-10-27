use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Wry};

use super::model::Model;
use crate::updates::Status;

pub const OPEN: &str = "open";
pub const PAUSE: &str = "pause";
pub const UPDATES: &str = "updates";
pub const QUIT: &str = "quit";
pub const PROFILE_PREFIX: &str = "profile:";
pub const BRIGHTNESS_PREFIX: &str = "brightness:";

const BRIGHTNESS_LEVELS: [u8; 5] = [100, 75, 50, 25, 10];

pub fn build(app: &AppHandle, model: &Model) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::new(
        app,
        model.status_line(),
        false,
        None::<&str>,
    )?)?;
    if model.translocated {
        menu.append(&MenuItem::new(
            app,
            tr!("tray.moveToApplications"),
            false,
            None::<&str>,
        )?)?;
    }
    menu.append(&MenuItem::with_id(
        app,
        OPEN,
        trf!("tray.open", name = app.package_info().name),
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    if let Some(dock) = &model.dock {
        let profiles = Submenu::new(app, tr!("tray.profile"), true)?;
        for profile in &dock.profiles {
            let active = profile.id == dock.active_profile;
            let label = match &dock.chosen_by {
                Some(app) if active => trf!("tray.chosenBy", profile = profile.name, app = app),
                _ => profile.name.clone(),
            };
            profiles.append(&CheckMenuItem::with_id(
                app,
                format!("{PROFILE_PREFIX}{}", profile.id),
                label,
                true,
                active,
                None::<&str>,
            )?)?;
        }
        menu.append(&profiles)?;

        let brightness = Submenu::new(app, tr!("tray.brightness"), true)?;
        if !BRIGHTNESS_LEVELS.contains(&dock.settings.brightness) {
            brightness.append(&MenuItem::new(
                app,
                trf!("tray.brightnessNow", percent = dock.settings.brightness),
                false,
                None::<&str>,
            )?)?;
        }
        for level in BRIGHTNESS_LEVELS {
            brightness.append(&CheckMenuItem::with_id(
                app,
                format!("{BRIGHTNESS_PREFIX}{level}"),
                format!("{level} %"),
                true,
                level == dock.settings.brightness,
                None::<&str>,
            )?)?;
        }
        menu.append(&brightness)?;
        menu.append(&CheckMenuItem::with_id(
            app,
            PAUSE,
            tr!("tray.pause"),
            true,
            dock.paused,
            None::<&str>,
        )?)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    let (label, enabled) = updates_item(model);
    menu.append(&MenuItem::with_id(
        app,
        UPDATES,
        label,
        enabled,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        QUIT,
        tr!("tray.quit"),
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

fn updates_item(model: &Model) -> (String, bool) {
    if !model.managed {
        return (tr!("tray.updates.onlyInstalled").into(), false);
    }
    match &model.update {
        Status::Idle => (tr!("tray.updates.check").into(), true),
        Status::Checking => (tr!("tray.updates.checking").into(), false),
        Status::UpToDate => (tr!("tray.updates.upToDate").into(), true),
        Status::NotConfigured => (tr!("tray.updates.notConfigured").into(), false),
        Status::Downloading(version) => {
            (trf!("tray.updates.downloading", version = version), false)
        }
        Status::Ready(version) => (trf!("tray.updates.ready", version = version), true),
        Status::Installing(_) => (tr!("tray.updates.installing").into(), false),
        Status::Failed => (tr!("tray.updates.failed").into(), true),
    }
}
