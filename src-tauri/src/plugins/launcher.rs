//! Runs the main service of each plugin as Ulanzi Studio does: Node with
//! `<CodePath> <address> <port> <language> <version>` from the plugin folder, or the page of an
//! HTML plugin with the same values in its query. A process that stops is started again, waiting
//! longer each time.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use log::{info, warn};
use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System, UpdateKind};

use super::desktop::Desktop;
use super::hub::urlencode;
use super::manifest::Runtime;
use super::node::Node;
use super::registry::Plugin;

/// The version plugins are told the host is: the Ulanzi Studio they were tested against.
pub const HOST_VERSION: &str = "3.3.9";
const FIRST_RETRY: Duration = Duration::from_secs(1);
const LAST_RETRY: Duration = Duration::from_secs(60);
/// Running this long counts as working: the next failure retries quickly again.
const STABLE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RunState {
    /// Started and waiting for it to connect, or running.
    Running,
    /// Stopped by itself; started again after a while.
    Restarting {
        restarts: u32,
    },
    /// Cannot run here, and why.
    Unavailable {
        reason: String,
    },
    /// Has nothing to run: images only.
    Passive,
    Stopped,
}

struct Process {
    child: Arc<Mutex<Option<Child>>>,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<RunState>>,
}

/// macOS and Linux give an orphan another parent, so a plugin whose app was killed sees its
/// parent change; Windows keeps the old id, so the parent is also checked to be alive.
const WATCHDOG: &str = "const parent = process.ppid;\nsetInterval(() => {\n  let gone = process.ppid !== parent;\n  try { process.kill(parent, 0); } catch { gone = true; }\n  if (gone) process.exit(0);\n}, 1000).unref();\n";

pub struct Launcher {
    node: Option<Node>,
    desktop: Arc<dyn Desktop>,
    port: u16,
    language: String,
    logs: PathBuf,
    pids: PathBuf,
    /// Loaded before each plugin: ends it when the app is gone.
    watchdog: Option<PathBuf>,
    processes: Arc<Mutex<HashMap<String, Process>>>,
    pages: Mutex<Vec<String>>,
}

impl Launcher {
    pub fn new(
        node: Option<Node>,
        desktop: Arc<dyn Desktop>,
        port: u16,
        language: String,
        data_dir: &Path,
    ) -> Self {
        let pids = data_dir.join("plugin-processes.txt");
        kill_leftovers(&pids);
        let watchdog = data_dir.join("plugin-watchdog.cjs");
        let watchdog = match fs::write(&watchdog, WATCHDOG) {
            Ok(()) => Some(watchdog),
            Err(err) => {
                warn!("plugins will outlive a crash of the app: {err}");
                None
            }
        };
        Self {
            watchdog,
            node,
            desktop,
            port,
            language,
            logs: data_dir.join("plugin-logs"),
            pids,
            processes: Arc::default(),
            pages: Mutex::default(),
        }
    }

    pub fn node(&self) -> Option<&Node> {
        self.node.as_ref()
    }

    pub fn start(&self, plugin: &Plugin) {
        let id = plugin.manifest.uuid.clone();
        self.stop(&id);
        match &plugin.runtime {
            Runtime::Node(code) => self.start_node(&id, &plugin.dir, code),
            Runtime::Html(page) => {
                let url = format!(
                    "http://127.0.0.1:{port}/plugins/{folder}/{page}?address=127.0.0.1&port={port}&language={language}&uuid={id}",
                    port = self.port,
                    folder = urlencode(&plugin.folder),
                    language = urlencode(&self.language),
                    id = urlencode(&id),
                );
                info!("plugin {id} runs as a page");
                self.desktop.run_page(&id, &url);
                self.pages
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(id);
            }
            Runtime::Python(_) | Runtime::None => {}
        }
    }

