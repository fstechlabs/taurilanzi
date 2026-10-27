//! Windows has no brightness keys to press: the built-in screen is set through WMI and external
//! ones over DDC/CI, each where it works.

use std::process::{Command, Stdio};

use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR, SetMonitorBrightness,
};
use windows::Win32::Foundation::{LPARAM, RECT, TRUE};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use windows::core::BOOL;

use crate::actions::error::ActionError;

/// Percent per press, as a brightness key on a laptop.
const STEP: i32 = 10;

pub fn step(up: bool) -> Result<(), ActionError> {
    let delta = if up { STEP } else { -STEP };
    let internal = internal_screen(delta);
    let external = external_screens(delta);
    if internal || external {
        Ok(())
    } else {
        Err(ActionError::NoBrightness)
    }
}

/// The built-in screen of a laptop, through its WMI provider.
fn internal_screen(delta: i32) -> bool {
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $now = (Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness | Select-Object -First 1).CurrentBrightness; \
         $level = [Math]::Max(0, [Math]::Min(100, $now + ({delta}))); \
         Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods | Select-Object -First 1 | \
         Invoke-CimMethod -MethodName WmiSetBrightness -Arguments @{{ Timeout = 0; Brightness = [byte]$level }} | Out-Null"
    );
    let powershell = std::env::var_os("SystemRoot")
        .map(|root| {
            std::path::PathBuf::from(root).join("System32\\WindowsPowerShell\\v1.0\\powershell.exe")
        })
        .unwrap_or_else(|| "powershell.exe".into());
    crate::os::windows::without_console(&mut Command::new(powershell))
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Screens that take brightness over the cable; many do, some only after turning it on in their
/// own menu.
fn external_screens(delta: i32) -> bool {
    let mut monitors: Vec<HMONITOR> = Vec::new();
    // SAFETY: `monitors` outlives the enumeration, which is synchronous.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut monitors as *mut Vec<HMONITOR> as isize),
        );
    }
    let mut changed = false;
    for monitor in monitors {
        let mut count = 0u32;
        // SAFETY: `count` outlives the call.
        if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, &mut count) }.is_err()
            || count == 0
        {
            continue;
        }
        let mut physical = vec![PHYSICAL_MONITOR::default(); count as usize];
        // SAFETY: the slice has room for `count` monitors; they are destroyed below.
        if unsafe { GetPhysicalMonitorsFromHMONITOR(monitor, &mut physical) }.is_err() {
            continue;
        }
        for screen in &physical {
            let (mut min, mut now, mut max) = (0u32, 0u32, 0u32);
            // SAFETY: a handle just returned by the system, and outputs that outlive the calls.
            unsafe {
                if GetMonitorBrightness(screen.hPhysicalMonitor, &mut min, &mut now, &mut max) == 0
                    || max <= min
                {
                    continue;
                }
                let range = (max - min) as i32;
                let level = (now as i32 + delta * range / 100).clamp(min as i32, max as i32);
                if SetMonitorBrightness(screen.hPhysicalMonitor, level as u32) != 0 {
                    changed = true;
                }
            }
        }
        // SAFETY: the monitors were returned by GetPhysicalMonitorsFromHMONITOR above.
        let _ = unsafe { DestroyPhysicalMonitors(&physical) };
    }
    changed
}

unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, found: LPARAM) -> BOOL {
    // SAFETY: the caller passes a pointer to a live Vec<HMONITOR>.
    let found = unsafe { &mut *(found.0 as *mut Vec<HMONITOR>) };
    found.push(monitor);
    TRUE
}
