use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System};

use crate::actions::error::ActionError;
use crate::os::linux;

/// Opens an app (its desktop file or its program), a file, a folder or a URL, as the desktop's
/// file manager would.
pub fn open(target: &str) -> Result<(), ActionError> {
    let path = Path::new(target);
    let mut command = if is_desktop_file(path) {
        let mut command = Command::new("gio");
        command.args(["launch", target]);
        command
    } else if is_program(path) {
        Command::new(path)
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(target);
        command
    };
    linux::outside_bundle(&mut command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Its own group, so the app it opens outlives this one.
        .process_group(0);
    let mut child = command.spawn()?;
    std::thread::spawn(move || match child.wait() {
        Ok(status) if !status.success() => log::warn!("opening exited with {status}"),
        _ => {}
    });
    Ok(())
}

/// Asks every process of the app to quit, as logging out would; the app may still save first.
/// `app` is its desktop file or its program.
pub fn close_app(app: &str) -> Result<(), ActionError> {
    let path = Path::new(app);
    if !path.exists() {
        return Err(ActionError::NotFound(app.into()));
    }
    let wanted = if is_desktop_file(path) {
        let entry = linux::entry_at(path).ok_or_else(|| ActionError::NotAnApp(app.into()))?;
        Wanted::App {
            program: entry.program().and_then(|p| std::fs::canonicalize(p).ok()),
            id: entry.id,
        }
    } else if is_program(path) {
        Wanted::Program(std::fs::canonicalize(path)?)
    } else {
        return Err(ActionError::NotAnApp(app.into()));
    };
    let processes = linux::user_processes();
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::OnlyIfNotSet),
    );
    let matching: HashSet<u32> = processes
        .iter()
        .filter(|process| {
            let exe = system
                .process(Pid::from_u32(process.pid))
                .and_then(|p| p.exe());
            wanted.matches(process, exe)
        })
        .map(|process| process.pid)
        .collect();
    // Only the app's first process: it closes its helpers itself, which may otherwise report a
    // crash.
    for process in &processes {
        if matching.contains(&process.pid)
            && !process
                .parent
                .is_some_and(|parent| matching.contains(&parent))
            && let Some(running) = system.process(Pid::from_u32(process.pid))
        {
            running.kill_with(Signal::Term);
        }
    }
    Ok(())
}

enum Wanted {
    App {
        id: String,
        program: Option<PathBuf>,
    },
    Program(PathBuf),
}

impl Wanted {
    fn matches(&self, process: &linux::UserProcess, exe: Option<&Path>) -> bool {
        match self {
            Self::App { id, program } => {
                process.app.as_ref().is_some_and(|app| &app.id == id)
                    || program.is_some() && exe == program.as_deref()
            }
            Self::Program(program) => exe == Some(program.as_path()),
        }
    }
}

fn is_desktop_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "desktop") && path.is_file()
}

fn is_program(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}
