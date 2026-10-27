//! The user's processes and the installed app each belongs to.

use std::collections::HashMap;
use std::path::PathBuf;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use super::{Entry, entries, find_entry, scope_app_id};

pub struct UserProcess {
    pub pid: u32,
    pub parent: Option<u32>,
    pub app: Option<Entry>,
}

/// Every process of this user, apart from this app's own.
pub fn user_processes() -> Vec<UserProcess> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_user(UpdateKind::OnlyIfNotSet),
    );
    let own = std::process::id();
    let user = system
        .process(Pid::from_u32(own))
        .and_then(|p| p.user_id().cloned());
    let programs = programs();
    system
        .processes()
        .values()
        .filter(|process| process.pid().as_u32() != own && process.user_id() == user.as_ref())
        .map(|process| {
            let pid = process.pid().as_u32();
            UserProcess {
                pid,
                parent: process.parent().map(Pid::as_u32),
                app: app_of(pid, process.exe().map(PathBuf::from), &programs),
            }
        })
        .collect()
}

/// The app of a process: the one the desktop started it as, or the one whose program it runs.
pub fn process_app(pid: u32) -> Option<Entry> {
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
    app_of(pid, exe, &programs())
}

fn app_of(pid: u32, exe: Option<PathBuf>, programs: &HashMap<PathBuf, Entry>) -> Option<Entry> {
    scope_app_id(pid)
        .and_then(|id| find_entry(&id))
        .or_else(|| programs.get(&exe?).cloned())
}

/// Apps by the real path of their program, the one a process shows.
fn programs() -> HashMap<PathBuf, Entry> {
    let mut programs = HashMap::new();
    for entry in entries() {
        if let Some(program) = entry.program()
            && let Ok(real) = std::fs::canonicalize(&program)
        {
            programs.entry(real).or_insert(entry);
        }
    }
    programs
}
