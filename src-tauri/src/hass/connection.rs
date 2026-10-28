//! The Home Assistant client: a thread of its own that connects while some key needs Home
//! Assistant, keeps trying while it cannot, runs commands one after another and reports the
//! states, pictures, histories and templates the keys show.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use jiff::{SignedDuration, Timestamp};
use log::{info, warn};
use serde_json::{Value, json};
use tokio::sync::oneshot;
use uuid::Uuid;

use super::address::Address;
use super::catalog::{self, HassCatalog};
use super::domains::{self, Plan};
use super::entities::{self, Entity, domain};
use super::error::HassError;
use super::link::{Link, SLOW_REQUEST_TIMEOUT};
use super::pictures::{Fetch, Fetched, Pictures};
use super::status::{
    Forecast, HassFailure, HassReport, HassStatus, HomeConfig, Point, Topic, Wants,
};
use crate::config::HassCommand;
use crate::i18n::Language;
use crate::secrets::Secrets;

const RETRY: Duration = Duration::from_secs(3);
/// Home Assistant can ban an address after failed logins, so a wrong token waits for a new one,
/// or a long while.
const RETRY_AFTER_WRONG_TOKEN: Duration = Duration::from_secs(10 * 60);
/// Asking to connect, from the settings or the editor, keeps the client at it this long even
/// without keys that use Home Assistant.
const ASKED_FOR: Duration = Duration::from_secs(10 * 60);
/// Home Assistant elsewhere can vanish without closing the connection; asking now and then
/// finds out.
const KEEP_ALIVE: Duration = Duration::from_secs(30);
const MIN_PICTURE_SECONDS: u32 = 2;
/// The event automations of Home Assistant fire to move the dock.
const REMOTE_EVENT: &str = "taurilanzi";

type Reply<T> = oneshot::Sender<Result<T, HassError>>;

enum Request {
    Want(Wants),
    Connect,
    Run(HassCommand, Reply<()>),
    Catalog(Reply<HassCatalog>),
    Token(Option<String>, Reply<()>),
    Code {
        entity: String,
        code: Option<String>,
        reply: Reply<()>,
    },
    CodeSaved(String, Reply<bool>),
}

/// Handle to the Home Assistant worker thread. Dropping every copy stops the worker.
#[derive(Clone)]
pub struct Hass {
    requests: Sender<Request>,
}

impl Hass {
    /// `report` gets every change of the connection or of what the keys show, and `remote` the
    /// data of each `taurilanzi` event fired in Home Assistant; they return false once nobody
    /// listens.
    pub fn start(
        secrets: Secrets,
        server: Uuid,
        report: impl Fn(HassReport) -> bool + Send + 'static,
        remote: impl Fn(Value) -> bool + Send + 'static,
    ) -> Self {
        let (requests, request_rx) = mpsc::channel();
        thread::Builder::new()
            .name("hass".into())
            .spawn(move || {
                Worker::new(
                    request_rx,
                    Box::new(report),
                    Box::new(remote),
                    secrets,
                    server,
                )
                .run()
            })
            .expect("spawn the Home Assistant worker");
        Self { requests }
    }

    pub fn want(&self, wants: Wants) {
        self.send(Request::Want(wants));
    }

    /// Tries now, and keeps trying for a while even if no key uses Home Assistant.
    pub fn connect(&self) {
        self.send(Request::Connect);
    }

