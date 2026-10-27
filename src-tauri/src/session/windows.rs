use windows::Win32::System::RemoteDesktop::{
    WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTS_SESSIONSTATE_LOCK, WTSFreeMemory,
    WTSINFOEXW, WTSQuerySessionInformationW, WTSSessionInfoEx,
};
use windows::core::PWSTR;

/// The lock screen marks the user's session as locked.
pub fn screen_locked() -> Option<bool> {
    let mut buffer = PWSTR::null();
    let mut size = 0u32;
    // SAFETY: the system allocates the buffer, which is freed below, and checks its size.
    unsafe {
        WTSQuerySessionInformationW(
            Some(WTS_CURRENT_SERVER_HANDLE),
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buffer,
            &mut size,
        )
        .ok()?;
        let locked = (size as usize >= size_of::<WTSINFOEXW>()).then(|| {
            let info = &*(buffer.0 as *const WTSINFOEXW);
            info.Data.WTSInfoExLevel1.SessionFlags == WTS_SESSIONSTATE_LOCK as i32
        });
        WTSFreeMemory(buffer.0.cast());
        locked
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_session() {
        assert!(super::screen_locked().is_some());
    }
}
