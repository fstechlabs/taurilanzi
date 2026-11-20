//! The MCP server's life: off until the user turns it on, on this computer only, with a token
//! of its own that never leaves it (not even to the cloud).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::listener::{Listener, OnRequest, PATH};
use super::tools::Tools;
use crate::secrets::Secrets;

/// Where the token is kept with the secrets; sync leaves it on this computer.
pub const TOKEN_ACCOUNT: &str = "mcp:token";
const SETTINGS_FILE: &str = "mcp.json";
/// Another port, so the tests never meet the user's app.
const PORT_ENV: &str = "TAURILANZI_MCP_PORT";
/// Fixed, so what the assistants were told keeps working after a restart. Development builds
/// get their own, to run next to the released app.
const RELEASED_PORT: u16 = 3917;
const DEVELOPMENT_PORT: u16 = 3918;
const RELEASED_IDENTIFIER: &str = "com.taurilanzi.desktop";
const TOKEN_BYTES: usize = 32;
/// The window hears of the assistant's requests this often at most.
const USAGE_EVERY: Duration = Duration::from_secs(10);

/// How the MCP server is doing, for the window.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct McpStatus {
    pub enabled: bool,
    pub running: bool,
    pub url: String,
    /// What assistants send to be let in; only while it is on.
    pub token: Option<String>,
    /// The app's own program, which connects assistants that only start commands.
    pub bridge: String,
    /// Why it is on but not running.
    pub error: Option<String>,
    /// The `User-Agent` of the last assistant that called.
    pub last_client: Option<String>,
    pub last_used: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    enabled: bool,
}

pub struct McpServiceSetup {
    pub data_dir: PathBuf,
    /// Where assistants reach it; `port_for` gives the app's.
    pub port: u16,
    pub secrets: Secrets,
    pub tools: Arc<Tools>,
    pub notify: Box<dyn Fn(McpStatus) + Send + Sync>,
}

pub struct McpService {
    settings_path: PathBuf,
    secrets: Secrets,
    port: u16,
    tools: Arc<Tools>,
    notify: Box<dyn Fn(McpStatus) + Send + Sync>,
    state: tokio::sync::Mutex<State>,
    usage: Arc<Mutex<Usage>>,
}

#[derive(Default)]
struct State {
    enabled: bool,
    listener: Option<Listener>,
    error: Option<String>,
}

#[derive(Default)]
struct Usage {
    client: Option<String>,
    at: Option<jiff::Timestamp>,
    told: Option<Instant>,
}

impl McpService {
    /// Starts the server if the user left it on.
    pub fn start(setup: McpServiceSetup) -> Arc<Self> {
        let settings_path = setup.data_dir.join(SETTINGS_FILE);
        let enabled = std::fs::read(&settings_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
            .unwrap_or_default()
            .enabled;
        let service = Arc::new(Self {
            settings_path,
            secrets: setup.secrets,
            port: setup.port,
            tools: setup.tools,
            notify: setup.notify,
            state: tokio::sync::Mutex::new(State::default()),
            usage: Arc::default(),
        });
        if enabled {
            let starting = service.clone();
            tauri::async_runtime::spawn(async move {
                starting.set_enabled(true).await;
            });
        }
        service
    }

    pub async fn status(&self) -> McpStatus {
        let state = self.state.lock().await;
        self.status_of(&state)
    }

    pub async fn set_enabled(self: &Arc<Self>, enabled: bool) -> McpStatus {
        let mut state = self.state.lock().await;
        state.enabled = enabled;
        if let Err(err) = self.save_settings(enabled) {
            log::warn!("could not save whether the MCP server is on: {err}");
        }
        if let Some(listener) = state.listener.take() {
            listener.stop().await;
        }
        state.error = None;
        if enabled {
            self.listen(&mut state).await;
        }
        self.changed(&state)
    }

    /// A new token: assistants given the old one are let in no more.
    pub async fn regenerate_token(self: &Arc<Self>) -> McpStatus {
        let mut state = self.state.lock().await;
        if let Err(err) = self.secrets.set(TOKEN_ACCOUNT, Some(&new_token())) {
            state.error = Some(trf!("mcp.failed", error = err));
            return self.changed(&state);
        }
        if let Some(listener) = state.listener.take() {
            listener.stop().await;
        }
        if state.enabled {
            self.listen(&mut state).await;
        }
        self.changed(&state)
    }

    async fn listen(self: &Arc<Self>, state: &mut State) {
        let token = match self.token() {
            Ok(token) => token,
            Err(err) => {
                state.error = Some(trf!("mcp.failed", error = err));
                return;
            }
        };
        match Listener::start(self.port, &token, self.tools.clone(), self.on_request()).await {
            Ok(listener) => {
                log::info!("MCP server on http://127.0.0.1:{}{PATH}", self.port);
                state.listener = Some(listener);
            }
            Err(err) => {
                log::warn!("the MCP server could not listen on {}: {err}", self.port);
                state.error = Some(if err.kind() == std::io::ErrorKind::AddrInUse {
                    trf!("mcp.portTaken", port = self.port)
                } else {
                    trf!("mcp.failed", error = err)
                });
            }
        }
    }

    fn on_request(self: &Arc<Self>) -> OnRequest {
        let service = Arc::downgrade(self);
        Arc::new(move |client| {
            let Some(service) = service.upgrade() else {
                return;
            };
            let tell = {
                let mut usage = service.usage.lock().unwrap_or_else(PoisonError::into_inner);
                let new_client = client.is_some() && usage.client != client;
                usage.at = Some(jiff::Timestamp::now());
                if client.is_some() {
                    usage.client = client;
                }
                let due = usage.told.is_none_or(|at| at.elapsed() >= USAGE_EVERY);
                if new_client || due {
                    usage.told = Some(Instant::now());
                }
                new_client || due
            };
            if tell {
                tauri::async_runtime::spawn(async move {
                    let status = service.status().await;
                    (service.notify)(status);
                });
            }
        })
    }

    fn token(&self) -> std::io::Result<String> {
        if let Some(token) = self.secrets.get(TOKEN_ACCOUNT)? {
            return Ok(token);
        }
        let token = new_token();
        self.secrets.set(TOKEN_ACCOUNT, Some(&token))?;
        Ok(token)
    }

    fn save_settings(&self, enabled: bool) -> std::io::Result<()> {
        let json =
            serde_json::to_vec_pretty(&Settings { enabled }).map_err(std::io::Error::other)?;
        std::fs::write(&self.settings_path, json)
    }

    fn changed(&self, state: &State) -> McpStatus {
        let status = self.status_of(state);
        (self.notify)(status.clone());
        status
    }

    fn status_of(&self, state: &State) -> McpStatus {
        let usage = self.usage.lock().unwrap_or_else(PoisonError::into_inner);
        McpStatus {
            enabled: state.enabled,
            running: state.listener.is_some(),
            url: format!("http://127.0.0.1:{}{PATH}", self.port),
            token: state
                .enabled
                .then(|| self.secrets.get(TOKEN_ACCOUNT).ok().flatten())
                .flatten(),
            bridge: bridge_program(),
            error: state.error.clone(),
            last_client: usage.client.clone(),
            last_used: usage.at.map(|at| at.to_string()),
        }
    }
}

/// The program assistants start as the bridge. An AppImage runs from a folder mounted anew on
/// every launch, so it is the AppImage file itself, which `APPIMAGE` names, that stays put.
fn bridge_program() -> String {
    std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The port of this build: fixed, so what the assistants were told keeps working.
pub fn port_for(identifier: &str) -> u16 {
    std::env::var(PORT_ENV)
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(if identifier == RELEASED_IDENTIFIER {
            RELEASED_PORT
        } else {
            DEVELOPMENT_PORT
        })
}

fn new_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).expect("random bytes from the system");
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::time::Duration;