    /// Waits until Home Assistant has done it, so the next action of a key happens after it.
    pub fn run(&self, command: &HassCommand) -> Result<(), HassError> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Run(command.clone(), reply));
        response.blocking_recv().map_err(|_| HassError::Stopped)?
    }

    pub async fn catalog(&self) -> Result<HassCatalog, HassError> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Catalog(reply));
        response.await.map_err(|_| HassError::Stopped)?
    }

    /// Stores it with the secrets and connects again with it; `None` or empty removes it.
    pub async fn set_token(&self, token: Option<String>) -> Result<(), HassError> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Token(token, reply));
        response.await.map_err(|_| HassError::Stopped)?
    }

    /// The code of an alarm panel or a lock, kept with the secrets; `None` or empty removes it.
    pub async fn set_code(&self, entity: String, code: Option<String>) -> Result<(), HassError> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Code {
            entity,
            code,
            reply,
        });
        response.await.map_err(|_| HassError::Stopped)?
    }

    pub async fn code_saved(&self, entity: String) -> Result<bool, HassError> {
        let (reply, response) = oneshot::channel();
        self.send(Request::CodeSaved(entity, reply));
        response.await.map_err(|_| HassError::Stopped)?
    }

    fn send(&self, request: Request) {
        if self.requests.send(request).is_err() {
            warn!("the Home Assistant worker is not running");
        }
    }
}

enum Next {
    Stay,
    Reconnect,
    Stop,
}

/// What one connection subscribed to and fetched, to change only what the wants change.
struct Session {
    link: Link,
    address: Address,
    entities: Option<(u64, BTreeSet<String>)>,
    remote: Option<u64>,
    templates: BTreeMap<String, u64>,
    forecasts: BTreeMap<String, u64>,
    histories: BTreeMap<String, u32>,
    /// The address of the last picture asked for, by entity.
    picture_urls: BTreeMap<String, String>,
    /// When each camera's picture is next taken.
    cameras_due: BTreeMap<String, Instant>,
    language: Option<Language>,
}

struct Worker {
    requests: Receiver<Request>,
    report: Box<dyn Fn(HassReport) -> bool + Send>,
    remote: Box<dyn Fn(Value) -> bool + Send>,
    secrets: Secrets,
    server: Uuid,
    wants: Wants,
    asked_until: Option<Instant>,
    /// Read from the secrets the first time it is needed, then kept.
    token: Option<Option<String>>,
    current: HassReport,
    reported: Option<HassReport>,
    next_attempt: Instant,
    pictures: Pictures,
    fetched: Receiver<(String, Fetched)>,
}

impl Worker {
    fn new(
        requests: Receiver<Request>,
        report: Box<dyn Fn(HassReport) -> bool + Send>,
        remote: Box<dyn Fn(Value) -> bool + Send>,
        secrets: Secrets,
        server: Uuid,
    ) -> Self {
        let (fetched_tx, fetched) = mpsc::channel();
        let pictures =
            Pictures::start(move |entity, picture| fetched_tx.send((entity, picture)).is_ok());
        Self {
            requests,
            report,
            remote,
            secrets,
            server,
            wants: Wants::default(),
            asked_until: None,
            token: None,
            current: HassReport::default(),
            reported: None,
            next_attempt: Instant::now(),
            pictures,
            fetched,
        }
    }

