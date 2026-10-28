//! The cloud as the rest of the app sees it: signing this computer in and out, and a thread that
//! syncs a while after things change, every few minutes and when asked.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use uuid::Uuid;

use super::api::{Api, ApiError};
use super::documents::{Kind, SETTINGS_ID, TOKEN_ACCOUNT, is_synced_account, local_docs};
use super::plugins::{MissingPlugin, PluginInstaller, reconcile};
use super::sign_in::{self, SignInError};
use super::state::{StateFile, SyncState};
use super::status::{CloudProfile, CloudStatus, JoinMode, JoinOffer};
use super::worker::{Outcome, SyncError};
use crate::config::AssetStore;
use crate::engine::{EngineEvent, EngineHandle};
use crate::secrets::Secrets;

/// Edits wait this long to go up together, and other computers' changes come down as often.
const INTERVAL: Duration = Duration::from_secs(5 * 60);
/// After the app starts, once the engine is up.
const FIRST_SYNC: Duration = Duration::from_secs(5);
/// What is left after joining an account, such as merged settings.
const SOON: Duration = Duration::from_secs(2);
/// What the cloud answers when the account's space is full even after deleting old versions.
const QUOTA_EXCEEDED: u16 = 507;
/// How often to ask the cloud, cheaply, whether other computers changed something.
const CHECK_EVERY: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error("this computer is already signing in")]
    Busy,
    #[error(transparent)]
    SignIn(#[from] SignInError),
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

type Notify = Box<dyn Fn(CloudStatus) + Send + Sync>;
pub type Open = Box<dyn Fn(&str) -> io::Result<()> + Send + Sync>;

pub struct CloudSetup {
    pub data_dir: PathBuf,
    /// The cloud's web, without a trailing slash.
    pub url: String,
    pub app_name: String,
    pub engine: EngineHandle,
    pub secrets: Secrets,
    pub assets: AssetStore,
    /// Gets every change of the status.
    pub notify: Notify,
    /// Opens a page in the user's browser.
    pub open: Open,
    /// None when plugins cannot run in this build.
    pub plugins: Option<Arc<dyn PluginInstaller>>,
}

enum Message {
    Sync,
    Join(JoinMode),
    /// Syncs what is left before the app quits, then answers.
    Flush(Sender<()>),
}

pub(super) struct Shared {
    pub(super) state: SyncState,
    signing_in: bool,
    syncing: bool,
    problem: Option<String>,
    pub(super) join: Option<JoinOffer>,
    /// Synced profiles with changes the cloud does not have yet.
    pending: BTreeSet<Uuid>,
    /// The settings, or profiles deleted here, have changes the cloud does not have yet.
    settings_pending: bool,
    local_only: BTreeSet<Uuid>,
    /// The cloud has changes of other computers this one has not downloaded.
    incoming: bool,
    /// Plugins of the other computers this one could not install.
    missing_plugins: Vec<MissingPlugin>,
}

pub(super) struct Inner {
    url: String,
    app_name: String,
    device_name: String,
    pub(super) engine: EngineHandle,
    secrets: Secrets,
    pub(super) assets: AssetStore,
    pub(super) file: StateFile,
    notify: Notify,
    open: Open,
    shared: Mutex<Shared>,
    cancel: AtomicBool,
    plugins: Option<Arc<dyn PluginInstaller>>,
    /// Plugins already looked up in the catalog without finding them, this run.
    checked_plugins: Mutex<BTreeSet<String>>,
}

#[derive(Clone)]
pub struct Cloud {
    inner: Arc<Inner>,
    messages: Sender<Message>,
}

impl Cloud {
    pub fn start(setup: CloudSetup) -> io::Result<Self> {
        let file = StateFile::new(&setup.data_dir);
        let state = file.load();
        let device_name = sysinfo::System::host_name()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| tr!("cloud.unnamedComputer").into());
        let inner = Arc::new(Inner {
            url: setup.url,
            app_name: setup.app_name,
            device_name,
            engine: setup.engine,
            secrets: setup.secrets,
            assets: setup.assets,
            file,
            notify: setup.notify,
            open: setup.open,
            shared: Mutex::new(Shared {
                state,
                signing_in: false,
                syncing: false,
                problem: None,
                join: None,
                pending: BTreeSet::new(),
                settings_pending: false,
                local_only: BTreeSet::new(),
                incoming: false,
                missing_plugins: Vec::new(),
            }),
            cancel: AtomicBool::new(false),
            plugins: setup.plugins,
            checked_plugins: Mutex::default(),
        });
        let (messages, receiver) = mpsc::channel();
        let worker = Arc::clone(&inner);
        thread::Builder::new()
            .name("cloud".into())
            .spawn(move || worker.run(receiver))?;
        let watcher = Arc::clone(&inner);
        thread::Builder::new()
            .name("cloud-changes".into())
            .spawn(move || watcher.watch_changes())?;
        Ok(Self { inner, messages })
    }

    pub fn status(&self) -> CloudStatus {
        self.inner.status()
    }

    /// Opens the browser to sign in and waits for it to come back, for minutes if need be.
    pub fn sign_in(&self, locale: &str) -> Result<(), CloudError> {
        let inner = &self.inner;
        {
            let mut shared = inner.lock();
            if shared.signing_in {
                return Err(CloudError::Busy);
            }
            shared.signing_in = true;
        }
        inner.cancel.store(false, Ordering::Relaxed);
        inner.changed();
        let result = inner.sign_in(locale);
        inner.lock().signing_in = false;
        inner.changed();
        if result.is_ok() {
            let _ = self.messages.send(Message::Sync);
        }
        result
    }

    /// Opens the account's page of the cloud in the browser, where the versions are.
    pub fn open_web(&self, locale: &str) -> io::Result<()> {
        (self.inner.open)(&format!("{}/{locale}/cloud", self.inner.url))
    }

    pub fn cancel_sign_in(&self) {
        self.inner.cancel.store(true, Ordering::Relaxed);
    }

    /// Profiles stay on this computer as they are; the cloud keeps them for the other ones.
    pub fn sign_out(&self) -> Result<(), CloudError> {
        if let Some(api) = self.inner.api()
            && let Err(err) = api.revoke()
        {
            log::warn!("could not tell the cloud this computer signed out: {err}");
        }
        self.inner.forget_account()?;
        self.inner.changed();
        Ok(())
    }

    pub fn sync_now(&self) {
        let _ = self.messages.send(Message::Sync);
    }

    pub fn join(&self, mode: JoinMode) {
        let _ = self.messages.send(Message::Join(mode));
    }

    pub fn set_paused(&self, paused: bool) -> io::Result<()> {
        self.inner.update(|state| state.paused = paused)?;
        if !paused {
            self.sync_now();
        }
        Ok(())
    }

    /// Before a synced profile is deleted on this computer only, or kept here while the cloud
    /// and the other computers keep their copy: the cloud's stays out of here from then on.
    pub fn leave_out(&self, id: Uuid, name: &str) -> io::Result<()> {
        if !self.inner.signed_in() {
            return Ok(());
        }
        self.inner.update(|state| {
            state.ignored.insert(id, name.to_owned());
        })
    }

    /// Brings back a profile of the cloud that was left out of this computer.
    pub fn bring_back(&self, id: Uuid) -> io::Result<()> {
        self.inner.update(|state| {
            state.ignored.remove(&id);
        })?;
        self.sync_now();
        Ok(())
    }

    /// Uploads what changed here before the app quits, waiting at most `wait`.
    pub fn flush(&self, wait: Duration) {
        if !self.inner.signed_in() || self.inner.lock().state.paused {
            return;
        }
        let (done, finished) = mpsc::channel();
        if self.messages.send(Message::Flush(done)).is_ok() {
            let _ = finished.recv_timeout(wait);
        }
    }
}

