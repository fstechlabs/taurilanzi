use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{HSTRING, Interface};

use super::{apartment, from_wide};

/// The program or file a `.lnk` shortcut points at, as the Start menu keeps them.
pub fn shortcut_target(path: &Path) -> Option<PathBuf> {
    let is_link = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"));
    if !is_link {
        return None;
    }
    apartment();
    // SAFETY: COM is initialised on this thread, and the buffer outlives the call that fills it.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        link.cast::<IPersistFile>()
            .ok()?
            .Load(&HSTRING::from(path.as_os_str()), STGM_READ)
            .ok()?;
        let mut buffer = [0u16; 1024];
        link.GetPath(&mut buffer, std::ptr::null_mut(), 0).ok()?;
        let target = PathBuf::from(from_wide(&buffer));
        (!target.as_os_str().is_empty()).then_some(target)
    }
}
