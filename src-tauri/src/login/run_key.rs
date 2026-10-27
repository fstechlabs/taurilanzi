//! A value under the user's `Run` key: Windows starts the app at login.

use std::io;
use std::path::Path;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, REG_VALUE_TYPE, RegCloseKey,
    RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::core::{HSTRING, w};

pub fn register(label: &str, _name: &str, app: &Path, background_arg: &str) -> io::Result<()> {
    let command = format!("\"{}\" {background_arg}", app.display());
    let mut key = HKEY::default();
    // SAFETY: the key is closed below, and every buffer outlives the call that uses it.
    unsafe {
        let opened = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            None,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            &mut key,
        );
        if opened != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(opened.0 as i32));
        }
        let name = HSTRING::from(label);
        let result = if current(key, &name).as_deref() == Some(command.as_str()) {
            Ok(())
        } else {
            let mut data: Vec<u8> = command
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes)
                .collect();
            let written = RegSetValueExW(key, &name, None, REG_SZ, Some(&data));
            data.clear();
            if written == ERROR_SUCCESS {
                Ok(())
            } else {
                Err(io::Error::from_raw_os_error(written.0 as i32))
            }
        };
        let _ = RegCloseKey(key);
        result
    }
}

/// SAFETY: `key` must be open with permission to read values.
unsafe fn current(key: HKEY, name: &HSTRING) -> Option<String> {
    let mut kind = REG_VALUE_TYPE::default();
    let mut buffer = [0u16; 2048];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: the buffer and its size outlive the call.
    let read = unsafe {
        RegQueryValueExW(
            key,
            name,
            None,
            Some(&mut kind),
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if read != ERROR_SUCCESS || kind != REG_SZ {
        return None;
    }
    let units = &buffer[..(size as usize / 2).min(buffer.len())];
    Some(
        crate::os::windows::from_wide(units)
            .to_string_lossy()
            .into_owned(),
    )
}
