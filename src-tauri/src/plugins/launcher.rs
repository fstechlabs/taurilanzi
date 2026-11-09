//! Runs the main service of each plugin as Ulanzi Studio does: Node with
//! `<CodePath> <address> <port> <language> <version>` from the plugin folder, Python with the
//! same arguments after the whole path of its script, or the page of an HTML plugin with those
//! values in its query. A process that stops is started again, waiting longer each time.

use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
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
use super::elgato::protocol as streamdeck;
use super::hub::urlencode;
use super::manifest::{Protocol, Runtime};
use super::node::Node;
use super::python::Python;
use super::registry::Plugin;

/// The version plugins are told the host is: the Ulanzi Studio they were tested against.
pub const HOST_VERSION: &str = "3.3.9";
const FIRST_RETRY: Duration = Duration::from_secs(1);
const LAST_RETRY: Duration = Duration::from_secs(60);
/// Running this long counts as working: the next failure retries quickly again.
const STABLE: Duration = Duration::from_secs(30);
/// Options meant for whatever started the app (a debugger, a test runner) would break the
/// plugin; Ulanzi Studio starts them with a clean Node.
const NODE_OPTIONS: &[&str] = &["NODE_OPTIONS", "NODE_PATH", "NODE_ENV"];

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

/// A program to start for a plugin, from its folder.
#[derive(Debug, Clone)]
struct Program {
    path: PathBuf,
    args: Vec<String>,
    env: Vec<(String, OsString)>,
    /// Variables of the app's environment the plugin must not see.
    hidden: Vec<OsString>,
}

/// macOS and Linux give an orphan another parent, so a plugin whose app was killed sees its
/// parent change; Windows keeps the old id, so the parent is also checked to be alive.
const WATCHDOG: &str = "const parent = process.ppid;\nsetInterval(() => {\n  let gone = process.ppid !== parent;\n  try { process.kill(parent, 0); } catch { gone = true; }\n  if (gone) process.exit(0);\n}, 1000).unref();\n";

/// The same for Python, which loads `sitecustomize` from its path before the plugin. Windows
/// needs none: the job object ends plugins there.
const PYTHON_WATCHDOG: &str = "import os, threading, time\n\nif os.name != \"nt\":\n    _parent = os.getppid()\n\n    def _watch():\n        while True:\n            time.sleep(1)\n            if os.getppid() != _parent:\n                os._exit(0)\n\n    threading.Thread(target=_watch, name=\"host-watchdog\", daemon=True).start()\n";

pub struct Launcher {
    node: Option<Node>,
    python: Option<Python>,
    desktop: Arc<dyn Desktop>,
    port: u16,
    language: String,
    logs: PathBuf,
    pids: PathBuf,
    /// Loaded before each Node plugin: ends it when the app is gone.
    watchdog: Option<PathBuf>,
    /// The folder of the Python watchdog, last in the path of each Python plugin.
    python_watchdog: Option<PathBuf>,
    processes: Arc<Mutex<HashMap<String, Process>>>,
    pages: Mutex<Vec<String>>,
}

