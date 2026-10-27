//! Apps by their desktop file id (`org.gnome.Nautilus`), which is how Linux desktops name them.

use std::collections::HashSet;
use std::path::Path;

use crate::config::AppRef;
use crate::os::linux::{self, Entry};

/// Apps with a launcher entry that have a process running, apart from this one, by name.
pub fn running_apps() -> Vec<AppRef> {
    let mut seen = HashSet::new();
    let mut apps: Vec<AppRef> = linux::user_processes()
        .into_iter()
        .filter_map(|process| process.app)
        .filter(|entry| !entry.hidden && seen.insert(entry.id.clone()))
        .map(app_ref)
        .collect();
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

/// The app of a desktop file the user chose, or of a program some desktop file runs.
pub fn app_at(path: &Path) -> Option<AppRef> {
    if path.extension().is_some_and(|ext| ext == "desktop") {
        return linux::entry_at(path).map(app_ref);
    }
    let program = std::fs::canonicalize(path).ok()?;
    let entry = linux::entries().into_iter().find(|entry| {
        entry
            .program()
            .and_then(|p| std::fs::canonicalize(p).ok())
            .is_some_and(|p| p == program)
    });
    Some(match entry {
        Some(entry) => app_ref(entry),
        None => {
            let name = path.file_name()?.to_string_lossy().into_owned();
            AppRef {
                id: name.clone(),
                name,
            }
        }
    })
}

/// The icon of the installed app with this id, as a PNG.
pub fn app_icon(id: &str) -> Option<Vec<u8>> {
    linux::icon_png(&linux::entry(id)?.icon?)
}

pub fn app_ref(entry: Entry) -> AppRef {
    AppRef {
        id: entry.id,
        name: entry.name,
    }
}

/// The app a desktop names `id` (a desktop file id, a window class), or the bare name if no
/// launcher entry has it.
pub fn named(id: &str) -> Option<AppRef> {
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    Some(linux::find_entry(id).map(app_ref).unwrap_or_else(|| {
        let id = id.strip_suffix(".desktop").unwrap_or(id).to_owned();
        AppRef {
            name: id.clone(),
            id,
        }
    }))
}

/// The app of a process, by the unit the desktop started it in or by its program.
pub fn of_process(pid: u32) -> Option<AppRef> {
    linux::process_app(pid).map(app_ref)
}