impl Inner {
    pub(super) fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Tells which profiles have changes to upload after every saved change of the configuration.
    fn watch_changes(&self) {
        let mut events = self.engine.subscribe();
        let mut revision = None;
        loop {
            match events.blocking_recv() {
                Ok(EngineEvent::State(snapshot)) if revision != Some(snapshot.revision) => {
                    revision = Some(snapshot.revision);
                    self.refresh_pending();
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => self.refresh_pending(),
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }

    /// Tells the account which plugins this computer has and installs the others'. Answers
    /// whether the account learned of new ones, which have to go up.
    fn reconcile_plugins(&self) -> Result<bool, SyncError> {
        let Some(installer) = &self.plugins else {
            return Ok(false);
        };
        let export = self.engine.sync_export_blocking()?;
        let reconciled = {
            let mut checked = self
                .checked_plugins
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            reconcile(installer.as_ref(), &export, &mut checked)
        };
        let learned = !reconciled.learned.is_empty();
        if learned {
            self.engine.plugins_installed_blocking(reconciled.learned)?;
        }
        self.lock().missing_plugins = reconciled.missing;
        Ok(learned)
    }

    /// What this computer has changed since its last sync.
    pub(super) fn refresh_pending(&self) {
        let (pending, settings_pending, local_only) = if self.signed_in() {
            let docs = self
                .engine
                .sync_export_blocking()
                .map_err(|err| err.to_string())
                .and_then(|export| {
                    let secrets = self.synced_secrets().map_err(|err| err.to_string())?;
                    let docs = local_docs(&export, secrets).map_err(|err| err.to_string())?;
                    Ok((docs, export.local))
                });
            let (docs, local_only) = match docs {
                Ok(found) => found,
                Err(err) => {
                    log::warn!("could not tell what changed since the last sync: {err}");
                    return;
                }
            };
            let shared = self.lock();
            let changed: BTreeSet<Uuid> = docs
                .iter()
                .filter(|(id, doc)| {
                    shared
                        .state
                        .documents
                        .get(*id)
                        .and_then(|b| b.hash.as_ref())
                        != Some(&doc.hash)
                })
                .map(|(id, _)| *id)
                .collect();
            let profiles = changed
                .iter()
                .copied()
                .filter(|id| docs.get(id).is_some_and(|d| d.kind == Kind::Profile))
                .collect();
            // A synced profile deleted here, or made local and taken out of the cloud, goes up as
            // a deletion; one deleted or made local here only does not.
            let deleted = shared.state.documents.iter().any(|(id, base)| {
                base.hash.is_some()
                    && !docs.contains_key(id)
                    && !shared.state.ignored.contains_key(id)
            });
            (
                profiles,
                changed.contains(&SETTINGS_ID) || deleted,
                local_only,
            )
        } else {
            (BTreeSet::new(), false, BTreeSet::new())
        };
        let changed = {
            let mut shared = self.lock();
            let changed = shared.pending != pending || shared.settings_pending != settings_pending;
            shared.pending = pending;
            shared.settings_pending = settings_pending;
            shared.local_only = local_only;
            changed
        };
        if changed {
            self.changed();
        }
    }

    /// Asks the cloud which version of each document is the current one, without downloading
    /// anything, to tell whether other computers changed something.
    fn check_remote(&self) {
        if !self.signed_in() || self.lock().state.paused {
            return;
        }
        let Some(api) = self.api() else { return };
        let remote = match api.state() {
            Ok(remote) => remote,
            Err(err) => {
                log::info!("could not ask the cloud for changes: {err}");
                return;
            }
        };
        let changed = {
            let mut shared = self.lock();
            let incoming = remote.documents.iter().any(|d| {
                !shared.state.ignored.contains_key(&d.id)
                    && !shared.local_only.contains(&d.id)
                    && match shared.state.documents.get(&d.id) {
                        Some(base) => base.version != d.head.version,
                        None => !d.head.deleted,
                    }
            });
            let changed = shared.incoming != incoming;
            shared.incoming = incoming;
            changed
        };
        if changed {
            self.changed();
        }
    }

    fn run(&self, messages: Receiver<Message>) {
        self.refresh_pending();
        let mut next_sync = Instant::now() + FIRST_SYNC;
        let mut next_check = Instant::now() + CHECK_EVERY;
        loop {
            let due = next_sync.min(next_check);
            let message = match messages.recv_timeout(due.saturating_duration_since(Instant::now()))
            {
                Ok(message) => Some(message),
                Err(RecvTimeoutError::Timeout) if Instant::now() < next_sync => {
                    self.check_remote();
                    next_check = Instant::now() + CHECK_EVERY;
                    continue;
                }
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            let (join, done) = match message {
                Some(Message::Join(mode)) => (Some(mode), None),
                Some(Message::Flush(done)) => (None, Some(done)),
                Some(Message::Sync) | None => (None, None),
            };
            let again = self.sync(join);
            next_sync = Instant::now() + if again { SOON } else { INTERVAL };
            next_check = Instant::now() + CHECK_EVERY;
            if let Some(done) = done {
                let _ = done.send(());
            }
        }
    }

    /// Returns whether to sync again soon.
    fn sync(&self, join: Option<JoinMode>) -> bool {
        if !self.signed_in() || self.lock().state.paused {
            return false;
        }
        self.lock().syncing = true;
        self.changed();
        let result = self.tick(join).and_then(|outcome| {
            let learned = self.reconcile_plugins()?;
            Ok(Outcome {
                again: outcome.again || learned,
                ..outcome
            })
        });
        let again = {
            let mut shared = self.lock();
            shared.syncing = false;
            match &result {
                Ok(outcome) => {
                    shared.incoming = false;
                    shared.problem = outcome.newer.then(|| tr!("cloud.problems.newer").into());
                    outcome.again
                }
                Err(SyncError::Api(ApiError::Unauthorized)) => {
                    shared.problem = Some(tr!("cloud.problems.signedOut").into());
                    false
                }
                Err(SyncError::Api(ApiError::Network(_))) => {
                    shared.problem = Some(tr!("cloud.problems.offline").into());
                    false
                }
                Err(SyncError::Api(ApiError::Status(QUOTA_EXCEEDED))) => {
                    shared.problem = Some(tr!("cloud.problems.full").into());
                    false
                }
                Err(err) => {
                    shared.problem = Some(trf!("cloud.problems.failed", error = err));
                    false
                }
            }
        };
        match result {
            Err(SyncError::Api(ApiError::Unauthorized)) => {
                log::warn!("the cloud no longer knows this computer, signing out");
                if let Err(err) = self.forget_account() {
                    log::warn!("could not forget the cloud account: {err}");
                }
            }
            Err(err) => log::warn!("sync with the cloud failed: {err}"),
            Ok(_) => {}
        }
        self.changed();
        self.refresh_pending();
        again
    }

    fn sign_in(&self, locale: &str) -> Result<(), CloudError> {
        let pending = sign_in::start(&self.url, locale)?;
        (self.open)(&pending.url)?;
        let code = pending.wait(&self.cancel, |ok| self.browser_page(ok))?;
        let answer =
            Api::new(&self.url, None).exchange(&code, &pending.verifier, &self.device_name)?;
        self.secrets.set(TOKEN_ACCOUNT, Some(&answer.token))?;
        let mut shared = self.lock();
        let paused = shared.state.paused;
        shared.state = SyncState {
            device_id: Some(answer.device_id),
            paused,
            ..SyncState::default()
        };
        shared.problem = None;
        self.file.save(&shared.state)?;
        log::info!("signed in to the cloud");
        Ok(())
    }

    fn forget_account(&self) -> io::Result<()> {
        self.secrets.set(TOKEN_ACCOUNT, None)?;
        let mut shared = self.lock();
        shared.state = SyncState {
            paused: shared.state.paused,
            ..SyncState::default()
        };
        shared.join = None;
        self.file.save(&shared.state)
    }

    pub(super) fn api(&self) -> Option<Api> {
        let token = self.secrets.get(TOKEN_ACCOUNT).ok().flatten()?;
        Some(Api::new(&self.url, Some(token)))
    }

    fn signed_in(&self) -> bool {
        self.secrets.exists(TOKEN_ACCOUNT).unwrap_or(false)
    }

    pub(super) fn synced_secrets(&self) -> io::Result<BTreeMap<String, String>> {
        let mut secrets = self.secrets.all()?;
        secrets.retain(|account, _| is_synced_account(account));
        Ok(secrets)
    }

    fn update(&self, change: impl FnOnce(&mut SyncState)) -> io::Result<()> {
        let result = {
            let mut shared = self.lock();
            change(&mut shared.state);
            self.file.save(&shared.state)
        };
        self.changed();
        result
    }

    fn status(&self) -> CloudStatus {
        let signed_in = self.signed_in();
        let shared = self.lock();
        CloudStatus {
            url: self.url.clone(),
            signed_in,
            signing_in: shared.signing_in,
            device_name: self.device_name.clone(),
            paused: shared.state.paused,
            syncing: shared.syncing,
            last_sync: shared.state.last_sync.clone(),
            problem: shared.problem.clone(),
            join: shared.join.clone().filter(|_| signed_in),
            pending: if signed_in {
                shared.pending.iter().copied().collect()
            } else {
                Vec::new()
            },
            outgoing: signed_in && (!shared.pending.is_empty() || shared.settings_pending),
            missing_plugins: if signed_in {
                shared.missing_plugins.clone()
            } else {
                Vec::new()
            },
            incoming: signed_in && shared.incoming,
            // Kept here only while the others keep their copy: it is here, not elsewhere.
            elsewhere: shared
                .state
                .ignored
                .iter()
                .filter(|(id, _)| !shared.local_only.contains(id))
                .map(|(id, name)| CloudProfile {
                    id: *id,
                    name: name.clone(),
                })
                .collect(),
        }
    }

    fn changed(&self) {
        (self.notify)(self.status());
    }

    fn browser_page(&self, ok: bool) -> String {
        let (title, body) = if ok {
            (
                tr!("cloud.browser.doneTitle").to_string(),
                trf!("cloud.browser.done", name = self.app_name).to_string(),
            )
        } else {
            (
                tr!("cloud.browser.deniedTitle").to_string(),
                trf!("cloud.browser.denied", name = self.app_name).to_string(),
            )
        };
        format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
             <style>body{{font-family:system-ui,sans-serif;background:#0a0a0a;color:#e5e5e5;\
             display:grid;place-items:center;height:100vh;margin:0}}main{{max-width:28rem;\
             text-align:center}}p{{color:#a3a3a3}}</style></head><body><main><h1>{title}</h1>\
             <p>{body}</p></main></body></html>",
            title = escape(&title),
            body = escape(&body),
        )
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
