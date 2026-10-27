//! A desktop file in the autostart folder: the desktop starts the app at login.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn register(label: &str, name: &str, app: &Path, background_arg: &str) -> io::Result<()> {
    let path = folder()?.join(format!("{label}.desktop"));
    let content = entry(name, app, background_arg);
    if fs::read_to_string(&path).is_ok_and(|current| current == content) {
        return Ok(());
    }
    fs::create_dir_all(path.parent().expect("autostart folder"))?;
    fs::write(&path, content)
}

fn folder() -> io::Result<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|config| config.join("autostart"))
        .ok_or_else(|| io::Error::other("HOME is not set"))
}

fn entry(name: &str, app: &Path, background_arg: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Exec={app} {background_arg}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        name = name.replace('\n', " "),
        app = quote(&app.to_string_lossy()),
    )
}

/// A quoted argument of an `Exec` line: quotes, backquotes, dollars and backslashes escaped with
/// a backslash, itself written twice as every backslash in the file; percents doubled.
fn quote(argument: &str) -> String {
    let mut quoted = String::from("\"");
    for c in argument.chars() {
        match c {
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(c);
            }
            '\\' => quoted.push_str("\\\\\\\\"),
            '%' => quoted.push_str("%%"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_quotes_the_path_and_passes_the_argument() {
        let text = entry(
            "App",
            Path::new("/home/a/My Apps/app.AppImage"),
            "--background",
        );
        assert!(text.contains("Exec=\"/home/a/My Apps/app.AppImage\" --background\n"));
        assert!(text.contains("Name=App\n"));
        assert_eq!(quote("a$b%"), "\"a\\\\$b%%\"");
    }
}