    pub fn state(&self, plugin: &Plugin) -> RunState {
        match &plugin.runtime {
            Runtime::None => RunState::Passive,
            Runtime::Python(_) => RunState::Unavailable {
                reason: tr!("plugins.launcher.python").into(),
            },
            Runtime::Node(_) if self.node.is_none() => RunState::Unavailable {
                reason: tr!("plugins.launcher.needsNode").into(),
            },
            Runtime::Html(_) => {
                let pages = self.pages.lock().unwrap_or_else(PoisonError::into_inner);
                if pages.contains(&plugin.manifest.uuid) {
                    RunState::Running
                } else {
                    RunState::Stopped
                }
            }
            Runtime::Node(_) => self
                .processes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&plugin.manifest.uuid)
                .map(|p| {
                    p.state
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone()
                })
                .unwrap_or(RunState::Stopped),
        }
    }

    pub fn stop(&self, id: &str) {
        let process = self
            .processes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
        if let Some(process) = process {
            process.stop.store(true, Ordering::SeqCst);
            if let Some(mut child) = process
                .child
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
            {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        let mut pages = self.pages.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(at) = pages.iter().position(|p| p == id) {
            pages.remove(at);
            self.desktop.stop_page(id);
        }
        self.save_pids();
    }

    pub fn stop_all(&self) {
        let ids: Vec<String> = self
            .processes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .chain(
                self.pages
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .iter()
                    .cloned(),
            )
            .collect();
        for id in ids {
            self.stop(&id);
        }
    }

    fn start_node(&self, id: &str, dir: &Path, code: &Path) {
        let Some(node) = self.node.clone() else {
            warn!("plugin {id} needs Node and none was found");
            return;
        };
        let process = Process {
            child: Arc::default(),
            stop: Arc::default(),
            state: Arc::new(Mutex::new(RunState::Running)),
        };
        let child = Arc::clone(&process.child);
        let stop = Arc::clone(&process.stop);
        let state = Arc::clone(&process.state);
        self.processes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id.to_owned(), process);
        let id = id.to_owned();
        let mut args: Vec<String> = self
            .watchdog
            .iter()
            .flat_map(|w| ["--require".into(), w.to_string_lossy().into_owned()])
            .collect();
        args.extend([
            // Relative, from the plugin folder, as Ulanzi Studio starts them: some plugins only
            // run when the path they were given is the one they resolve to.
            code.strip_prefix(dir)
                .unwrap_or(code)
                .to_string_lossy()
                .into_owned(),
            "127.0.0.1".into(),
            self.port.to_string(),
            self.language.clone(),
            HOST_VERSION.into(),
        ]);
        let dir = dir.to_path_buf();
        let log = self.logs.join(format!("{id}.out.log"));
        let pids = self.pids.clone();
        let processes = Arc::clone(&self.processes);
        let spawned = thread::Builder::new()
            .name(format!("plugin {id}"))
            .spawn(move || {
                let mut wait = FIRST_RETRY;
                let mut restarts = 0;
                while !stop.load(Ordering::SeqCst) {
                    let started = Instant::now();
                    match spawn_node(&node.path, &args, &dir, &log) {
                        Ok(spawned) => {
                            info!("plugin {id} started (pid {})", spawned.id());
                            *child.lock().unwrap_or_else(PoisonError::into_inner) = Some(spawned);
                            write_pids(&pids, &pids_of(&processes));
                            *state.lock().unwrap_or_else(PoisonError::into_inner) =
                                RunState::Running;
                            let status = loop {
                                let mut guard =
                                    child.lock().unwrap_or_else(PoisonError::into_inner);
                                let Some(running) = guard.as_mut() else {
                                    break None;
                                };
                                match running.try_wait() {
                                    Ok(Some(status)) => break Some(status),
                                    Ok(None) => {}
                                    Err(_) => break None,
                                }
                                drop(guard);
                                thread::sleep(Duration::from_millis(250));
                            };
                            if stop.load(Ordering::SeqCst) {
                                break;
                            }
                            warn!("plugin {id} stopped: {status:?}");
                        }
                        Err(err) => warn!("plugin {id} could not start: {err}"),
                    }
                    if started.elapsed() > STABLE {
                        wait = FIRST_RETRY;
                    }
                    restarts += 1;
                    *state.lock().unwrap_or_else(PoisonError::into_inner) =
                        RunState::Restarting { restarts };
                    thread::sleep(wait);
                    wait = (wait * 2).min(LAST_RETRY);
                }
            });
        if let Err(err) = spawned {
            warn!("could not watch plugin: {err}");
        }
    }

    fn save_pids(&self) {
        write_pids(&self.pids, &pids_of(&self.processes));
    }
}

