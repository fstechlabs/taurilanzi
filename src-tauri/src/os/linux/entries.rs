//! The desktop files apps install (`org.gnome.Nautilus.desktop`): their name, icon and program,
//! as the desktop's launcher shows them.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Apps are seldom installed while the app runs; reading the folders again this often is enough.
const FRESH_FOR: Duration = Duration::from_secs(30);

static CACHE: Mutex<Option<(Instant, Vec<Entry>)>> = Mutex::new(None);

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The file's name without `.desktop`, which desktops use to name the app.
    pub id: String,
    pub path: PathBuf,
    pub name: String,
    pub icon: Option<String>,
    exec: Option<String>,
    wm_class: Option<String>,
    flatpak: Option<String>,
    /// Not shown in launchers: helpers, settings panels.
    pub hidden: bool,
}

impl Entry {
    /// The program it runs, unless it runs through a sandbox (Flatpak), whose program is hidden.
    pub fn program(&self) -> Option<PathBuf> {
        if self.flatpak.is_some() {
            return None;
        }
        let words = split_exec(self.exec.as_deref()?);
        let mut words = words.iter().map(String::as_str);
        let mut first = words.next()?;
        if first == "env" || first.ends_with("/env") {
            first = words.find(|word| !word.contains('=') && !word.starts_with('-'))?;
        }
        resolve(first)
    }

    /// Whether the desktop names a window or a unit of this app with `id`.
    fn is_called(&self, id: &str) -> bool {
        self.id.eq_ignore_ascii_case(id)
            || self
                .wm_class
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case(id))
            || self
                .flatpak
                .as_deref()
                .is_some_and(|f| f.eq_ignore_ascii_case(id))
    }
}

/// `$XDG_DATA_HOME` then `$XDG_DATA_DIRS`, without an AppImage's own folders.
pub fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    let system = std::env::var_os("XDG_DATA_DIRS")
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let bundle = std::env::var_os("APPDIR").map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = home.into_iter().collect();
    dirs.extend(std::env::split_paths(&system));
    let mut seen = HashSet::new();
    dirs.retain(|dir| {
        !dir.as_os_str().is_empty()
            && !bundle.as_ref().is_some_and(|b| dir.starts_with(b))
            && seen.insert(dir.clone())
    });
    dirs
}

/// Every app installed, each id once: the user's own files hide the system's.
pub fn entries() -> Vec<Entry> {
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((read_at, entries)) = cache.as_ref()
        && read_at.elapsed() < FRESH_FOR
    {
        return entries.clone();
    }
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for dir in data_dirs() {
        let applications = dir.join("applications");
        let mut files = Vec::new();
        collect(&applications, &mut files);
        for path in files {
            let Some(id) = file_id(&applications, &path) else {
                continue;
            };
            if seen.insert(id.clone())
                && let Some(entry) = parse(&id, &path)
            {
                entries.push(entry);
            }
        }
    }
    *cache = Some((Instant::now(), entries.clone()));
    entries
}

/// The installed app with this desktop file id.
pub fn entry(id: &str) -> Option<Entry> {
    let id = id.strip_suffix(".desktop").unwrap_or(id);
    entries().into_iter().find(|entry| entry.id == id)
}

/// The app a desktop calls `id`, which may be the desktop file id, the window class or the
/// sandbox id, in any case.
pub fn find_entry(id: &str) -> Option<Entry> {
    let id = id.strip_suffix(".desktop").unwrap_or(id);
    let entries = entries();
    entries
        .iter()
        .find(|entry| entry.id == id)
        .or_else(|| entries.iter().find(|entry| entry.is_called(id)))
        .or_else(|| {
            entries.iter().find(|entry| {
                entry
                    .id
                    .rsplit('.')
                    .next()
                    .is_some_and(|last| last.eq_ignore_ascii_case(id))
            })
        })
        .cloned()
}

/// A desktop file the user chose, wherever it is.
pub fn entry_at(path: &Path) -> Option<Entry> {
    let id = path.file_stem()?.to_str()?.to_owned();
    let known = entry(&id).filter(|entry| entry.path == path);
    known.or_else(|| parse(&id, path))
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for item in read.flatten() {
        let path = item.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "desktop") {
            out.push(path);
        }
    }
}

/// Files in subfolders take the folder in their id: `kde4/dolphin.desktop` is `kde4-dolphin`.
fn file_id(applications: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(applications).ok()?.to_str()?;
    let id = relative.strip_suffix(".desktop")?.replace('/', "-");
    Some(id)
}

