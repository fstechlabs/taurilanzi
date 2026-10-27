//! GNOME on Wayland tells no app which window has the focus. A small extension of the app's
//! answers over D-Bus; GNOME loads new extensions only at login, so the first time it waits for
//! the next one.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

use super::apps;
use super::backend::Front;
use crate::foreground::AppDetection;
use crate::os::linux::session_bus;

const UUID: &str = "taurilanzi@taurilanzi.com";
const NAME: &str = "com.taurilanzi.Shell";
const PATH: &str = "/com/taurilanzi/Shell";
const METADATA: &str = include_str!("gnome-shell/metadata.json");
const MODULE: &str = include_str!("gnome-shell/extension.js");
const LEGACY: &str = include_str!("gnome-shell/extension-legacy.js");
/// The first version that loads extensions as modules.
const MODULES_SINCE: u32 = 45;
const LEGACY_VERSIONS: &str = "\"42\", \"43\", \"44\"";
const MODULE_VERSIONS: &str = "\"45\", \"46\", \"47\", \"48\", \"49\", \"50\"";

pub fn frontmost() -> Option<Front> {
    let bus = session_bus()?;
    let reply = bus
        .call_method(Some(NAME), PATH, Some(NAME), "FocusedApp", &())
        .ok()?;
    let (id, pid): (String, u32) = reply.body().deserialize().ok()?;
    // Windows GNOME matches to no app get a made-up id.
    let id = if id.starts_with("window:") { "" } else { &id };
    Some(Front {
        pid,
        app: apps::named(id).or_else(|| (pid != 0).then(|| apps::of_process(pid)).flatten()),
    })
}

pub fn detection() -> AppDetection {
    if frontmost().is_some() {
        AppDetection::Ready
    } else if installed() && enabled() {
        AppDetection::NeedsRelogin
    } else {
        AppDetection::NeedsExtension
    }
}

/// Copies the extension and turns it on for the next login.
pub fn install() -> io::Result<()> {
    write_files()?;
    let mut extensions = enabled_extensions();
    if !extensions.iter().any(|e| e == UUID) {
        extensions.push(UUID.to_owned());
        gsettings_set("enabled-extensions", &as_variant(&extensions))?;
    }
    if gsettings_get("disable-user-extensions").as_deref() == Some("true") {
        gsettings_set("disable-user-extensions", "false")?;
    }
    Ok(())
}

/// Brings an extension installed by an older build up to date, for the next login.
pub fn refresh() {
    if installed()
        && let Err(err) = write_files()
    {
        log::warn!("could not update the GNOME extension: {err}");
    }
}

fn folder() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(data.join("gnome-shell/extensions").join(UUID))
}

fn installed() -> bool {
    folder().is_some_and(|f| f.join("metadata.json").is_file())
}

fn enabled() -> bool {
    enabled_extensions().iter().any(|e| e == UUID)
}

fn write_files() -> io::Result<()> {
    let folder = folder().ok_or_else(|| io::Error::other("HOME is not set"))?;
    let modules = shell_version().is_none_or(|v| v >= MODULES_SINCE);
    let (script, versions) = if modules {
        (MODULE, MODULE_VERSIONS)
    } else {
        (LEGACY, LEGACY_VERSIONS)
    };
    fs::create_dir_all(&folder)?;
    write_if_changed(&folder.join("extension.js"), script)?;
    write_if_changed(
        &folder.join("metadata.json"),
        &METADATA.replace("@VERSIONS@", versions),
    )
}

fn write_if_changed(path: &std::path::Path, content: &str) -> io::Result<()> {
    if fs::read_to_string(path).is_ok_and(|current| current == content) {
        return Ok(());
    }
    fs::write(path, content)
}

/// `GNOME Shell 46.0` gives 46.
fn shell_version() -> Option<u32> {
    let output = Command::new("gnome-shell").arg("--version").output().ok()?;
    parse_shell_version(&String::from_utf8_lossy(&output.stdout))
}

fn parse_shell_version(text: &str) -> Option<u32> {
    text.split_whitespace()
        .find_map(|word| word.split('.').next()?.parse().ok())
}

fn enabled_extensions() -> Vec<String> {
    gsettings_get("enabled-extensions")
        .map(|value| parse_list(&value))
        .unwrap_or_default()
}

fn gsettings_get(key: &str) -> Option<String> {
    let output = Command::new("gsettings")
        .args(["get", "org.gnome.shell", key])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn gsettings_set(key: &str, value: &str) -> io::Result<()> {
    let status = Command::new("gsettings")
        .args(["set", "org.gnome.shell", key, value])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("gsettings exited with {status}")))
    }
}

/// A list of strings as `gsettings` prints it: `['a@b', 'c@d']`, or `@as []` when empty.
fn parse_list(value: &str) -> Vec<String> {
    let value = value.trim().trim_start_matches("@as").trim();
    let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
        return Vec::new();
    };
    inner
        .split(',')
        .map(|item| item.trim().trim_matches(['\'', '"']).to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

fn as_variant(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("'{item}'")).collect();
    format!("[{}]", quoted.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_gsettings_lists() {
        assert_eq!(
            parse_list("['ding@rastersoft.com', 'ubuntu-dock@ubuntu.com']"),
            vec!["ding@rastersoft.com", "ubuntu-dock@ubuntu.com"]
        );
        assert!(parse_list("@as []").is_empty());
        assert_eq!(as_variant(&["a@b".into(), "c@d".into()]), "['a@b', 'c@d']");
    }

    #[test]
    fn reads_the_shell_version() {
        assert_eq!(parse_shell_version("GNOME Shell 46.0\n"), Some(46));
        assert_eq!(parse_shell_version("GNOME Shell 42.9"), Some(42));
        assert_eq!(parse_shell_version(""), None);
    }

    #[test]
    fn the_metadata_is_valid_json_for_both_kinds() {
        for versions in [LEGACY_VERSIONS, MODULE_VERSIONS] {
            let metadata: serde_json::Value =
                serde_json::from_str(&METADATA.replace("@VERSIONS@", versions)).unwrap();
            assert_eq!(metadata["uuid"], UUID);
        }
    }
}
