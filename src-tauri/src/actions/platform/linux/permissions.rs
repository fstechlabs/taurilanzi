//! The dock and the virtual keyboard are devices only root may open, until a udev rule gives them
//! to the user sitting at the computer. Packages install the rule; otherwise the app installs it,
//! asking for the password through polkit.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use super::virtual_keyboard::UINPUT;
use crate::actions::error::ActionError;

pub const RULES: &str = include_str!("../../../../packaging/linux/70-taurilanzi.rules");
const INSTALLED: &str = "/etc/udev/rules.d/70-taurilanzi.rules";
const PACKAGED: &str = "/usr/lib/udev/rules.d/70-taurilanzi.rules";

/// Run as root: saves the rule from its input, loads the virtual keyboard module now and at every
/// start, and applies the rule to the devices already plugged in.
const INSTALL: &str = "set -e
cat > /etc/udev/rules.d/70-taurilanzi.rules
echo uinput > /etc/modules-load.d/taurilanzi.conf
modprobe uinput || true
udevadm control --reload-rules
udevadm trigger --action=change --subsystem-match=misc --subsystem-match=hidraw --subsystem-match=usb
udevadm settle || true
";

/// Whether the rule is in place and the virtual keyboard can be opened.
pub fn accessibility_granted() -> bool {
    let rule = Path::new(INSTALLED).exists() || Path::new(PACKAGED).exists();
    rule && OpenOptions::new().write(true).open(UINPUT).is_ok()
}

/// Asks for the administrator password and installs the rule; returns when it is done.
pub fn request_accessibility() {
    if let Err(err) = install() {
        log::warn!("could not install the udev rule: {err}");
    }
}

pub fn open_accessibility_settings() -> Result<(), ActionError> {
    Ok(())
}

fn install() -> io::Result<()> {
    let mut child = Command::new("pkexec")
        .args(["/bin/sh", "-c", INSTALL])
        .stdin(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped above")
        .write_all(RULES.as_bytes())?;
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("pkexec exited with {status}")))
    }
}
