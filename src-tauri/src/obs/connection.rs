//! The OBS client: a thread of its own that connects while some key needs OBS, keeps trying
//! while OBS is closed, runs commands one after another and reports OBS's states.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use log::{info, warn};
use tokio::sync::oneshot;
use uuid::Uuid;

use super::error::ObsError;
use super::events::{self, Effect};
use super::link::{Link, Versions};
use super::requests::{self, ItemIds, ObsList};
use super::status::{ObsFailure, ObsReport, ObsStates, ObsStatus, Wants};
use crate::config::ObsCommand;
use crate::secrets::Secrets;

const RETRY: Duration = Duration::from_secs(3);
/// So OBS does not log a failed login every few seconds.
const RETRY_AFTER_WRONG_PASSWORD: Duration = Duration::from_secs(30);
/// Asking to connect, from the settings or the editor, keeps the client at it this long even
/// without keys that use OBS.
const ASKED_FOR: Duration = Duration::from_secs(10 * 60);
/// Changing scene collection sends a burst of events; the states are read once it settles.
const REREAD_DELAY: Duration = Duration::from_millis(300);
/// An OBS on another computer can vanish without closing the connection; asking now and then
/// finds out.
const KEEP_ALIVE: Duration = Duration::from_secs(20);

type Reply<T> = oneshot::Sender<Result<T, ObsError>>;

enum Request {
    Want(Wants),
    Connect,
    Run(ObsCommand, Reply<()>),
    Names(ObsList, Reply<Vec<String>>),
    Password(Option<String>, Reply<()>),
}

/// Where the password of a server is kept among the secrets. The one OBS of before servers had
/// names, whose id is nil, keeps the account it had.
pub fn password_account(server: Uuid) -> String {
    if server.is_nil() {
        "obs".into()
    } else {
        format!("obs:{server}")
    }
}

/// Handle to the OBS worker thread. Dropping every copy stops the worker.
#[derive(Clone)]
pub struct Obs {
    requests: Sender<Request>,
}

impl Obs {
    /// `account` is where its password is kept among the secrets. `report` gets every change of
    /// the connection or of OBS's states; it returns false once nobody listens.
    pub fn start(
        secrets: Secrets,
        account: String,
        report: impl Fn(ObsReport) -> bool + Send + 'static,
    ) -> Self {
        let (requests, request_rx) = mpsc::channel();
        thread::Builder::new()
            .name("obs".into())
            .spawn(move || Worker::new(request_rx, Box::new(report), secrets, account).run())
            .expect("spawn obs worker");
        Self { requests }
    }

    pub fn want(&self, wants: Wants) {
        self.send(Request::Want(wants));
    }

    /// Tries now, and keeps trying for a while even if no key uses OBS.
    pub fn connect(&self) {
        self.send(Request::Connect);
    }

    /// Waits until OBS has done it, so the next action of a key happens after it.
    pub fn run(&self, command: &ObsCommand) -> Result<(), ObsError> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Run(command.clone(), reply));
        response.blocking_recv().map_err(|_| ObsError::Stopped)?
    }

    pub async fn names(&self, list: ObsList) -> Result<Vec<String>, ObsError> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Names(list, reply));
        response.await.map_err(|_| ObsError::Stopped)?
    }

    /// Stores it with the secrets and connects again with it; `None` or empty removes it.
    pub async fn set_password(&self, password: Option<String>) -> Result<(), ObsError> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Password(password, reply));
        response.await.map_err(|_| ObsError::Stopped)?
    }

    fn send(&self, request: Request) {
        if self.requests.send(request).is_err() {
            warn!("obs worker is not running");
        }
    }
}

enum Next {
    Stay,
    Reconnect,
    Stop,
}

struct Worker {
    requests: Receiver<Request>,
    report: Box<dyn Fn(ObsReport) -> bool + Send>,
    secrets: Secrets,
    account: String,
    wants: Wants,
    asked_until: Option<Instant>,
    /// Read from the secrets the first time OBS asks for it, then kept.
    password: Option<Option<String>>,
    current: ObsReport,
    reported: Option<ObsReport>,
    next_attempt: Instant,
}