impl Launcher {
    pub fn new(
        node: Option<Node>,
        python: Option<Python>,
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
        let python_watchdog = data_dir.join("plugin-python");
        let python_watchdog = match fs::create_dir_all(&python_watchdog)
            .and_then(|()| fs::write(python_watchdog.join("sitecustomize.py"), PYTHON_WATCHDOG))
        {
            Ok(()) => Some(python_watchdog),
            Err(err) => {
                warn!("Python plugins will outlive a crash of the app: {err}");
                None
            }
        };
        Self {
            watchdog,
            python_watchdog,
            node,
            python,
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

    pub fn python(&self) -> Option<&Python> {
        self.python.as_ref()
    }

    pub fn start(&self, plugin: &Plugin) {
        let id = plugin.manifest.uuid.clone();
        self.stop(&id);
        let args = match plugin.protocol {
            Protocol::Ulanzi => self.host_arguments().to_vec(),
            Protocol::StreamDeck => streamdeck::arguments(self.port, plugin, &self.language),
        };
        match &plugin.runtime {
            Runtime::Node(code) => match self.node_program(&plugin.dir, code, args) {
                Some(program) => self.supervise(&id, &plugin.dir, program),
                None => warn!("plugin {id} needs Node and none was found"),
            },
            Runtime::Python(code) => match self.python_program(code, args) {
                Some(program) => self.supervise(&id, &plugin.dir, program),
                None => warn!("plugin {id} needs Python and none was found"),
            },
            Runtime::Native(code) if runs_here(code) => {
                make_runnable(code);
                let program = Program {
                    path: code.clone(),
                    args,
                    env: Vec::new(),
                    hidden: Vec::new(),
                };
                self.supervise(&id, &plugin.dir, program);
            }
            Runtime::Native(_) => warn!("plugin {id} is a program for another system"),
            Runtime::Html(page) if !plugin.file(page).is_some_and(|f| f.is_file()) => {
                warn!("plugin {id} has no page {page}");
            }
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
            Runtime::None => {}
        }
        self.install_deps(plugin);
    }

    pub fn state(&self, plugin: &Plugin) -> RunState {
        match &plugin.runtime {
            Runtime::None => RunState::Passive,
            Runtime::Python(_) if self.python.is_none() => RunState::Unavailable {
                reason: tr!("plugins.launcher.needsPython").into(),
            },
            Runtime::Node(_) if self.node.is_none() => RunState::Unavailable {
                reason: tr!("plugins.launcher.needsNode").into(),
            },
            Runtime::Native(code) if !runs_here(code) => RunState::Unavailable {
                reason: tr!("plugins.launcher.otherSystem").into(),
            },
            Runtime::Html(_) => {
                let pages = self.pages.lock().unwrap_or_else(PoisonError::into_inner);
                if pages.contains(&plugin.manifest.uuid) {
                    RunState::Running
                } else {
                    RunState::Stopped
                }
            }
            Runtime::Node(_) | Runtime::Python(_) | Runtime::Native(_) => self
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
                #[cfg(unix)]
                super::process_group::end(child.id(), false);
                let _ = child.kill();
                let _ = child.wait();
                #[cfg(unix)]
                super::process_group::end(child.id(), true);
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

    fn node_program(&self, dir: &Path, code: &Path, host: Vec<String>) -> Option<Program> {
        let node = self.node.as_ref()?;
        let mut args: Vec<String> = self
            .watchdog
            .iter()
            .flat_map(|w| ["--require".into(), w.to_string_lossy().into_owned()])
            .collect();
        // Relative, from the plugin folder, as Ulanzi Studio starts them: some plugins only run
        // when the path they were given is the one they resolve to.
        args.push(
            code.strip_prefix(dir)
                .unwrap_or(code)
                .to_string_lossy()
                .into_owned(),
        );
        args.extend(host);
        Some(Program {
            path: node.path.clone(),
            args,
            env: Vec::new(),
            hidden: NODE_OPTIONS.iter().map(OsString::from).collect(),
        })
    }

    /// As Ulanzi Studio starts them: the whole path of the script, its folder first in the
    /// path, and no bytecode written next to it.
    fn python_program(&self, code: &Path, host: Vec<String>) -> Option<Program> {
        let python = self.python.as_ref()?;
        let folders = code
            .parent()
            .map(Path::to_path_buf)
            .into_iter()
            .chain(self.python_watchdog.clone());
        let path = env::join_paths(folders).ok()?;
        let mut args = vec![code.to_string_lossy().into_owned()];
        args.extend(host);
        Some(Program {
            path: python.path.clone(),
            args,
            env: vec![
                ("PYTHONPATH".into(), path),
                ("PYTHONDONTWRITEBYTECODE".into(), "1".into()),
                // So the log has what a plugin printed before it stopped, in any language.
                ("PYTHONUNBUFFERED".into(), "1".into()),
                ("PYTHONIOENCODING".into(), "utf-8".into()),
            ],
            // A Python installed on the computer would point the bundled one at its own files.
            hidden: env::vars_os()
                .map(|(name, _)| name)
                .filter(|name| name.to_string_lossy().starts_with("PYTHON"))
                .collect(),
        })
    }

    fn host_arguments(&self) -> [String; 4] {
        [
            "127.0.0.1".into(),
            self.port.to_string(),
            self.language.clone(),
            HOST_VERSION.into(),
        ]
    }

    /// Runs the `InstallToDepsApp` script of a plugin as Ulanzi Studio does, `<script> --silent`
    /// with Node, without waiting for it.
    fn install_deps(&self, plugin: &Plugin) {
        let Some(script) = plugin.manifest.deps_installer(&plugin.dir) else {
            return;
        };
        let id = plugin.manifest.uuid.clone();
        let Some(node) = self.node.as_ref() else {
            warn!("plugin {id} sets up its app with Node and none was found");
            return;
        };
        if !script.is_file() {
            warn!("plugin {id} names a setup script that is not there");
            return;
        }
        let program = Program {
            path: node.path.clone(),
            args: vec![script.to_string_lossy().into_owned(), "--silent".into()],
            env: Vec::new(),
            hidden: NODE_OPTIONS.iter().map(OsString::from).collect(),
        };
        let dir = plugin.dir.clone();
        let log = self.logs.join(format!("{id}.setup.log"));
        let spawned = thread::Builder::new()
            .name(format!("plugin setup {id}"))
            .spawn(move || match spawn(&program, &dir, &log, Stdio::null()) {
                Ok(mut child) => {
                    info!("plugin {id} sets up its app (pid {})", child.id());
                    let _ = child.wait();
                }
                Err(err) => warn!("plugin {id} could not set up its app: {err}"),
            });
        if let Err(err) = spawned {
            warn!("could not set up the app of a plugin: {err}");
        }
    }

    fn supervise(&self, id: &str, dir: &Path, program: Program) {
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
                    // Open for as long as the child is kept, as Ulanzi Studio leaves it: some
                    // plugins stop when their input closes, which also tells them when the app
                    // is gone.
                    match spawn(&program, &dir, &log, Stdio::piped()) {
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
                                    Ok(Some(status)) => {
                                        // What the plugin started dies with it, before it is
                                        // started again.
                                        #[cfg(unix)]
                                        super::process_group::end(running.id(), true);
                                        break Some(status);
                                    }
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

fn spawn(program: &Program, dir: &Path, log: &Path, stdin: Stdio) -> std::io::Result<Child> {
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent)?;
    }
    let out = OpenOptions::new().create(true).append(true).open(log)?;
    let err = out.try_clone()?;
    let mut command = Command::new(&program.path);
    #[cfg(target_os = "linux")]
    crate::os::linux::outside_bundle(&mut command);
    for name in &program.hidden {
        command.env_remove(name);
    }
    command
        .args(&program.args)
        .envs(program.env.iter().map(|(name, value)| (name, value)))
        .current_dir(dir)
        .stdin(stdin)
        .stdout(out)
        .stderr(err);
    #[cfg(target_os = "windows")]
    crate::os::windows::without_console(&mut command);
    // Plugins call the system's tools by name, and a shell's path may put others of the same
    // name first, such as Python's `xattr`.
    #[cfg(target_os = "macos")]
    command.env("PATH", system_first(env::var_os("PATH")));
    #[cfg(unix)]
    super::process_group::isolate(&mut command);
    let child = command.spawn()?;
    #[cfg(target_os = "windows")]
    super::job_object::adopt(&child);
    Ok(child)
}

#[cfg(target_os = "macos")]
fn system_first(path: Option<OsString>) -> OsString {
    let system = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"].map(PathBuf::from);
    let rest = path
        .iter()
        .flat_map(env::split_paths)
        .filter(|dir| !system.contains(dir));
    env::join_paths(system.iter().cloned().chain(rest)).unwrap_or_default()
}

fn write_pids(file: &Path, pids: &[u32]) {
    let text: String = pids.iter().map(|p| format!("{p}\n")).collect();
    let _ = fs::write(file, text);
}

/// A program for this system: Windows runs `.exe` files, macOS the others. Linux runs neither,
/// as the stores only have them for those two.
pub fn runs_here(code: &Path) -> bool {
    if !code.is_file() {
        return false;
    }
    if cfg!(target_os = "windows") {
        return code
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    }
    // Packages for several systems name every program alike: what it is built for is in its
    // first bytes.
    let mut start = [0u8; 4];
    let read = fs::File::open(code).and_then(|mut f| std::io::Read::read(&mut f, &mut start));
    let start = &start[..read.unwrap_or(0)];
    let own = if cfg!(target_os = "macos") {
        const MACH_O: [[u8; 4]; 5] = [
            [0xcf, 0xfa, 0xed, 0xfe],
            [0xce, 0xfa, 0xed, 0xfe],
            [0xfe, 0xed, 0xfa, 0xcf],
            [0xfe, 0xed, 0xfa, 0xce],
            [0xca, 0xfe, 0xba, 0xbe],
        ];
        MACH_O.iter().any(|m| start == m)
    } else {
        start == b"\x7fELF"
    };
    own || start.starts_with(b"#!")
}

/// Packages made on Windows lose the mark that lets a program run.
fn make_runnable(code: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(code)
            && meta.permissions().mode() & 0o111 == 0
        {
            let _ = fs::set_permissions(
                code,
                fs::Permissions::from_mode(meta.permissions().mode() | 0o755),
            );
        }
    }
    #[cfg(not(unix))]
    let _ = code;
}

/// Plugins left running by a copy of the app that did not close cleanly. Only processes still
/// running a plugin, from its folder or with its path, are killed, never a process that reused the
/// id.
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
        ProcessRefreshKind::nothing()
            .with_cmd(UpdateKind::Always)
            .with_cwd(UpdateKind::Always),
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
        let place = process
            .cwd()
            .map(|dir| dir.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let lower = command.to_ascii_lowercase();
        let plugin = [&lower, &place]
            .iter()
            .any(|s| s.contains(".ulanziplugin") || s.contains(".sdplugin"));
        if plugin {
            info!("stopping plugin process {pid} left from before");
            #[cfg(unix)]
            super::process_group::end(pid.as_u32(), false);
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
            runtime: manifest.runtime(dir, Protocol::Ulanzi),
            protocol: Protocol::Ulanzi,
            manifest,
            dir: dir.to_path_buf(),
            folder: "com.a.b.c.ulanziPlugin".into(),
        }
    }

    fn launcher(dir: &Path) -> Launcher {
        Launcher::new(None, None, Arc::new(Headless), 0, "es".into(), dir)
    }

    fn manifest_plugin(dir: &Path, manifest: serde_json::Value) -> Plugin {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        let manifest = Manifest::read(dir).unwrap();
        Plugin {
            runtime: manifest.runtime(dir, Protocol::Ulanzi),
            protocol: Protocol::Ulanzi,
            manifest,
            dir: dir.to_path_buf(),
            folder: "com.a.b.c.ulanziPlugin".into(),
        }
    }

    /// Waits for a file a plugin writes when it starts.
    fn written(file: &Path) -> String {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if let Ok(text) = fs::read_to_string(file)
                && !text.is_empty()
            {
                return text;
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("{} was never written", file.display());
    }

    #[test]
    fn python_runs_the_whole_path_of_the_script_with_its_folder_first() {
        let dir = tempfile::tempdir().unwrap();
        let python = Python {
            path: PathBuf::from("/opt/python/bin/python3"),
            version: "3.14.10".into(),
        };
        let launcher = Launcher::new(
            None,
            Some(python),
            Arc::new(Headless),
            3906,
            "es".into(),
            dir.path(),
        );
        let code = dir.path().join("p.ulanziPlugin/plugin/app.py");
        let program = launcher
            .python_program(&code, launcher.host_arguments().to_vec())
            .unwrap();
        assert_eq!(program.path, PathBuf::from("/opt/python/bin/python3"));
        assert_eq!(
            program.args,
            vec![
                code.to_string_lossy().into_owned(),
                "127.0.0.1".into(),
                "3906".into(),
                "es".into(),
                HOST_VERSION.into()
            ]
        );
        let env: HashMap<&str, &OsString> =
            program.env.iter().map(|(k, v)| (k.as_str(), v)).collect();
        let path: Vec<PathBuf> = env::split_paths(env["PYTHONPATH"]).collect();
        assert_eq!(
            path,
            vec![
                code.parent().unwrap().to_path_buf(),
                dir.path().join("plugin-python")
            ]
        );
        assert_eq!(env["PYTHONDONTWRITEBYTECODE"], "1");
        assert!(
            fs::read_to_string(dir.path().join("plugin-python/sitecustomize.py"))
                .unwrap()
                .contains("getppid")
        );
        assert!(launcher.python().is_some());
    }

    #[test]
    fn runs_a_python_plugin_with_the_bundled_python() {
        let Some(python) = super::super::python::find(None) else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let plugin = manifest_plugin(
            &dir.path().join("p.ulanziPlugin"),
            json!({ "UUID": "com.a.b.c", "CodePath": "plugin/main", "Type": "Python" }),
        );
        let script = "import json, os, sys, websockets\n\
                      json.dump({'argv': sys.argv[1:], 'first': sys.path[0], 'watchdog': 'sitecustomize' in sys.modules}, open(os.path.join(os.path.dirname(__file__), 'ran.json'), 'w'))\n\
                      import time\n\
                      time.sleep(30)\n";
        fs::create_dir_all(plugin.dir.join("plugin")).unwrap();
        fs::write(plugin.dir.join("plugin/main"), script).unwrap();
        let launcher = Launcher::new(
            None,
            Some(python),
            Arc::new(Headless),
            3906,
            "es".into(),
            dir.path(),
        );
        launcher.start(&plugin);
        let ran: serde_json::Value =
            serde_json::from_str(&written(&plugin.dir.join("plugin/ran.json"))).unwrap();
        assert_eq!(
            ran["argv"],
            json!(["127.0.0.1", "3906", "es", HOST_VERSION])
        );
        assert_eq!(
            PathBuf::from(ran["first"].as_str().unwrap()).file_name(),
            Some("plugin".as_ref())
        );
        assert_eq!(ran["watchdog"], true);
        assert_eq!(launcher.state(&plugin), RunState::Running);
        launcher.stop_all();
        assert_eq!(launcher.state(&plugin), RunState::Stopped);
        assert!(
            !plugin.dir.join("plugin/__pycache__").exists(),
            "no bytecode next to the plugin"
        );
    }

    #[test]
    fn runs_the_setup_script_with_node_each_time_the_plugin_starts() {
        let Some(node) = super::super::node::find(None) else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let plugin = manifest_plugin(
            &dir.path().join("p.ulanziPlugin"),
            json!({
                "UUID": "com.a.b.c",
                "InstallToDepsApp": { "Type": "JavaScript", "CodePath": "setup/install.js" },
            }),
        );
        fs::create_dir_all(plugin.dir.join("setup")).unwrap();
        fs::write(
            plugin.dir.join("setup/install.js"),
            "require('fs').writeFileSync(__dirname + '/ran.json', JSON.stringify(process.argv.slice(2)))",
        )
        .unwrap();
        let launcher = Launcher::new(
            Some(node),
            None,
            Arc::new(Headless),
            0,
            "es".into(),
            dir.path(),
        );
        launcher.start(&plugin);
        assert_eq!(
            written(&plugin.dir.join("setup/ran.json")),
            r#"["--silent"]"#
        );
        assert_eq!(launcher.state(&plugin), RunState::Passive);
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
        launcher.start(&page);
        assert_eq!(launcher.state(&page), RunState::Stopped, "no such page");
        fs::write(dir.path().join("index.html"), "<p>x</p>").unwrap();
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

    /// A Stream Deck package as the registry loads it.
    fn stream_deck_plugin(dir: &Path, manifest: serde_json::Value) -> Plugin {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        let mut manifest = Manifest::read(dir).unwrap();
        manifest.fill_id(dir.file_name().unwrap().to_str().unwrap());
        Plugin {
            runtime: manifest.runtime(dir, Protocol::StreamDeck),
            protocol: Protocol::StreamDeck,
            manifest,
            dir: dir.to_path_buf(),
            folder: dir.file_name().unwrap().to_string_lossy().into_owned(),
        }
    }

    #[test]
    fn stream_deck_plugins_get_their_arguments_in_pairs_from_their_folder() {
        let Some(node) = super::super::node::find(None) else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let plugin = stream_deck_plugin(
            &dir.path().join("com.a.sd.sdPlugin"),
            json!({ "CodePathMac": "plugin/index.js", "CodePathWin": "plugin/index.js", "CodePath": "plugin/index.js", "Version": "2.0" }),
        );
        fs::create_dir_all(plugin.dir.join("plugin")).unwrap();
        fs::write(
            plugin.dir.join("plugin/index.js"),
            "require('fs').writeFileSync('ran.json', JSON.stringify({ argv: process.argv.slice(2) }))\nsetTimeout(() => {}, 30000)\n",
        )
        .unwrap();
        let launcher = Launcher::new(
            Some(node),
            None,
            Arc::new(Headless),
            3906,
            "es".into(),
            dir.path(),
        );
        launcher.start(&plugin);
        let ran: serde_json::Value =
            serde_json::from_str(&written(&plugin.dir.join("ran.json"))).unwrap();
        let argv = ran["argv"].as_array().unwrap();
        assert_eq!(
            argv[..7],
            [
                json!("-port"),
                json!("3906"),
                json!("-pluginUUID"),
                json!("com.a.sd"),
                json!("-registerEvent"),
                json!("registerPlugin"),
                json!("-info"),
            ],
            "the id from the folder, the file written in the folder"
        );
        let info: serde_json::Value = serde_json::from_str(argv[7].as_str().unwrap()).unwrap();
        assert_eq!(
            info["plugin"],
            json!({ "uuid": "com.a.sd", "version": "2.0" })
        );
        launcher.stop_all();
    }

    #[test]
    fn programs_of_their_own_run_only_on_their_system() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = Launcher::new(
            None,
            None,
            Arc::new(Headless),
            3906,
            "es".into(),
            dir.path(),
        );
        let windows = stream_deck_plugin(
            &dir.path().join("com.a.win.sdPlugin"),
            json!({ "CodePath": "bin/plugin" }),
        );
        fs::create_dir_all(windows.dir.join("bin")).unwrap();
        fs::write(windows.dir.join("bin/plugin.exe"), "MZ").unwrap();
        let windows = stream_deck_plugin(&windows.dir, json!({ "CodePath": "bin/plugin" }));
        assert_eq!(
            windows.runtime,
            Runtime::Native(windows.dir.join("bin/plugin.exe")),
            "named without its .exe"
        );
        let state = launcher.state(&windows);
        if cfg!(target_os = "windows") {
            assert!(!matches!(state, RunState::Unavailable { .. }), "{state:?}");
        } else {
            assert!(matches!(state, RunState::Unavailable { .. }), "{state:?}");
        }
        let missing = stream_deck_plugin(
            &dir.path().join("com.a.gone.sdPlugin"),
            json!({ "CodePathMac": "gone", "CodePathWin": "gone.exe" }),
        );
        assert!(matches!(
            launcher.state(&missing),
            RunState::Unavailable { .. }
        ));
    }

    #[test]
    fn programs_are_told_apart_by_what_they_are_built_for() {
        let dir = tempfile::tempdir().unwrap();
        let file = |name: &str, bytes: &[u8]| {
            let path = dir.path().join(name);
            fs::write(&path, bytes).unwrap();
            path
        };
        let mach_o = file("mac", &[0xcf, 0xfa, 0xed, 0xfe, 7, 0, 0, 1]);
        let universal = file("universal", &[0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 2]);
        let elf = file("linux", b"\x7fELF\x02\x01");
        let script = file("script", b"#!/bin/sh\n");
        let exe = file("plugin.exe", b"MZ");
        let text = file("notes", b"hello");
        let windows = cfg!(target_os = "windows");
        let mac = cfg!(target_os = "macos");
        assert_eq!(runs_here(&mach_o), mac);
        assert_eq!(runs_here(&universal), mac);
        assert_eq!(runs_here(&elf), !windows && !mac);
        assert_eq!(runs_here(&script), !windows);
        assert_eq!(runs_here(&exe), windows);
        assert!(!runs_here(&text) || windows);
        assert!(!runs_here(&dir.path().join("missing")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_program_that_lost_its_mark_runs_again() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("program");
        fs::write(&program, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(runs_here(&program));
        make_runnable(&program);
        assert_eq!(
            fs::metadata(&program).unwrap().permissions().mode() & 0o111,
            0o111
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_system_tools_come_first_in_the_path_of_plugins() {
        let path = system_first(Some("/opt/py/bin:/usr/bin:/custom".into()));
        assert_eq!(
            path,
            OsString::from("/usr/bin:/bin:/usr/sbin:/sbin:/opt/py/bin:/custom")
        );
    }
}
