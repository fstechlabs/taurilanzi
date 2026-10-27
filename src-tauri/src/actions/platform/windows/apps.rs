use std::path::Path;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SW_SHOWNORMAL, WM_CLOSE};
use windows::core::{HSTRING, PCWSTR, w};

use crate::actions::error::ActionError;
use crate::os::windows::{apartment, app_windows, exe_path, same_path, shortcut_target};

/// Opens an app, a file, a folder or a URL, as a double click in Explorer would.
pub fn open(target: &str) -> Result<(), ActionError> {
    apartment();
    // SAFETY: the strings outlive the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(target),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values up to 32 are error codes.
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(ActionError::Launch(std::io::Error::other(format!(
            "ShellExecute failed with {}",
            result.0 as isize
        ))))
    }
}

/// Asks every window of the app to close, as its close button would; unsaved work is still
/// offered to be saved. `app` is its program or a shortcut to it.
pub fn close_app(app: &str) -> Result<(), ActionError> {
    let path = Path::new(app);
    if !path.exists() {
        return Err(ActionError::NotFound(app.into()));
    }
    let program = shortcut_target(path).unwrap_or_else(|| path.to_path_buf());
    let is_program = program
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
    if !is_program {
        return Err(ActionError::NotAnApp(app.into()));
    }
    for window in app_windows() {
        if exe_path(window.pid).is_some_and(|exe| same_path(&exe, &program)) {
            // SAFETY: a window handle the system just listed; posting never blocks.
            let _ = unsafe { PostMessageW(Some(window.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
    }
    Ok(())
}