    use tauri::async_runtime::block_on;

    use super::*;

    fn service(dir: &std::path::Path, port: u16) -> Arc<McpService> {
        let (inputs, _) = std::sync::mpsc::channel();
        let (events, _) = tokio::sync::broadcast::channel(1);
        let store = crate::config::Store::open(dir.to_path_buf()).unwrap();
        McpService::start(McpServiceSetup {
            data_dir: dir.to_path_buf(),
            port,
            secrets: Secrets::new(dir, "com.taurilanzi.test"),
            tools: Arc::new(Tools {
                engine: crate::engine::EngineHandle::new(
                    inputs,
                    events,
                    Default::default(),
                    Default::default(),
                ),
                plugins: None,
                assets: store.assets.clone(),
                notify: Arc::new(|_| {}),
            }),
            notify: Box::new(|_| {}),
        })
    }

    /// The HTTP status a request with this token gets, or none if nothing listens.
    fn answer(port: u16, token: &str) -> Option<u16> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        agent
            .post(format!("http://127.0.0.1:{port}{PATH}"))
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
            .ok()
            .map(|response| response.status().as_u16())
    }

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[test]
    fn it_is_off_until_the_user_turns_it_on_and_remembers_it() {
        let dir = tempfile::tempdir().unwrap();
        let port = free_port();
        let mcp = service(dir.path(), port);

        let off = block_on(mcp.status());
        assert!(!off.enabled && !off.running && off.token.is_none());
        assert_eq!(answer(port, "anything"), None, "nothing listens while off");

        let on = block_on(mcp.set_enabled(true));
        assert!(on.enabled && on.running, "{:?}", on.error);
        let token = on.token.unwrap();
        assert_eq!(answer(port, &token), Some(200));
        assert_eq!(answer(port, "wrong"), Some(401));

        let renewed = block_on(mcp.regenerate_token()).token.unwrap();
        assert_ne!(renewed, token);
        assert_eq!(
            answer(port, &token),
            Some(401),
            "the old token is let in no more"
        );
        assert_eq!(answer(port, &renewed), Some(200));

        drop(mcp);
        let deadline = Instant::now() + Duration::from_secs(5);
        while answer(port, &renewed).is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let again = service(dir.path(), port);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !block_on(again.status()).running && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            block_on(again.status()).running,
            "on after a restart, as left"
        );

        let off = block_on(again.set_enabled(false));
        assert!(!off.running && off.token.is_none());
        assert_eq!(answer(port, &renewed), None);
        let settings: Settings =
            serde_json::from_slice(&std::fs::read(dir.path().join(SETTINGS_FILE)).unwrap())
                .unwrap();
        assert!(!settings.enabled);
    }

    #[test]
    fn a_taken_port_is_told_rather_than_moved() {
        let dir = tempfile::tempdir().unwrap();
        let taken = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = taken.local_addr().unwrap().port();
        let status = block_on(service(dir.path(), port).set_enabled(true));
        assert!(status.enabled && !status.running);
        assert!(status.error.unwrap().contains(&port.to_string()));
    }
}