fn parse(id: &str, path: &Path) -> Option<Entry> {
    parse_text(id, path, &fs::read_to_string(path).ok()?, &locales())
}

fn parse_text(id: &str, path: &Path, text: &str, locales: &[String]) -> Option<Entry> {
    let mut in_entry = false;
    let mut name: Option<String> = None;
    let mut localized: Option<(usize, String)> = None;
    let mut entry = Entry {
        id: id.to_owned(),
        path: path.to_owned(),
        name: String::new(),
        icon: None,
        exec: None,
        wm_class: None,
        flatpak: None,
        hidden: false,
    };
    let mut application = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), unescape(value.trim()));
        match key {
            "Type" => application = value == "Application",
            "Name" => name = Some(value),
            "Icon" => entry.icon = Some(value).filter(|v| !v.is_empty()),
            "Exec" => entry.exec = Some(value),
            "StartupWMClass" => entry.wm_class = Some(value),
            "X-Flatpak" => entry.flatpak = Some(value),
            "NoDisplay" | "Hidden" if value == "true" => entry.hidden = true,
            _ => {
                if let Some(locale) = key
                    .strip_prefix("Name[")
                    .and_then(|rest| rest.strip_suffix(']'))
                    && let Some(rank) = locales.iter().position(|l| l == locale)
                    && localized.as_ref().is_none_or(|(best, _)| rank < *best)
                {
                    localized = Some((rank, value));
                }
            }
        }
    }
    entry.name = localized.map(|(_, name)| name).or(name)?;
    application.then_some(entry)
}

/// The locales to look for a name in, best first: `es_ES`, then `es`.
fn locales() -> Vec<String> {
    let language = crate::i18n::current().tag().to_owned();
    let mut locales = Vec::new();
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var)
            && !value.is_empty()
        {
            let locale = value
                .split(['.', '@'])
                .next()
                .unwrap_or_default()
                .to_owned();
            if locale.starts_with(&language) {
                locales.push(locale);
            }
            break;
        }
    }
    locales.push(language);
    locales
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The words of an `Exec` line, quotes removed and field codes (`%U`) dropped.
fn split_exec(exec: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            ' ' | '\t' if !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words.retain(|word| !(word.len() == 2 && word.starts_with('%')));
    words
}

fn resolve(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_owned());
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAUTILUS: &str = "[Desktop Entry]\nType=Application\nName=Files\nName[es]=Archivos\n\
        Name[es_ES]=Archivos de España\nIcon=org.gnome.Nautilus\n\
        Exec=nautilus --new-window %U\nStartupWMClass=org.gnome.Nautilus\n\
        [Desktop Action new-window]\nName=New Window\nExec=nautilus --new-window\n";

    #[test]
    fn reads_the_name_in_the_users_language_and_ignores_actions() {
        let path = Path::new("/usr/share/applications/org.gnome.Nautilus.desktop");
        let entry = parse_text(
            "org.gnome.Nautilus",
            path,
            NAUTILUS,
            &["es_ES".into(), "es".into()],
        )
        .unwrap();
        assert_eq!(entry.name, "Archivos de España");
        assert_eq!(entry.icon.as_deref(), Some("org.gnome.Nautilus"));
        assert_eq!(entry.exec.as_deref(), Some("nautilus --new-window %U"));
        assert!(!entry.hidden);
        let entry = parse_text("x", path, NAUTILUS, &["fr".into()]).unwrap();
        assert_eq!(entry.name, "Files");
        assert!(entry.is_called("org.gnome.nautilus"));
    }

    #[test]
    fn skips_files_that_are_not_apps() {
        let link = "[Desktop Entry]\nType=Link\nName=Site\nURL=https://example.com\n";
        assert_eq!(parse_text("site", Path::new("/x"), link, &[]), None);
    }

    #[test]
    fn splits_exec_lines_as_the_specification_says() {
        assert_eq!(
            split_exec(r#""/opt/My App/run" --flag "a \"b\"" %f"#),
            vec!["/opt/My App/run", "--flag", "a \"b\""]
        );
        assert_eq!(split_exec("env A=1 code %F"), vec!["env", "A=1", "code"]);
    }

    #[test]
    fn subfolders_go_into_the_id() {
        let apps = Path::new("/usr/share/applications");
        assert_eq!(
            file_id(apps, &apps.join("kde4/dolphin.desktop")).as_deref(),
            Some("kde4-dolphin")
        );
    }
}
