//! Apps by their program: Windows names the app in front by the window, and the window by the
//! process that draws it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};

use super::names::display_name;
use crate::config::AppRef;
use crate::os::windows::{app_pid, app_windows, exe_path, shortcut_target};

/// Parts of Windows itself that take the front for a moment: the Start menu, search, the lock
/// screen, the taskbar. They must not switch the profile and back.
const SHELL_PROGRAMS: &[&str] = &[
    "startmenuexperiencehost.exe",
    "searchhost.exe",
    "searchapp.exe",
    "shellexperiencehost.exe",
    "lockapp.exe",
    "textinputhost.exe",
    "logonui.exe",
];
/// The windows of Explorer that are the desktop and the taskbar, not a folder.
const SHELL_WINDOWS: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Progman",
    "WorkerW",
];

/// Where each app seen was, by id, to find its icon later.
static PROGRAMS: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);

pub struct Front {
    pub pid: u32,
    pub app: Option<AppRef>,
}

pub fn frontmost() -> Option<Front> {
    // SAFETY: no arguments; the handle may be null, which is checked.
    let window = unsafe { GetForegroundWindow() };
    if window.is_invalid() {
        return None;
    }
    let mut class = [0u16; 64];
    // SAFETY: the buffer outlives the call.
    let length = unsafe { GetClassNameW(window, &mut class) } as usize;
    let class = String::from_utf16_lossy(&class[..length]);
    if SHELL_WINDOWS.contains(&class.as_str()) {
        return None;
    }
    let pid = app_pid(window);
    let program = exe_path(pid)?;
    let app = app_ref(&program)?;
    if SHELL_PROGRAMS.contains(&app.id.as_str()) {
        return None;
    }
    Some(Front {
        pid,
        app: Some(app),
    })
}

/// Apps with a window on the taskbar, apart from this one, by name.
pub fn running_apps() -> Vec<AppRef> {
    let own = std::process::id();
    let mut seen = HashSet::new();
    let mut apps: Vec<AppRef> = app_windows()
        .into_iter()
        .filter(|window| window.pid != own)
        .filter_map(|window| exe_path(window.pid))
        .filter_map(|program| app_ref(&program))
        .filter(|app| !SHELL_PROGRAMS.contains(&app.id.as_str()) && seen.insert(app.id.clone()))
        .collect();
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

/// The app of a program the user chose, or of the program a shortcut opens.
pub fn app_at(path: &Path) -> Option<AppRef> {
    let program = shortcut_target(path).unwrap_or_else(|| path.to_path_buf());
    let is_program = program
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
    if !is_program || !program.is_file() {
        return None;
    }
    app_ref(&program)
}

/// The id is the program's file name, the same wherever each computer installs it.
fn app_ref(program: &Path) -> Option<AppRef> {
    let id = program.file_name()?.to_string_lossy().to_lowercase();
    remember(&id, program);
    Some(AppRef {
        name: display_name(program).unwrap_or_else(|| {
            program
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| id.clone())
        }),
        id,
    })
}

fn remember(id: &str, program: &Path) {
    let mut programs = PROGRAMS.lock().unwrap_or_else(PoisonError::into_inner);
    programs
        .get_or_insert_with(HashMap::new)
        .entry(id.to_owned())
        .or_insert_with(|| program.to_path_buf());
}

/// The program of an app seen before, or of a running one.
pub fn program_of(id: &str) -> Option<PathBuf> {
    let known = PROGRAMS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|programs| programs.get(id).cloned());
    known.or_else(|| {
        app_windows()
            .into_iter()
            .filter_map(|window| exe_path(window.pid))
            .find(|program| {
                program
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(id))
            })
    })
}
