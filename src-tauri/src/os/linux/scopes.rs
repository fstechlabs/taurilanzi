//! Desktops start each app in a systemd unit named after its desktop file
//! (`app-gnome-org.gnome.Nautilus-1234.scope`, `snap.firefox.firefox-….scope`), which every process
//! of the app inherits. It names the app even for sandboxed ones, whose programs are hidden.

use std::fs;

/// The desktop file id the process was started as, if a desktop started it.
pub fn scope_app_id(pid: u32) -> Option<String> {
    let cgroup = fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    cgroup
        .lines()
        .find_map(|line| app_id(line.rsplit('/').next()?))
}

/// `app[-<launcher>]-<id>-<random>.scope`, `app[-<launcher>]-<id>@<random>.service`, with `-` in
/// the id written `\x2d`; or `snap.<snap>.<app>-<random>.scope`.
fn app_id(unit: &str) -> Option<String> {
    let (name, scope) = if let Some(name) = unit.strip_suffix(".scope") {
        (name, true)
    } else {
        (unit.strip_suffix(".service")?, false)
    };
    if let Some(snap) = name.strip_prefix("snap.") {
        let (snap, rest) = snap.split_once('.')?;
        let (app, _) = rest.rsplit_once('-')?;
        return Some(format!("{snap}_{app}"));
    }
    let rest = name.strip_prefix("app-")?;
    let id = match rest.split_once('@') {
        Some((id, _)) => id,
        None if scope => rest.rsplit_once('-')?.0,
        None => rest,
    };
    let id = id.split_once('-').map_or(id, |(_launcher, id)| id);
    let id = id.replace("\\x2d", "-");
    (!id.is_empty()).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_app_of_each_kind_of_unit() {
        assert_eq!(
            app_id("app-gnome-org.gnome.Nautilus-4567.scope").as_deref(),
            Some("org.gnome.Nautilus")
        );
        assert_eq!(
            app_id("app-flatpak-org.gimp.GIMP-12345.scope").as_deref(),
            Some("org.gimp.GIMP")
        );
        assert_eq!(
            app_id("app-gnome-gnome\\x2dterminal-99.scope").as_deref(),
            Some("gnome-terminal")
        );
        assert_eq!(
            app_id("app-org.kde.konsole@a1b2.service").as_deref(),
            Some("org.kde.konsole")
        );
        assert_eq!(
            app_id("snap.firefox.firefox-1f2e.scope").as_deref(),
            Some("firefox_firefox")
        );
        assert_eq!(app_id("session-2.scope"), None);
        assert_eq!(app_id("user@1000.service"), None);
    }
}
