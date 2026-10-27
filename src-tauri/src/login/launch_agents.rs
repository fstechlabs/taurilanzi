//! A user LaunchAgent: launchd starts the app at login and again if it crashes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Seconds launchd waits before starting the app again after a crash.
const RESTART_DELAY: u32 = 5;

/// Not loaded now, which would start a second copy: launchd picks it up at the next login.
pub fn register(label: &str, _name: &str, app: &Path, background_arg: &str) -> io::Result<()> {
    write_if_changed(label, &agent(label, app, background_arg))
}

/// `KeepAlive` only after a failed exit, so quitting from the menu really quits. The restart
/// after an update is a new process of the same job; `AbandonProcessGroup` keeps launchd from
/// killing it when the old one exits.
fn agent(label: &str, app: &Path, background_arg: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\"><dict>\
         <key>Label</key><string>{label}</string>\
         <key>ProgramArguments</key><array><string>{app}</string><string>{background_arg}</string></array>\
         <key>RunAtLoad</key><true/>\
         <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>\
         <key>ThrottleInterval</key><integer>{RESTART_DELAY}</integer>\
         <key>ProcessType</key><string>Interactive</string>\
         <key>AbandonProcessGroup</key><true/>\
         <key>LimitLoadToSessionType</key><string>Aqua</string>\
         </dict></plist>\n",
        label = escape(label),
        app = escape(&app.to_string_lossy()),
        background_arg = escape(background_arg),
    )
}

fn write_if_changed(label: &str, content: &str) -> io::Result<()> {
    let path = plist(label)?;
    if fs::read_to_string(&path).is_ok_and(|current| current == content) {
        return Ok(());
    }
    fs::create_dir_all(path.parent().expect("agents folder"))?;
    fs::write(&path, content)
}

fn plist(label: &str) -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| io::Error::other("HOME is not set"))?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{label}.plist")))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_escapes_the_path_and_keeps_the_arguments_in_order() {
        let xml = agent(
            "com.taurilanzi.test",
            Path::new("/Apps/A & B.app/app"),
            "--background",
        );
        assert!(
            xml.contains("<string>/Apps/A &amp; B.app/app</string><string>--background</string>")
        );
        assert!(xml.contains("<key>SuccessfulExit</key><false/>"));
    }
}