    fn run(mut self) {
        self.current.info.token_set =
            self.secrets
                .exists(&self.token_account())
                .unwrap_or_else(|err| {
                    warn!("could not look for the Home Assistant token: {err}");
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
                if self.current.info.status != HassStatus::Off {
                    self.set_status(HassStatus::Off);
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
        let Some(address) = Address::parse(&self.wants.url) else {
            self.next_attempt = Instant::now() + RETRY_AFTER_WRONG_TOKEN;
            self.set_status(HassStatus::Failed {
                reason: HassFailure::NoAddress,
                detail: "no address".into(),
            });
            return Next::Stay;
        };
        if !matches!(self.current.info.status, HassStatus::Failed { .. }) {
            self.set_status(HassStatus::Connecting);
        }
        let insecure = self.wants.insecure;
        match Link::open(&address, insecure, || self.token()) {
            Ok((link, version)) => {
                info!(
                    "connected to Home Assistant {version} at {}",
                    address.origin()
                );
                let next = self.session(link, address, version);
                self.forget_session();
                if self.wanted() {
                    self.set_status(HassStatus::Connecting);
                }
                next
            }
            Err((reason, detail)) => {
                self.next_attempt = Instant::now()
                    + match reason {
                        HassFailure::WrongToken | HassFailure::TokenRequired => {
                            RETRY_AFTER_WRONG_TOKEN
                        }
                        _ => RETRY,
                    };
                self.set_status(HassStatus::Failed { reason, detail });
                Next::Stay
            }
        }
    }

    /// What a connection read goes with it: keys show nothing of Home Assistant while it is away.
    fn forget_session(&mut self) {
        let data = &mut self.current.data;
        data.entities = Default::default();
        data.history = Default::default();
        data.templates = Default::default();
        data.forecasts = Default::default();
        data.pictures = Default::default();
        self.current.changed.insert(Topic::Everything);
    }

    /// Serves a connected Home Assistant until the connection drops or should change.
    fn session(&mut self, link: Link, address: Address, version: String) -> Next {
        let mut session = Session {
            link,
            address,
            entities: None,
            remote: None,
            templates: BTreeMap::new(),
            forecasts: BTreeMap::new(),
            histories: BTreeMap::new(),
            picture_urls: BTreeMap::new(),
            cameras_due: BTreeMap::new(),
            language: None,
        };
        let home = match session.link.call("get_config", Value::Null) {
            Ok(config) => HomeConfig {
                temperature_unit: config["unit_system"]["temperature"]
                    .as_str()
                    .unwrap_or("°C")
                    .to_string(),
                location: config["location_name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                version: version.clone(),
            },
            Err(err) => {
                warn!("could not read Home Assistant's settings: {err}");
                session.link.close();
                self.next_attempt = Instant::now() + RETRY;
                return Next::Reconnect;
            }
        };
        let location = home.location.clone();
        self.current.data.config = Arc::new(home);
        self.load_texts(&mut session);
        if let Err(err) = self.sync(&mut session) {
            warn!("could not follow Home Assistant: {err}");
            session.link.close();
            self.next_attempt = Instant::now() + RETRY;
            return Next::Reconnect;
        }
        // Only an admin can follow events of other kinds; this one is open to every user.
        match session
            .link
            .subscribe("subscribe_events", json!({ "event_type": REMOTE_EVENT }))
        {
            Ok(id) => session.remote = Some(id),
            Err(err) => warn!("Home Assistant cannot move the dock: {err}"),
        }
        self.current.changed.insert(Topic::Everything);
        self.set_status(HassStatus::Connected { version, location });

        let mut checked = Instant::now();
        loop {
            match session.link.next_event() {
                Ok(Some((id, event))) => {
                    checked = Instant::now();
                    self.on_event(&mut session, id, &event);
                    self.publish();
                }
                Ok(None) => {}
                Err(_) => {}
            }
            if checked.elapsed() >= KEEP_ALIVE {
                checked = Instant::now();
                if session.link.ping().is_err() {
                    info!("Home Assistant stopped answering");
                    session.link.close();
                    self.next_attempt = Instant::now();
                    return Next::Reconnect;
                }
            }
            if session.link.is_lost() {
                info!("Home Assistant closed the connection");
                self.next_attempt = Instant::now() + RETRY;
                return Next::Reconnect;
            }
            loop {
                let next = match self.requests.try_recv() {
                    Ok(request) => self.online(&mut session, request),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => Next::Stop,
                };
                if !matches!(next, Next::Stay) {
                    session.link.close();
                    return next;
                }
            }
            while let Ok((entity, picture)) = self.fetched.try_recv() {
                self.on_picture(&entity, picture);
            }
            self.take_due_pictures(&mut session);
            self.publish();
            if !self.wanted() {
                session.link.close();
                return Next::Reconnect;
            }
        }
    }

    fn language(&self) -> &'static str {
        match self.wants.language.unwrap_or_else(crate::i18n::current) {
            Language::Es => "es",
            Language::En => "en",
        }
    }

    fn load_texts(&mut self, session: &mut Session) {
        let language = self.language();
        match catalog::state_texts(&mut session.link, language) {
            Ok(labels) => self.current.data.labels = Arc::new(labels),
            Err(err) => warn!("could not read Home Assistant's texts: {err}"),
        }
        match catalog::registry(&mut session.link) {
            Ok(registry) => self.current.data.registry = Arc::new(registry),
            Err(err) => warn!("could not read Home Assistant's entity registry: {err}"),
        }
        session.language = self.wants.language;
        self.current.changed.insert(Topic::Everything);
    }

    /// Subscribes to, and fetches, what the wants ask for and was not yet; drops the rest.
    fn sync(&mut self, session: &mut Session) -> Result<(), HassError> {
        if session.language != self.wants.language {
            self.load_texts(session);
        }
        let wanted = self.wants.entities.clone();
        if session.entities.as_ref().map(|(_, e)| e) != Some(&wanted) {
            if let Some((id, _)) = session.entities.take() {
                session.link.unsubscribe(id)?;
            }
            Arc::make_mut(&mut self.current.data.entities).retain(|id, _| wanted.contains(id));
            if !wanted.is_empty() {
                let id = session.link.subscribe(
                    "subscribe_entities",
                    json!({ "entity_ids": wanted.iter().collect::<Vec<_>>() }),
                )?;
                session.entities = Some((id, wanted));
            }
        }

        let templates = self.wants.templates.clone();
        let gone: Vec<String> = session
            .templates
            .keys()
            .filter(|t| !templates.contains(*t))
            .cloned()
            .collect();
        for template in gone {
            if let Some(id) = session.templates.remove(&template) {
                session.link.unsubscribe(id)?;
            }
            Arc::make_mut(&mut self.current.data.templates).remove(&template);
            self.current.changed.insert(Topic::Template(template));
        }
        for template in templates {
            if session.templates.contains_key(&template) {
                continue;
            }
            match session.link.subscribe(
                "render_template",
                json!({ "template": template, "report_errors": true }),
            ) {
                Ok(id) => {
                    session.templates.insert(template, id);
                }
                Err(err) => {
                    Arc::make_mut(&mut self.current.data.templates)
                        .insert(template.clone(), Err(err.to_string()));
                    self.current.changed.insert(Topic::Template(template));
                }
            }
        }

        let forecasts = self.wants.forecasts.clone();
        let gone: Vec<String> = session
            .forecasts
            .keys()
            .filter(|e| !forecasts.contains(*e))
            .cloned()
            .collect();
        for entity in gone {
            if let Some(id) = session.forecasts.remove(&entity) {
                session.link.unsubscribe(id)?;
            }
            Arc::make_mut(&mut self.current.data.forecasts).remove(&entity);
        }
        for entity in forecasts {
            if session.forecasts.contains_key(&entity) {
                continue;
            }
            // Not every weather integration gives a daily forecast; it then shows none.
            match session.link.subscribe(
                "weather/subscribe_forecast",
                json!({ "entity_id": entity, "forecast_type": "daily" }),
            ) {
                Ok(id) => {
                    session.forecasts.insert(entity, id);
                }
                Err(err) => info!("no forecast for {entity}: {err}"),
            }
        }

        let histories = self.wants.histories.clone();
        session
            .histories
            .retain(|entity, _| histories.contains_key(entity));
        Arc::make_mut(&mut self.current.data.history)
            .retain(|entity, _| histories.contains_key(entity));
        for (entity, hours) in histories {
            if session.histories.get(&entity) == Some(&hours) {
                continue;
            }
            match read_history(&mut session.link, &entity, hours) {
                Ok(points) => {
                    Arc::make_mut(&mut self.current.data.history)
                        .insert(entity.clone(), Arc::new(points));
                    self.current.changed.insert(Topic::History(entity.clone()));
                }
                Err(err) => warn!("could not read the history of {entity}: {err}"),
            }
            session.histories.insert(entity, hours);
        }

        let pictures = self.wants.pictures.clone();
        session
            .picture_urls
            .retain(|entity, _| pictures.contains_key(entity));
        session
            .cameras_due
            .retain(|entity, _| pictures.contains_key(entity));
        Arc::make_mut(&mut self.current.data.pictures)
            .retain(|entity, _| pictures.contains_key(entity));
        for entity in pictures.keys() {
            if domain(entity) == "camera" {
                session
                    .cameras_due
                    .entry(entity.clone())
                    .or_insert_with(Instant::now);
            } else {
                self.check_picture(session, entity);
            }
        }
        Ok(())
    }

    fn on_event(&mut self, session: &mut Session, id: u64, event: &Value) {
        if session.remote == Some(id) {
            (self.remote)(event["data"].clone());
            return;
        }
        if session.entities.as_ref().is_some_and(|(sub, _)| *sub == id) {
            let changed = entities::apply(Arc::make_mut(&mut self.current.data.entities), event);
            for entity in changed {
                self.record_history(&entity);
                if self.wants.pictures.contains_key(&entity) && domain(&entity) != "camera" {
                    self.check_picture(session, &entity);
                }
                self.current.changed.insert(Topic::Entity(entity));
            }
            return;
        }
        if let Some(template) = session
            .templates
            .iter()
            .find(|(_, sub)| **sub == id)
            .map(|(t, _)| t.clone())
        {
            let result = match (&event["result"], event["error"].as_str()) {
                (_, Some(error)) => Err(error.to_string()),
                (Value::String(text), _) => Ok(text.clone()),
                (Value::Null, _) => Ok(String::new()),
                (other, _) => Ok(other.to_string()),
            };
            let templates = Arc::make_mut(&mut self.current.data.templates);
            if templates.get(&template) != Some(&result) {
                templates.insert(template.clone(), result);
                self.current.changed.insert(Topic::Template(template));
            }
            return;
        }
        if let Some(entity) = session
            .forecasts
            .iter()
            .find(|(_, sub)| **sub == id)
            .map(|(e, _)| e.clone())
        {
            let days = parse_forecast(&event["forecast"]);
            Arc::make_mut(&mut self.current.data.forecasts).insert(entity.clone(), Arc::new(days));
            self.current.changed.insert(Topic::Forecast(entity));
        }
    }

    /// Adds the new value of a graphed entity, and lets go of what falls out of its window.
    fn record_history(&mut self, entity: &str) {
        let Some(hours) = self.wants.histories.get(entity) else {
            return;
        };
        let Some(state) = self.current.data.entities.get(entity) else {
            return;
        };
        let Some(value) = state.value() else {
            return;
        };
        let at = state.last_updated;
        let start = now_seconds() - f64::from(*hours) * 3600.0;
        let history = Arc::make_mut(&mut self.current.data.history);
        let points = Arc::make_mut(history.entry(entity.to_string()).or_default());
        if points
            .last()
            .is_some_and(|(last, v)| *last == at && *v == value)
        {
            return;
        }
        points.push((at, value));
        // One point before the window keeps the line from starting in the middle.
        let first = points
            .iter()
            .position(|(t, _)| *t >= start)
            .unwrap_or(points.len());
        if first > 1 {
            points.drain(..first - 1);
        }
        self.current
            .changed
            .insert(Topic::History(entity.to_string()));
    }

    /// Asks for an entity's picture if it points at a new one.
    fn check_picture(&mut self, session: &mut Session, entity: &str) {
        let Some(url) = self
            .current
            .data
            .entities
            .get(entity)
            .and_then(|e| e.text("entity_picture"))
            .map(str::to_string)
        else {
            return;
        };
        if session.picture_urls.get(entity) == Some(&url) {
            return;
        }
        session.picture_urls.insert(entity.to_string(), url.clone());
        self.fetch_picture(session, entity, &url);
    }

    fn take_due_pictures(&mut self, session: &mut Session) {
        let now = Instant::now();
        let due: Vec<String> = session
            .cameras_due
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(e, _)| e.clone())
            .collect();
        for entity in due {
            let seconds = self
                .wants
                .pictures
                .get(&entity)
                .copied()
                .unwrap_or(10)
                .max(MIN_PICTURE_SECONDS);
            session.cameras_due.insert(
                entity.clone(),
                now + Duration::from_secs(u64::from(seconds)),
            );
            let url = format!("/api/camera_proxy/{entity}");
            self.fetch_picture(session, &entity, &url);
        }
    }

    fn fetch_picture(&mut self, session: &Session, entity: &str, url: &str) {
        let absolute = if url.starts_with('/') {
            session.address.web(url)
        } else {
            url.to_string()
        };
        let token = if session.address.owns(&absolute) {
            self.token()
        } else {
            None
        };
        self.pictures.fetch(Fetch {
            entity: entity.to_string(),
            url: absolute,
            token,
            insecure: self.wants.insecure,
        });
    }

    fn on_picture(&mut self, entity: &str, picture: Fetched) {
        if !self.wants.pictures.contains_key(entity) {
            return;
        }
        match picture {
            Ok(image) => {
                let pictures = Arc::make_mut(&mut self.current.data.pictures);
                if pictures.get(entity) != Some(&image) {
                    pictures.insert(entity.to_string(), image);
                    self.current
                        .changed
                        .insert(Topic::Picture(entity.to_string()));
                }
            }
            Err(err) => warn!("could not get the picture of {entity}: {err}"),
        }
    }

    fn online(&mut self, session: &mut Session, request: Request) -> Next {
        match request {
            Request::Want(wants) => {
                let moved = (&wants.url, wants.insecure) != (&self.wants.url, self.wants.insecure);
                self.wants = wants;
                if moved {
                    self.next_attempt = Instant::now();
                    return Next::Reconnect;
                }
                if let Err(err) = self.sync(session) {
                    warn!("could not follow Home Assistant: {err}");
                    self.next_attempt = Instant::now() + RETRY;
                    return Next::Reconnect;
                }
            }
            Request::Connect => self.asked_until = Some(Instant::now() + ASKED_FOR),
            Request::Run(command, reply) => {
                let result = self.execute(session, &command);
                if let Err(err) = &result {
                    info!("Home Assistant could not do it: {err}");
                }
                let _ = reply.send(result);
            }
            Request::Catalog(reply) => {
                let language = self.language();
                let origin = session.address.origin();
                let result = catalog::read(&mut session.link, language, origin);
                // The editor may be about to use what the user just renamed or translated.
                if let Ok(registry) = catalog::registry(&mut session.link) {
                    self.current.data.registry = Arc::new(registry);
                    self.current.changed.insert(Topic::Everything);
                }
                let _ = reply.send(result);
            }
            Request::Token(token, reply) => {
                let result = self.store_token(token);
                let stored = result.is_ok();
                let _ = reply.send(result);
                if stored {
                    self.next_attempt = Instant::now();
                    return Next::Reconnect;
                }
            }
            Request::Code {
                entity,
                code,
                reply,
            } => {
                let _ = reply.send(self.store_code(&entity, code));
            }
            Request::CodeSaved(entity, reply) => {
                let _ = reply.send(self.code_saved(&entity));
            }
        }
        Next::Stay
    }

    fn offline(&mut self, request: Request) {
        match request {
            Request::Want(wants) => {
                let moved = (&wants.url, wants.insecure) != (&self.wants.url, self.wants.insecure);
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
                let _ = reply.send(Err(HassError::NotConnected));
            }
            Request::Catalog(reply) => {
                let _ = reply.send(Err(HassError::NotConnected));
            }
            Request::Token(token, reply) => {
                let result = self.store_token(token);
                if result.is_ok() {
                    self.next_attempt = Instant::now();
                }
                let _ = reply.send(result);
            }
            Request::Code {
                entity,
                code,
                reply,
            } => {
                let _ = reply.send(self.store_code(&entity, code));
            }
            Request::CodeSaved(entity, reply) => {
                let _ = reply.send(self.code_saved(&entity));
            }
        }
    }

    fn execute(&mut self, session: &mut Session, command: &HassCommand) -> Result<(), HassError> {
        let about = match command {
            HassCommand::Call { .. } => None,
            _ => command.entity(),
        };
        let entity = match about {
            Some(id) => match self.current.data.entities.get(id) {
                Some(entity) => Some(entity.clone()),
                None => read_entity(&mut session.link, id)?,
            },
            None => None,
        };
        let code = command
            .entity()
            .filter(|id| matches!(domain(id), "lock" | "alarm_control_panel"))
            .and_then(|id| {
                self.secrets
                    .get(&self.code_account(id))
                    .unwrap_or_else(|err| {
                        warn!("could not read the code of {id}: {err}");
                        None
                    })
            });
        let plan = domains::plan(command, entity.as_ref(), code.as_deref())?;
        match &plan {
            Plan::Service { .. } => {
                let mut message = plan.message().unwrap_or_default();
                match session.link.call("call_service", message.clone()) {
                    // Some actions only run when asked for what they answer.
                    Err(HassError::Rejected { message: text, .. })
                        if text.contains("return_response") || text.contains("responses") =>
                    {
                        message["return_response"] = true.into();
                        session.link.call("call_service", message).map(|_| ())
                    }
                    other => other.map(|_| ()),
                }
            }
            Plan::Assist { text, agent } => {
                let mut fields = json!({ "text": text });
                if !agent.is_empty() {
                    fields["agent_id"] = agent.clone().into();
                }
                let result = session.link.call("conversation/process", fields)?;
                let response = &result["response"];
                if response["response_type"] == "error" {
                    return Err(HassError::Rejected {
                        code: "assist".into(),
                        message: response["speech"]["plain"]["speech"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                    });
                }
                Ok(())
            }
            Plan::Event { event, data } => session
                .link
                .call(
                    "fire_event",
                    json!({ "event_type": event, "event_data": data }),
                )
                .map(|_| ()),
            Plan::Nothing => Ok(()),
        }
    }

    fn token_account(&self) -> String {
        token_account(self.server)
    }

    fn code_account(&self, entity: &str) -> String {
        code_account(self.server, entity)
    }

    fn token(&mut self) -> Option<String> {
        let account = self.token_account();
        self.token
            .get_or_insert_with(|| {
                self.secrets.get(&account).unwrap_or_else(|err| {
                    warn!("could not read the Home Assistant token: {err}");
                    None
                })
            })
            .clone()
    }

    fn store_token(&mut self, token: Option<String>) -> Result<(), HassError> {
        let token = token
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        self.secrets.set(&self.token_account(), token.as_deref())?;
        self.current.info.token_set = token.is_some();
        self.token = Some(token);
        self.publish();
        Ok(())
    }

    fn store_code(&mut self, entity: &str, code: Option<String>) -> Result<(), HassError> {
        let code = code.filter(|c| !c.is_empty());
        self.secrets
            .set(&self.code_account(entity), code.as_deref())?;
        Ok(())
    }

    fn code_saved(&self, entity: &str) -> Result<bool, HassError> {
        Ok(self.secrets.exists(&self.code_account(entity))?)
    }

    /// Logs only when the kind of status changes, not on every retry.
    fn set_status(&mut self, status: HassStatus) {
        let before = &self.current.info.status;
        let changed = match (before, &status) {
            (HassStatus::Failed { reason: a, .. }, HassStatus::Failed { reason: b, .. }) => a != b,
            _ => std::mem::discriminant(before) != std::mem::discriminant(&status),
        };
        if changed {
            match &status {
                HassStatus::Failed { detail, .. } => {
                    warn!("could not connect to Home Assistant, will keep trying: {detail}");
                }
                HassStatus::Off => info!("not connecting to Home Assistant: no key uses it"),
                _ => {}
            }
        }
        self.current.info.status = status;
        self.publish();
    }

    fn publish(&mut self) {
        if let Some(reported) = &self.reported
            && reported.info == self.current.info
            && reported.data == self.current.data
            && self.current.changed.is_empty()
        {
            return;
        }
        let report = HassReport {
            info: self.current.info.clone(),
            data: self.current.data.clone(),
            changed: std::mem::take(&mut self.current.changed),
        };
        self.reported = Some(report.clone());
        (self.report)(report);
    }
}

/// Where the token of a server is kept among the secrets.
pub fn token_account(server: Uuid) -> String {
    format!("home-assistant:{server}")
}

/// Where the code of an alarm panel or a lock of a server is kept among the secrets.
pub fn code_account(server: Uuid, entity: &str) -> String {
    format!("home-assistant-code:{server}:{entity}")
}

fn now_seconds() -> f64 {
    Timestamp::now().as_millisecond() as f64 / 1000.0
}

/// One entity's state, for a command on an entity no key follows yet.
fn read_entity(link: &mut Link, id: &str) -> Result<Option<Entity>, HassError> {
    let states = link.call_within("get_states", Value::Null, SLOW_REQUEST_TIMEOUT)?;
    Ok(states
        .as_array()
        .into_iter()
        .flatten()
        .filter(|state| state["entity_id"] == id)
        .find_map(Entity::from_full)
        .map(|(_, entity)| entity))
}

fn read_history(link: &mut Link, entity: &str, hours: u32) -> Result<Vec<Point>, HassError> {
    let start = Timestamp::now()
        .checked_sub(SignedDuration::from_hours(i64::from(hours)))
        .unwrap_or_else(|_| Timestamp::now());
    let result = link.call_within(
        "history/history_during_period",
        json!({
            "start_time": start.to_string(),
            "entity_ids": [entity],
            "minimal_response": true,
            "no_attributes": true,
            "significant_changes_only": false,
        }),
        SLOW_REQUEST_TIMEOUT,
    )?;
    Ok(result[entity]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|point| {
            let value = point["s"].as_str()?.parse::<f64>().ok()?;
            let at = point["lu"].as_f64().or_else(|| point["lc"].as_f64())?;
            value.is_finite().then_some((at, value))
        })
        .collect())
}

fn parse_forecast(list: &Value) -> Vec<Forecast> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|day| {
            let at = day["datetime"].as_str()?.parse::<Timestamp>().ok()?;
            Some(Forecast {
                at: at.as_millisecond() as f64 / 1000.0,
                condition: day["condition"].as_str().unwrap_or_default().to_string(),
                temperature: day["temperature"].as_f64(),
                low: day["templow"].as_f64(),
                precipitation_probability: day["precipitation_probability"].as_f64(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_forecasts() {
        let days = parse_forecast(&json!([
            { "datetime": "2026-10-26T16:00:43+00:00", "condition": "rainy", "temperature": 22.0, "templow": 15.0, "precipitation_probability": 60 },
            { "datetime": "not a date", "condition": "sunny" },
            { "datetime": "2026-10-27T16:00:43+00:00", "condition": "cloudy" },
        ]));
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].condition, "rainy");
        assert_eq!((days[0].temperature, days[0].low), (Some(22.0), Some(15.0)));
        assert_eq!(days[0].precipitation_probability, Some(60.0));
        assert_eq!(days[1].temperature, None);
    }
}
