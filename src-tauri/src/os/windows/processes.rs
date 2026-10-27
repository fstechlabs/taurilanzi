//! Processes and their windows, as the taskbar sees them.

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, TRUE};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongW,
    GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PWSTR};

use super::from_wide;

/// Store apps run inside this host, which owns their window frame.
const STORE_APP_HOST: &str = "applicationframehost.exe";

pub struct AppWindow {
    pub hwnd: HWND,
    pub pid: u32,
}

/// The program a process runs.
pub fn exe_path(pid: u32) -> Option<PathBuf> {
    // SAFETY: the handle is closed below, and the buffer outlives the call that fills it.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let read = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);
        read.ok()?;
        Some(PathBuf::from(from_wide(&buffer[..size as usize])))
    }
}

/// Windows paths ignore case.
pub fn same_path(a: &Path, b: &Path) -> bool {
    a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
}

/// The windows the taskbar would show, with the process of the app that draws each one.
pub fn app_windows() -> Vec<AppWindow> {
    let mut found: Vec<HWND> = Vec::new();
    // SAFETY: `found` outlives the enumeration, which is synchronous.
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut found as *mut Vec<HWND> as isize));
    }
    found
        .into_iter()
        .filter(|&hwnd| on_taskbar(hwnd))
        .map(|hwnd| AppWindow {
            hwnd,
            pid: app_pid(hwnd),
        })
        .collect()
}

/// The process of the app in a window; for a store app, the app's rather than its host's.
pub fn app_pid(hwnd: HWND) -> u32 {
    let pid = window_pid(hwnd);
    let hosted = exe_path(pid)
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
        })
        .is_some_and(|name| name == STORE_APP_HOST);
    if !hosted {
        return pid;
    }
    let mut children: Vec<HWND> = Vec::new();
    // SAFETY: `children` outlives the enumeration, which is synchronous.
    unsafe {
        let _ = EnumChildWindows(
            Some(hwnd),
            Some(collect),
            LPARAM(&mut children as *mut Vec<HWND> as isize),
        );
    }
    children
        .into_iter()
        .map(window_pid)
        .find(|&child| child != pid)
        .unwrap_or(pid)
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0;
    // SAFETY: `pid` outlives the call.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

fn on_taskbar(hwnd: HWND) -> bool {
    // SAFETY: plain queries about a window handle the system just gave.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || GetWindowTextLengthW(hwnd) == 0 {
            return false;
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.is_invalid()) {
            return false;
        }
        if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }
        let mut cloaked = 0u32;
        let hidden = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0;
        !hidden
    }
}

unsafe extern "system" fn collect(hwnd: HWND, found: LPARAM) -> BOOL {
    // SAFETY: the callers pass a pointer to a live Vec<HWND>.
    let found = unsafe { &mut *(found.0 as *mut Vec<HWND>) };
    found.push(hwnd);
    TRUE
}
