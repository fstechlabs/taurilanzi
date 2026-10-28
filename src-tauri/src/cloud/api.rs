//! The cloud's API for the desktop app: signing in a computer and syncing its documents.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ureq::typestate::WithoutBody;
use ureq::{Agent, RequestBuilder};
use uuid::Uuid;

use super::documents::Kind;

const ANSWER_LIMIT: u64 = 16 * 1024 * 1024;
const ASSET_LIMIT: u64 = 32 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The token is no longer valid: the computer was disconnected from the web.
    #[error("the cloud no longer knows this computer")]
    Unauthorized,
    #[error("the cloud answered {0}")]
    Status(u16),
    #[error("could not reach the cloud: {0}")]
    Network(String),
    #[error("the cloud answered something unexpected: {0}")]
    Invalid(String),
}

impl From<ureq::Error> for ApiError {
    fn from(err: ureq::Error) -> Self {
        match err {
            ureq::Error::StatusCode(401) => Self::Unauthorized,
            ureq::Error::StatusCode(code) => Self::Status(code),
            other => Self::Network(other.to_string()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RemoteState {
    pub documents: Vec<RemoteDocument>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteDocument {
    pub id: Uuid,
    pub kind: Kind,
    pub head: Head,
}

/// The current version of a document in the cloud.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Head {
    pub version: Uuid,
    pub edited_at: String,
    pub deleted: bool,
}

#[derive(Debug, Deserialize)]
pub struct RemoteVersion {
    pub content: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct NewVersion<'a> {
    pub kind: Kind,
    pub version: Uuid,
    pub parent: Option<Uuid>,
    pub edited_at: String,
    pub deleted: bool,
    pub content: Option<&'a Value>,
}

#[derive(Debug, Deserialize)]
pub struct Pushed {
    pub accepted: bool,
    pub head: Head,
}

#[derive(Debug, Serialize)]
struct TokenRequest<'a> {
    code: &'a str,
    verifier: &'a str,
    device_name: &'a str,
    platform: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct TokenAnswer {
    pub token: String,
    pub device_id: Uuid,
}

#[derive(Serialize)]
struct Ids<'a> {
    ids: &'a [String],
}

#[derive(Deserialize)]
struct Missing {
    missing: Vec<String>,
}

pub struct Api {
    agent: Agent,
    base: String,
    token: Option<String>,
}

impl Api {
    pub fn new(base: &str, token: Option<String>) -> Self {
        let agent: Agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(120)))
            .timeout_connect(Some(Duration::from_secs(10)))
            .user_agent(crate::version::user_agent())
            .build()
            .into();
        Self {
            agent,
            base: base.trim_end_matches('/').to_owned(),
            token,
        }
    }

    pub fn exchange(
        &self,
        code: &str,
        verifier: &str,
        device_name: &str,
    ) -> Result<TokenAnswer, ApiError> {
        let body = TokenRequest {
            code,
            verifier,
            device_name,
            platform: platform(),
        };
        self.post("/api/desktop/token", &body)
    }

    pub fn revoke(&self) -> Result<(), ApiError> {
        self.authorized(self.agent.delete(self.url("/api/desktop/token")))
            .call()
            .map(drop)
            .map_err(Into::into)
    }

    pub fn state(&self) -> Result<RemoteState, ApiError> {
        let mut answer = self.get("/api/sync/state").call()?;
        read_json(answer.body_mut())
    }

    pub fn version(&self, document: Uuid, version: Uuid) -> Result<RemoteVersion, ApiError> {
        let path = format!("/api/sync/documents/{document}/versions/{version}");
        let mut answer = self.get(&path).call()?;
        read_json(answer.body_mut())
    }

    pub fn push(&self, document: Uuid, version: &NewVersion) -> Result<Pushed, ApiError> {
        self.post(&format!("/api/sync/documents/{document}/versions"), version)
    }

    /// The images and sounds, out of `ids`, that the cloud does not have yet.
    pub fn missing_assets(&self, ids: &[String]) -> Result<Vec<String>, ApiError> {
        let answer: Missing = self.post("/api/sync/assets/missing", &Ids { ids })?;
        Ok(answer.missing)
    }

    pub fn put_asset(&self, id: &str, bytes: &[u8]) -> Result<(), ApiError> {
        self.authorized(self.agent.put(self.url(&format!("/api/sync/assets/{id}"))))
            .header("content-type", "application/octet-stream")
            .send(bytes)
            .map(drop)
            .map_err(Into::into)
    }

    pub fn asset(&self, id: &str) -> Result<Vec<u8>, ApiError> {
        let mut answer = self.get(&format!("/api/sync/assets/{id}")).call()?;
        answer
            .body_mut()
            .with_config()
            .limit(ASSET_LIMIT)
            .read_to_vec()
            .map_err(|err| ApiError::Network(err.to_string()))
    }

    fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T, ApiError> {
        let json = serde_json::to_vec(body).map_err(|err| ApiError::Invalid(err.to_string()))?;
        let mut answer = self
            .authorized(self.agent.post(self.url(path)))
            .header("content-type", "application/json")
            .send(&json[..])?;
        read_json(answer.body_mut())
    }

    fn get(&self, path: &str) -> RequestBuilder<WithoutBody> {
        self.authorized(self.agent.get(self.url(path)))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn authorized<B>(&self, request: RequestBuilder<B>) -> RequestBuilder<B> {
        match &self.token {
            Some(token) => request.header("authorization", format!("Bearer {token}")),
            None => request,
        }
    }
}

fn read_json<T: DeserializeOwned>(body: &mut ureq::Body) -> Result<T, ApiError> {
    let text = body
        .with_config()
        .limit(ANSWER_LIMIT)
        .read_to_string()
        .map_err(|err| ApiError::Network(err.to_string()))?;
    serde_json::from_str(&text).map_err(|err| ApiError::Invalid(err.to_string()))
}

fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "other"
    }
}
