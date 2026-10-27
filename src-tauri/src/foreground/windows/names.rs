use std::ffi::c_void;
use std::path::Path;

use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::core::HSTRING;

use crate::os::windows::to_wide;

/// The name a program gives itself in its version information, as the Task Manager shows it.
pub fn display_name(program: &Path) -> Option<String> {
    let path = HSTRING::from(program.as_os_str());
    // SAFETY: the buffer is as large as the system asked, and outlives every query into it.
    unsafe {
        let size = GetFileVersionInfoSizeW(&path, None);
        if size == 0 {
            return None;
        }
        let mut info = vec![0u8; size as usize];
        GetFileVersionInfoW(&path, None, size, info.as_mut_ptr().cast()).ok()?;
        let mut translation: *mut c_void = std::ptr::null_mut();
        let mut length = 0u32;
        let query = to_wide("\\VarFileInfo\\Translation");
        let languages: Vec<(u16, u16)> = if VerQueryValueW(
            info.as_ptr().cast(),
            windows::core::PCWSTR(query.as_ptr()),
            &mut translation,
            &mut length,
        )
        .as_bool()
            && length >= 4
        {
            let pairs = std::slice::from_raw_parts(translation as *const u16, length as usize / 2);
            pairs.chunks(2).map(|pair| (pair[0], pair[1])).collect()
        } else {
            Vec::new()
        };
        // US English in Unicode, which most programs carry when they declare nothing.
        for (language, code_page) in languages.into_iter().chain([(0x0409, 0x04B0)]) {
            let query = to_wide(format!(
                "\\StringFileInfo\\{language:04x}{code_page:04x}\\FileDescription"
            ));
            let mut value: *mut c_void = std::ptr::null_mut();
            let mut length = 0u32;
            if VerQueryValueW(
                info.as_ptr().cast(),
                windows::core::PCWSTR(query.as_ptr()),
                &mut value,
                &mut length,
            )
            .as_bool()
                && length > 1
            {
                let units = std::slice::from_raw_parts(value as *const u16, length as usize);
                let name = crate::os::windows::from_wide(units)
                    .to_string_lossy()
                    .trim()
                    .to_owned();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
        None
    }
}