fn pids_of(processes: &Mutex<HashMap<String, Process>>) -> Vec<u32> {
    let children: Vec<Arc<Mutex<Option<Child>>>> = processes
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .values()
        .map(|p| Arc::clone(&p.child))
        .collect();
    children
        .iter()
        .filter_map(|c| {
            c.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
                .map(Child::id)
        })
        .collect()
}

fn spawn_node(node: &Path, args: &[String], dir: &Path, log: &Path) -> std::io::Result<Child> {
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent)?;
    }
    let out = OpenOptions::new().create(true).append(true).open(log)?;
    let err = out.try_clone()?;
    // Options meant for whatever started the app (a debugger, a test runner) would break the
    // plugin; Ulanzi Studio starts them with a clean Node.
    let mut command = Command::new(node);
    command
        .args(args)
        .current_dir(dir)
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .env_remove("NODE_ENV")
        // Open for as long as the child is kept, as Ulanzi Studio leaves it: some plugins stop
        // when their input closes, which also tells them when the app is gone.
        .stdin(Stdio::piped())
        .stdout(out)
        .stderr(err);
    #[cfg(target_os = "linux")]
    crate::os::linux::outside_bundle(&mut command);
    #[cfg(target_os = "windows")]
    crate::os::windows::without_console(&mut command);
    let child = command.spawn()?;
    #[cfg(target_os = "windows")]
    super::job_object::adopt(&child);
    Ok(child)
}

fn write_pids(file: &Path, pids: &[u32]) {
    let text: String = pids.iter().map(|p| format!("{p}\n")).collect();
    let _ = fs::write(file, text);
}

/// Plugins left running by a copy of the app that did not close cleanly. Only processes still
/// running Node with a plugin are killed, never a process that reused the id.
fn kill_leftovers(file: &Path) {
    let Ok(text) = fs::read_to_string(file) else {
        return;
    };
    let pids: Vec<Pid> = text
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .map(Pid::from_u32)
        .collect();
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    for pid in pids {
        let Some(process) = system.process(pid) else {
            continue;
        };
        let command = process
            .cmd()
            .iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        if command.contains("node") && command.contains(".ulanziPlugin") {
            info!("stopping plugin process {pid} left from before");
            // Windows has no signal to ask a process to end.
            if process.kill_with(Signal::Term).is_none() {
                process.kill();
            }
        }
    }
    let _ = File::create(file);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::desktop::Headless;
    use super::super::manifest::Manifest;
    use super::*;

    fn plugin(dir: &Path, code: &str) -> Plugin {
        let manifest: Manifest =
            serde_json::from_value(json!({ "UUID": "com.a.b.c", "CodePath": code })).unwrap();
        Plugin {
            runtime: manifest.runtime(dir),
            manifest,
            dir: dir.to_path_buf(),
            folder: "com.a.b.c.ulanziPlugin".into(),
        }
    }

    fn launcher(dir: &Path) -> Launcher {
        Launcher::new(None, Arc::new(Headless), 0, "es".into(), dir)
    }

    #[test]
    fn tells_what_cannot_run() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = launcher(dir.path());
        let images_only = plugin(dir.path(), "");
        launcher.start(&images_only);
        assert_eq!(launcher.state(&images_only), RunState::Passive);
        assert!(matches!(
            launcher.state(&plugin(dir.path(), "main.py")),
            RunState::Unavailable { .. }
        ));
        let node = plugin(dir.path(), "app.js");
        launcher.start(&node);
        assert!(matches!(
            launcher.state(&node),
            RunState::Unavailable { .. }
        ));
        assert!(launcher.node().is_none());
    }

    #[test]
    fn pages_run_until_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = launcher(dir.path());
        let page = plugin(dir.path(), "index.html");
        assert_eq!(launcher.state(&page), RunState::Stopped);
        launcher.start(&page);
        assert_eq!(launcher.state(&page), RunState::Running);
        launcher.stop_all();
        assert_eq!(launcher.state(&page), RunState::Stopped);
    }

    #[test]
    fn leftovers_that_are_not_plugins_are_not_killed() {
        let dir = tempfile::tempdir().unwrap();
        let pids = dir.path().join("plugin-processes.txt");
        fs::write(&pids, format!("{}\nnot a pid\n\n", std::process::id())).unwrap();
        kill_leftovers(&pids);
        assert_eq!(fs::read_to_string(&pids).unwrap(), "");
        let _ = launcher(dir.path());
        assert!(pids.is_file());
    }
}
