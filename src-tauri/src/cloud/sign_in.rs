//! Signing a computer in from the browser (RFC 8252): the cloud sends the browser back to a port
//! of this computer with a one-time code, which only this app can exchange for a token, since only
//! it knows the verifier behind the challenge it sent (PKCE).

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use percent_encoding::percent_decode_str;
use sha2::{Digest, Sha256};

/// Long enough to sign in to Google without hurry.
const WAIT_FOR: Duration = Duration::from_secs(10 * 60);
const POLL: Duration = Duration::from_millis(100);
const REQUEST_LIMIT: usize = 16 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum SignInError {
    #[error("the sign-in was cancelled")]
    Cancelled,
    #[error("the browser did not come back in time")]
    TimedOut,
    #[error("the account did not allow this computer")]
    Denied,
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct Pending {
    listener: TcpListener,
    state: String,
    pub verifier: String,
    /// Where the browser starts.
    pub url: String,
}

/// Listens on a free port of this computer only and builds the address for the browser.
pub fn start(base: &str, locale: &str) -> io::Result<Pending> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let state = random(16)?;
    let verifier = random(32)?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let url = format!(
        "{}/auth/desktop?port={port}&state={state}&challenge={challenge}&locale={locale}",
        base.trim_end_matches('/'),
    );
    Ok(Pending {
        listener,
        state,
        verifier,
        url,
    })
}

impl Pending {
    /// Waits for the browser to come back with the code, answering it with a page that tells the
    /// user to go back to the app.
    pub fn wait(
        &self,
        cancelled: &AtomicBool,
        page: impl Fn(bool) -> String,
    ) -> Result<String, SignInError> {
        let until = Instant::now() + WAIT_FOR;
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err(SignInError::Cancelled);
            }
            if Instant::now() > until {
                return Err(SignInError::TimedOut);
            }
            match self.listener.accept() {
                Ok((stream, _)) => match self.answer(stream, &page) {
                    Ok(Some(answer)) => return answer,
                    Ok(None) => {}
                    Err(err) => log::warn!("a request to the sign-in port failed: {err}"),
                },
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => thread::sleep(POLL),
                Err(err) => return Err(err.into()),
            }
        }
    }

    /// `None` for requests that are not the callback, such as the browser asking for an icon.
    fn answer(
        &self,
        mut stream: TcpStream,
        page: &impl Fn(bool) -> String,
    ) -> io::Result<Option<Result<String, SignInError>>> {
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let Some(target) = read_target(&mut stream)? else {
            return Ok(None);
        };
        let Some(query) = target.strip_prefix("/callback?") else {
            respond(&mut stream, "404 Not Found", "")?;
            return Ok(None);
        };
        let (mut code, mut state, mut error) = (None, None, None);
        for pair in query.split('&') {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            let value = percent_decode_str(value).decode_utf8_lossy().into_owned();
            match name {
                "code" => code = Some(value),
                "state" => state = Some(value),
                "error" => error = Some(value),
                _ => {}
            }
        }
        // Another page of the browser cannot sign the computer in to someone else's account.
        if state.as_deref() != Some(self.state.as_str()) {
            respond(&mut stream, "400 Bad Request", "")?;
            return Ok(None);
        }
        let result = match (code, error) {
            (Some(code), None) if !code.is_empty() => Ok(code),
            _ => Err(SignInError::Denied),
        };
        respond(&mut stream, "200 OK", &page(result.is_ok()))?;
        Ok(Some(result))
    }
}

fn read_target(stream: &mut TcpStream) -> io::Result<Option<String>> {
    let mut request = Vec::new();
    let mut chunk = [0u8; 2048];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        let read = match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
            Err(err) if err.kind() == io::ErrorKind::TimedOut => break,
            Err(err) => return Err(err),
        };
        request.extend_from_slice(&chunk[..read]);
        if request.len() > REQUEST_LIMIT {
            break;
        }
    }
    let line = String::from_utf8_lossy(&request);
    let mut parts = line.lines().next().unwrap_or_default().split(' ');
    Ok(match (parts.next(), parts.next()) {
        (Some("GET"), Some(target)) => Some(target.to_owned()),
        _ => None,
    })
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\ncache-control: no-store\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

/// Random bytes from the system's generator, as URL-safe text.
fn random(bytes: usize) -> io::Result<String> {
    let mut raw = vec![0u8; bytes];
    getrandom::fill(&mut raw).map_err(|err| io::Error::other(err.to_string()))?;
    Ok(URL_SAFE_NO_PAD.encode(raw))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::Arc;

    use super::*;

    fn param<'a>(url: &'a str, name: &str) -> &'a str {
        url.split(['?', '&'])
            .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
            .unwrap()
    }

    /// What the browser does when the cloud sends it back to the app.
    fn visit(url: &str, path: &str) -> String {
        let port = param(url, "port");
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        answer
    }

    fn page(ok: bool) -> String {
        if ok { "dentro".into() } else { "fuera".into() }
    }

    #[test]
    fn sends_a_challenge_only_the_verifier_answers() {
        let pending = start("https://cloud.example/", "es").unwrap();
        assert!(
            pending
                .url
                .starts_with("https://cloud.example/auth/desktop?port=")
        );
        assert_eq!(param(&pending.url, "locale"), "es");
        let challenge = param(&pending.url, "challenge");
        assert_eq!(
            challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(pending.verifier.as_bytes()))
        );
        assert_eq!(challenge.len(), 43);
        assert!(param(&pending.url, "state").len() >= 16);
        let other = start("https://cloud.example", "es").unwrap();
        assert_ne!(other.verifier, pending.verifier);
    }

    #[test]
    fn takes_the_code_from_the_browser_and_answers_it() {
        let pending = start("http://127.0.0.1:1", "es").unwrap();
        let url = pending.url.clone();
        let browser = thread::spawn(move || {
            let icon = visit(&url, "/favicon.ico");
            let stranger = visit(&url, "/callback?code=evil&state=not-ours");
            let state = param(&url, "state").to_owned();
            let ours = visit(&url, &format!("/callback?code=the%2Dcode&state={state}"));
            (icon, stranger, ours)
        });
        let code = pending.wait(&AtomicBool::new(false), page).unwrap();
        let (icon, stranger, ours) = browser.join().unwrap();
        assert_eq!(code, "the-code");
        assert!(icon.starts_with("HTTP/1.1 404"));
        assert!(stranger.starts_with("HTTP/1.1 400"));
        assert!(ours.starts_with("HTTP/1.1 200"));
        assert!(ours.ends_with("dentro"));
    }

    #[test]
    fn says_no_when_the_account_does_not_allow_the_computer() {
        let pending = start("http://127.0.0.1:1", "es").unwrap();
        let url = pending.url.clone();
        let browser = thread::spawn(move || {
            let state = param(&url, "state").to_owned();
            visit(&url, &format!("/callback?error=denied&state={state}"))
        });
        let result = pending.wait(&AtomicBool::new(false), page);
        assert!(matches!(result, Err(SignInError::Denied)));
        assert!(browser.join().unwrap().ends_with("fuera"));
    }

    #[test]
    fn stops_waiting_when_cancelled() {
        let pending = start("http://127.0.0.1:1", "es").unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancelled);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            flag.store(true, Ordering::Relaxed);
        });
        assert!(matches!(
            pending.wait(&cancelled, page),
            Err(SignInError::Cancelled)
        ));
    }
}
