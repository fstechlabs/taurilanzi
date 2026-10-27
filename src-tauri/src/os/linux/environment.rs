//! An AppImage runs the app with its own libraries and data folders first in the environment.
//! Programs started from the app must get the user's environment instead, or they break.

use std::ffi::OsString;
use std::process::Command;

/// Removed outright: they only point inside the AppImage.
const BUNDLE_ONLY: &[&str] = &[
    "APPDIR",
    "APPIMAGE",
    "ARGV0",
    "OWD",
    "GDK_PIXBUF_MODULE_FILE",
    "GDK_PIXBUF_MODULEDIR",
    "GIO_EXTRA_MODULES",
    "GIO_MODULE_DIR",
    "GSETTINGS_SCHEMA_DIR",
    "GST_PLUGIN_PATH",
    "GST_PLUGIN_SCANNER",
    "GST_PLUGIN_SYSTEM_PATH",
    "GST_PLUGIN_SYSTEM_PATH_1_0",
    "GTK_DATA_PREFIX",
    "GTK_EXE_PREFIX",
    "GTK_IM_MODULE_FILE",
    "GTK_PATH",
    "PERLLIB",
    "PYTHONHOME",
    "PYTHONPATH",
    "QT_PLUGIN_PATH",
];
/// Lists of folders where only the AppImage's own entries are removed.
const FOLDER_LISTS: &[&str] = &["PATH", "LD_LIBRARY_PATH", "XDG_DATA_DIRS"];

pub fn outside_bundle(command: &mut Command) -> &mut Command {
    let Some(bundle) = std::env::var_os("APPDIR") else {
        return command;
    };
    for name in BUNDLE_ONLY {
        command.env_remove(name);
    }
    for name in FOLDER_LISTS {
        let Some(value) = std::env::var_os(name) else {
            continue;
        };
        match without_bundle(&value, &bundle) {
            Some(kept) => command.env(name, kept),
            None => command.env_remove(name),
        };
    }
    command
}

fn without_bundle(list: &OsString, bundle: &OsString) -> Option<OsString> {
    let bundle = std::path::Path::new(bundle);
    let kept: Vec<_> = std::env::split_paths(list)
        .filter(|folder| !folder.as_os_str().is_empty() && !folder.starts_with(bundle))
        .collect();
    if kept.is_empty() {
        None
    } else {
        std::env::join_paths(kept).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_folders_outside_the_bundle() {
        let list = OsString::from("/tmp/.mount_x/usr/lib:/usr/lib:/tmp/.mount_x/lib");
        let bundle = OsString::from("/tmp/.mount_x");
        assert_eq!(without_bundle(&list, &bundle), Some("/usr/lib".into()));
        assert_eq!(without_bundle(&"/tmp/.mount_x/bin".into(), &bundle), None);
    }
}