impl Worker {
    fn new(
        requests: Receiver<Request>,
        report: Box<dyn Fn(ObsReport) -> bool + Send>,
        secrets: Secrets,
        account: String,
    ) -> Self {
        Self {
            requests,
            report,
            secrets,
            account,
            wants: Wants::default(),
            asked_until: None,
            password: None,
            current: ObsReport::default(),
            reported: None,
            next_attempt: Instant::now(),
        }
    }

    fn run(mut self) {
        self.current.info.password_set = self.secrets.exists(&self.account).unwrap_or_else(|err| {
            warn!("could not look for the OBS password: {err}");
            false
        });
        self.publish();
        loop {
            let request = if self.wanted() {
                if Instant::now() >= self.next_attempt {
                    if let Next::Stop = self.attempt() {
                        return;
                    }
                    continue;
                }
                let wait = self.next_attempt.saturating_duration_since(Instant::now());
                match self.requests.recv_timeout(wait) {
                    Ok(request) => request,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            } else {
                if self.current.info.status != ObsStatus::Off {
                    self.set_status(ObsStatus::Off);
                }
                match self.requests.recv() {
                    Ok(request) => request,
                    Err(_) => return,
                }
            };
            self.offline(request);
        }
    }

    fn wanted(&self) -> bool {
        self.wants.connect || self.asked_until.is_some_and(|until| Instant::now() < until)
    }

    fn attempt(&mut self) -> Next {
        if !matches!(self.current.info.status, ObsStatus::Failed { .. }) {
            self.set_status(ObsStatus::Connecting);
        }
        let (host, port) = (self.wants.host.clone(), self.wants.port);
        match Link::open(&host, port, || self.password()) {
            Ok((link, versions)) => {
                info!("connected to OBS {} at {host}:{port}", versions.obs);
                let next = self.session(link, versions);
                self.current.states = ObsStates::default();
                if self.wanted() {
                    self.set_status(ObsStatus::Connecting);
                }
                next
            }
            Err((reason, detail)) => {
                self.next_attempt = Instant::now()
                    + if reason == ObsFailure::WrongPassword {
                        RETRY_AFTER_WRONG_PASSWORD
                    } else {
                        RETRY
                    };
                self.set_status(ObsStatus::Failed { reason, detail });
                Next::Stay
            }
        }
    }

    /// Serves a connected OBS until the connection drops or should change.
    fn session(&mut self, mut link: Link, versions: Versions) -> Next {
        let mut ids = ItemIds::new();
        match requests::read_states(&mut link, &self.wants.watches, &mut ids) {
            Ok(states) => self.current.states = states,
            Err(err) => {
                warn!("could not read OBS's state: {err}");
                link.close();
                self.next_attempt = Instant::now() + RETRY;
                return Next::Reconnect;
            }
        }
        self.set_status(ObsStatus::Connected {
            obs_version: versions.obs,
            websocket_version: versions.websocket,
        });
        let mut reread_at: Option<Instant> = None;
        let mut checked = Instant::now();
        loop {
            if let Ok(Some(event)) = link.next_event() {
                checked = Instant::now();
                match events::apply(&mut self.current.states, &ids, &event) {
                    Effect::Changed => self.publish(),
                    Effect::Reread => {
                        self.publish();
                        reread_at.get_or_insert_with(|| Instant::now() + REREAD_DELAY);
                    }
                    Effect::None => {}
                }
            }
            if checked.elapsed() >= KEEP_ALIVE {
                checked = Instant::now();
                if link.call("GetVersion", serde_json::Value::Null).is_err() {
                    info!("OBS stopped answering");
                    link.close();
                    self.next_attempt = Instant::now();
                    return Next::Reconnect;
                }
            }
            if link.is_lost() {
                info!("OBS closed the connection");
                self.next_attempt = Instant::now();
                return Next::Reconnect;
            }
            loop {
                let next = match self.requests.try_recv() {
                    Ok(request) => self.online(&mut link, &mut ids, request),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => Next::Stop,
                };
                if !matches!(next, Next::Stay) {
                    link.close();
                    return next;
                }
            }
            if reread_at.is_some_and(|at| Instant::now() >= at) {
                reread_at = None;
                match requests::read_states(&mut link, &self.wants.watches, &mut ids) {
                    Ok(states) => {
                        self.current.states = states;
                        self.publish();
                    }
                    Err(err) => warn!("could not read OBS's state: {err}"),
                }
            }
            if !self.wanted() {
                link.close();
                return Next::Reconnect;
            }
        }
    }

    fn online(&mut self, link: &mut Link, ids: &mut ItemIds, request: Request) -> Next {
        match request {
            Request::Want(wants) => {
                let moved = (&wants.host, wants.port) != (&self.wants.host, self.wants.port);
                let watches_changed = wants.watches != self.wants.watches;
                self.wants = wants;
                if moved {
                    self.next_attempt = Instant::now();
                    return Next::Reconnect;
                }
                if watches_changed {
                    let watches = self.wants.watches.clone();
                    match requests::read_items(link, &watches, ids, &mut self.current.states) {
                        Ok(()) => self.publish(),
                        Err(err) => warn!("could not read OBS's state: {err}"),
                    }
                }
            }
            Request::Connect => self.asked_until = Some(Instant::now() + ASKED_FOR),
            Request::Run(command, reply) => {
                let _ = reply.send(requests::run(link, &command));
            }
            Request::Names(list, reply) => {
                let _ = reply.send(requests::names(link, &list));
            }
            Request::Password(password, reply) => {
                let result = self.store_password(password);
                let stored = result.is_ok();
                let _ = reply.send(result);
                if stored {
                    self.next_attempt = Instant::now();
                    return Next::Reconnect;
                }
            }
        }
        Next::Stay
    }

    fn offline(&mut self, request: Request) {
        match request {
            Request::Want(wants) => {
                let moved = (&wants.host, wants.port) != (&self.wants.host, self.wants.port);
                if moved || !self.wanted() {
                    self.next_attempt = Instant::now();
                }
                self.wants = wants;
            }
            Request::Connect => {
                self.asked_until = Some(Instant::now() + ASKED_FOR);
                self.next_attempt = Instant::now();
            }
            Request::Run(_, reply) => {
                let _ = reply.send(Err(ObsError::NotConnected));
            }
            Request::Names(_, reply) => {
                let _ = reply.send(Err(ObsError::NotConnected));
            }
            Request::Password(password, reply) => {
                let result = self.store_password(password);
                if result.is_ok() {
                    self.next_attempt = Instant::now();
                }
                let _ = reply.send(result);
            }
        }
    }

    fn password(&mut self) -> Option<String> {
        self.password
            .get_or_insert_with(|| {
                self.secrets.get(&self.account).unwrap_or_else(|err| {
                    warn!("could not read the OBS password: {err}");
                    None
                })
            })
            .clone()
    }

    fn store_password(&mut self, password: Option<String>) -> Result<(), ObsError> {
        let password = password.filter(|p| !p.is_empty());
        self.secrets.set(&self.account, password.as_deref())?;
        self.current.info.password_set = password.is_some();
        self.password = Some(password);
        self.publish();
        Ok(())
    }

    /// Logs only when the kind of status changes, not on every retry.
    fn set_status(&mut self, status: ObsStatus) {
        let before = &self.current.info.status;
        let changed = match (before, &status) {
            (ObsStatus::Failed { reason: a, .. }, ObsStatus::Failed { reason: b, .. }) => a != b,
            _ => std::mem::discriminant(before) != std::mem::discriminant(&status),
        };
        if changed {
            match &status {
                ObsStatus::Failed { detail, .. } => {
                    warn!("could not connect to OBS, will keep trying: {detail}");
                }
                ObsStatus::Off => info!("not connecting to OBS: no key uses it"),
                _ => {}
            }
        }
        self.current.info.status = status;
        self.publish();
    }

    fn publish(&mut self) {
        if self.reported.as_ref() == Some(&self.current) {
            return;
        }
        self.reported = Some(self.current.clone());
        (self.report)(self.current.clone());
    }
}
