//! Pictures of entities: what a camera sees, an image entity, a media player's artwork or a
//! person's photo. Downloaded on a thread of their own, so a slow camera never holds up the
//! rest; only the newest request of each entity is kept while one is on its way.

use std::collections::BTreeMap;
use std::io::Read;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use log::warn;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig};

use crate::render::{ImageData, ImageFormat};

const TIMEOUT: Duration = Duration::from_secs(10);
const SIZE_LIMIT: u64 = 8 * 1024 * 1024;

pub struct Fetch {
    pub entity: String,
    pub url: String,
    /// Only for Home Assistant's own addresses.
    pub token: Option<String>,
    pub insecure: bool,
}

pub type Fetched = Result<ImageData, String>;

#[derive(Clone)]
pub struct Pictures {
    requests: Sender<Fetch>,
}

impl Pictures {
    /// `done` gets each picture, or why it could not be had; it returns false once nobody listens.
    pub fn start(done: impl Fn(String, Fetched) -> bool + Send + 'static) -> Self {
        let (requests, rx) = mpsc::channel();
        thread::Builder::new()
            .name("hass-pictures".into())
            .spawn(move || serve(rx, done))
            .expect("spawn the picture thread");
        Self { requests }
    }

    pub fn fetch(&self, fetch: Fetch) {
        if self.requests.send(fetch).is_err() {
            warn!("the picture thread is not running");
        }
    }
}

fn serve(requests: Receiver<Fetch>, done: impl Fn(String, Fetched) -> bool) {
    while let Ok(first) = requests.recv() {
        let mut pending = BTreeMap::new();
        pending.insert(first.entity.clone(), first);
        while let Ok(more) = requests.try_recv() {
            pending.insert(more.entity.clone(), more);
        }
        for (entity, fetch) in pending {
            if !done(entity, download(&fetch)) {
                return;
            }
        }
    }
}

fn download(fetch: &Fetch) -> Fetched {
    let tls = TlsConfig::builder()
        .root_certs(RootCerts::PlatformVerifier)
        .disable_verification(fetch.insecure)
        .build();
    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(crate::version::user_agent())
        .tls_config(tls)
        .build()
        .into();
    let mut request = agent.get(&fetch.url);
    if let Some(token) = &fetch.token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let response = request.call().map_err(|err| err.to_string())?;
    let format = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .and_then(ImageFormat::from_mime);
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(SIZE_LIMIT)
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    let format = format
        .or_else(|| sniff(&bytes))
        .ok_or_else(|| "not an image".to_string())?;
    Ok(ImageData {
        format,
        bytes: Arc::from(bytes),
    })
}

/// The format from the first bytes, for servers that do not say.
fn sniff(bytes: &[u8]) -> Option<ImageFormat> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some(ImageFormat::Png),
        [0xFF, 0xD8, 0xFF, ..] => Some(ImageFormat::Jpeg),
        [b'G', b'I', b'F', b'8', ..] => Some(ImageFormat::Gif),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some(ImageFormat::Webp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_formats_by_their_first_bytes() {
        assert_eq!(sniff(&[0x89, b'P', b'N', b'G', 0]), Some(ImageFormat::Png));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(ImageFormat::Jpeg));
        assert_eq!(sniff(b"RIFF1234WEBPVP8"), Some(ImageFormat::Webp));
        assert_eq!(sniff(b"<html>"), None);
    }
}
