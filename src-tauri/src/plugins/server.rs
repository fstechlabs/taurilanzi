//! One local port, 3906 like Ulanzi Studio when it is free, that speaks three things:
//!
//! - the plugin protocol over WebSocket;
//! - the files of the plugins over HTTP, for their pages (property inspectors, main services
//!   made of HTML and the windows they open);
//! - a proxy for those pages. Ulanzi Studio loads them from `file://` in a browser engine that
//!   lets local pages call any server; WebKit applies CORS, so calls to other origins go through
//!   the host instead. Only pages it served know the token the proxy asks for.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{PoisonError, RwLock};
use std::thread;
use std::time::Duration;

use log::{info, warn};
use tungstenite::handshake::derive_accept_key;
use tungstenite::protocol::Role;
use tungstenite::{Message, WebSocket};

use super::hub::PluginHost;
use super::registry::Registry;

pub const PREFERRED_PORT: u16 = 3906;
const HEAD_LIMIT: usize = 64 * 1024;
const BODY_LIMIT: usize = 32 * 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_millis(30);
const SHIM: &str = include_str!("shim.js");
const SHIM_PATH: &str = "/__taurilanzi/shim.js";
const PROXY_PATH: &str = "/__taurilanzi/proxy";
const TOKEN_HEADER: &str = "x-taurilanzi-token";
const TARGET_HEADER: &str = "x-taurilanzi-target";

/// Starts listening on the preferred port, or any free one if it is taken (by Ulanzi Studio,
/// most likely). Plugins learn the port from their arguments, so any one works.
pub fn start(host: PluginHost) -> io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", PREFERRED_PORT)).or_else(|err| {
        warn!("port {PREFERRED_PORT} is taken ({err}); plugins get another one");
        TcpListener::bind(("127.0.0.1", 0))
    })?;
    let port = listener.local_addr()?.port();
    host.set_port(port);
    let token = uuid::Uuid::new_v4().simple().to_string();
    info!("plugin host on 127.0.0.1:{port}");
    thread::Builder::new()
        .name("plugin-host".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let host = host.clone();
                let token = token.clone();
                let spawned = thread::Builder::new()
                    .name("plugin-client".into())
                    .spawn(move || serve(stream, &host, &token, port));
                if let Err(err) = spawned {
                    warn!("could not serve a plugin connection: {err}");
                }
            }
        })?;
    Ok(port)
}

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    /// What came after the head in the same read.
    rest: Vec<u8>,
}

fn serve(mut stream: TcpStream, host: &PluginHost, token: &str, port: u16) {
    let Some(request) = read_head(&mut stream) else {
        return;
    };
    let upgrade = request
        .headers
        .get("upgrade")
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    if upgrade {
        if allowed_origin(&request, port) {
            websocket(stream, request, host);
        } else {
            let _ = respond(&mut stream, 403, "text/plain", b"not allowed");
        }
        return;
    }
    let result = if request.path == PROXY_PATH {
        proxy(&mut stream, request, token)
    } else if request.path == SHIM_PATH {
        respond(&mut stream, 200, "text/javascript", SHIM.as_bytes())
    } else if let Some(rest) = request.path.strip_prefix("/plugins/") {
        let registry = host.registry();
        file(&mut stream, &registry, rest, token)
    } else {
        respond(&mut stream, 404, "text/plain", b"not found")
    };
    if let Err(err) = result {
        log::debug!("plugin page request failed: {err}");
    }
}

fn read_head(stream: &mut TcpStream) -> Option<Request> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let end = loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        if buf.len() > HEAD_LIMIT {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
    let rest = buf[end + 4..].to_vec();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_owned();
    let target = first.next()?;
    let path = target.split_once('?').map_or(target, |(path, _)| path);
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    Some(Request {
        method,
        path: percent_encoding::percent_decode_str(path)
            .decode_utf8_lossy()
            .into_owned(),
        headers,
        rest,
    })
}

/// Plugins written for Node send no origin; pages the host served come from its own. A page of
/// any other site must not drive the dock.
fn allowed_origin(request: &Request, port: u16) -> bool {
    match request.headers.get("origin") {
        None => true,
        Some(origin) => [
            format!("http://127.0.0.1:{port}"),
            format!("http://localhost:{port}"),
        ]
        .contains(origin),
    }
}

fn websocket(mut stream: TcpStream, request: Request, host: &PluginHost) {
    let Some(key) = request.headers.get("sec-websocket-key") else {
        let _ = respond(&mut stream, 400, "text/plain", b"missing key");
        return;
    };
    let accept = derive_accept_key(key.as_bytes());
    let head = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    if stream.write_all(head.as_bytes()).is_err()
        || stream.set_read_timeout(Some(READ_TIMEOUT)).is_err()
    {
        return;
    }
    let mut socket = if request.rest.is_empty() {
        WebSocket::from_raw_socket(stream, Role::Server, None)
    } else {
        WebSocket::from_partially_read(stream, request.rest, Role::Server, None)
    };
    let (out, outgoing) = mpsc::channel::<String>();
    let conn = host.open(out);
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => host.receive(conn, text.as_str()),
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(err))
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
        let mut wrote = false;
        loop {
            match outgoing.try_recv() {
                Ok(text) => {
                    if socket.write(Message::text(text)).is_err() {
                        host.close(conn);
                        return;
                    }
                    wrote = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let _ = socket.close(None);
                    host.close(conn);
                    return;
                }
            }
        }
        if wrote && socket.flush().is_err() {
            break;
        }
    }
    host.close(conn);
}

fn file(
    stream: &mut TcpStream,
    registry: &RwLock<Registry>,
    rest: &str,
    token: &str,
) -> io::Result<()> {
    let (folder, relative) = rest.split_once('/').unwrap_or((rest, ""));
    let path = {
        let registry = registry.read().unwrap_or_else(PoisonError::into_inner);
        registry
            .plugins()
            .iter()
            .find(|p| p.folder == folder)
            .and_then(|p| p.file(relative))
    };
    let Some(path) = path.filter(|p| p.is_file()) else {
        return respond(stream, 404, "text/plain", b"not found");
    };
    let bytes = std::fs::read(&path)?;
    let kind = content_type(relative);
    if kind.starts_with("text/html") {
        respond(stream, 200, kind, &with_shim(&bytes, token))
    } else {
        respond(stream, 200, kind, &bytes)
    }
}

/// The token and the shim go first in the page, before any script of the plugin runs.
fn with_shim(html: &[u8], token: &str) -> Vec<u8> {
    let text = String::from_utf8_lossy(html);
    let tag = format!(
        r#"<script>window.__TAURILANZI__={{proxy:"{PROXY_PATH}",token:"{token}"}};</script><script src="{SHIM_PATH}"></script>"#
    );
    let lower = text.to_ascii_lowercase();
    let at = lower
        .find("<head")
        .and_then(|i| lower[i..].find('>').map(|j| i + j + 1))
        .unwrap_or(0);
    let mut out = String::with_capacity(text.len() + tag.len());
    out.push_str(&text[..at]);
    out.push_str(&tag);
    out.push_str(&text[at..]);
    out.into_bytes()
}

fn proxy(stream: &mut TcpStream, request: Request, token: &str) -> io::Result<()> {
    if request.headers.get(TOKEN_HEADER).map(String::as_str) != Some(token) {
        return respond(stream, 403, "text/plain", b"not allowed");
    }
    let Some(target) = request
        .headers
        .get(TARGET_HEADER)
        .filter(|t| t.starts_with("http://") || t.starts_with("https://"))
        .cloned()
    else {
        return respond(stream, 400, "text/plain", b"missing target");
    };
    let length: usize = request
        .headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .min(BODY_LIMIT);
    let mut body = request.rest;
    while body.len() < length {
        let mut chunk = vec![0u8; (length - body.len()).min(64 * 1024)];
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);

    let mut builder = ureq::http::Request::builder()
        .method(request.method.as_str())
        .uri(&target);
    for (name, value) in &request.headers {
        let skip = matches!(
            name.as_str(),
            "host"
                | "origin"
                | "referer"
                | "connection"
                | "content-length"
                | "accept-encoding"
                | "keep-alive"
                | "transfer-encoding"
                | "upgrade"
                | "cookie"
        ) || name.starts_with("x-taurilanzi-")
            || name.starts_with("sec-");
        if !skip {
            builder = builder.header(name, value);
        }
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .allow_non_standard_methods(true)
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .into();
    let sent = builder
        .body(body)
        .map_err(io::Error::other)
        .and_then(|prepared| agent.run(prepared).map_err(io::Error::other));
    let response = match sent {
        Ok(response) => response,
        Err(err) => return respond(stream, 502, "text/plain", err.to_string().as_bytes()),
    };
    let status = response.status().as_u16();
    let kind = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_owned();
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(BODY_LIMIT as u64)
        .read_to_end(&mut bytes)?;
    respond(stream, status, &kind, &bytes)
}

fn respond(stream: &mut TcpStream, status: u16, kind: &str, body: &[u8]) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        502 => "Bad Gateway",
        _ => "",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn content_type(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or_default();
    match ext {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::mpsc::Receiver;
    use std::time::Instant;

    use serde_json::{Value, json};
    use tempfile::TempDir;
    use tungstenite::HandshakeError;
    use tungstenite::client::IntoClientRequest;

    use super::super::desktop::Headless;
    use super::super::hub::{HubSetup, PluginInput, VisualChange};
    use super::super::install;
    use super::super::protocol::Instance;
    use super::*;
    use crate::render::ImageFormat;

    const PLUGIN: &str = "com.taurilanzi.e2e.counter";
    const ACTION: &str = "com.taurilanzi.e2e.counter.count";
    const FOLDER: &str = "com.taurilanzi.e2e.ulanziPlugin";
    const WAIT: Duration = Duration::from_secs(5);

    type Socket = WebSocket<TcpStream>;

    struct Host {
        host: PluginHost,
        port: u16,
        inputs: Receiver<PluginInput>,
        dir: TempDir,
    }

    impl Host {
        fn input(&self) -> PluginInput {
            self.inputs
                .recv_timeout(WAIT)
                .expect("an input for the engine")
        }

        fn no_input(&self) {
            assert!(
                self.inputs.try_recv().is_err(),
                "nothing more for the engine"
            );
        }

        fn main(&self) -> Socket {
            let mut socket = connect(self.port, None).unwrap();
            send(
                &mut socket,
                json!({ "code": 0, "cmd": "connected", "uuid": PLUGIN }),
            );
            match self.input() {
                PluginInput::MainConnected { plugin } => assert_eq!(plugin, PLUGIN),
                other => panic!("{other:?}"),
            }
            socket
        }

        fn inspector(&self, actionid: &str) -> (Socket, u64) {
            let mut socket = connect(self.port, None).unwrap();
            send(
                &mut socket,
                json!({ "cmd": "connected", "uuid": ACTION, "key": "2_1", "actionid": actionid }),
            );
            match self.input() {
                PluginInput::InspectorConnected { conn, instance } => {
                    assert_eq!(
                        instance,
                        Instance {
                            action: ACTION.into(),
                            key: "2_1".into(),
                            actionid: actionid.into(),
                        }
                    );
                    (socket, conn)
                }
                other => panic!("{other:?}"),
            }
        }
    }

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/e2e/fixtures")
            .join(FOLDER)
    }

    fn host() -> Host {
        let dir = tempfile::tempdir().unwrap();
        let own = dir.path().join("plugins");
        install::install(&fixture(), &own).unwrap();
        let registry = Registry::scan(&own, "es");
        let (tx, inputs) = mpsc::channel();
        let host = PluginHost::new(HubSetup {
            registry: Arc::new(RwLock::new(registry)),
            notify: Box::new(move |input| {
                let _ = tx.send(input);
            }),
            desktop: Arc::new(Headless),
            logs: dir.path().join("logs"),
        });
        let port = start(host.clone()).unwrap();
        assert_eq!(host.port(), port);
        Host {
            host,
            port,
            inputs,
            dir,
        }
    }

    fn connect(port: u16, origin: Option<&str>) -> Result<Socket, tungstenite::Error> {
        let mut request = format!("ws://127.0.0.1:{port}/")
            .into_client_request()
            .unwrap();
        if let Some(origin) = origin {
            request
                .headers_mut()
                .insert("Origin", origin.parse().unwrap());
        }
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (socket, _) =
            tungstenite::client::client(request, stream).map_err(|err| match err {
                HandshakeError::Failure(err) => err,
                HandshakeError::Interrupted(_) => panic!("the handshake blocks"),
            })?;
        socket
            .get_ref()
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        Ok(socket)
    }

    fn send(socket: &mut Socket, message: Value) {
        socket.send(Message::text(message.to_string())).unwrap();
    }

    fn next(socket: &mut Socket) -> Value {
        let deadline = Instant::now() + WAIT;
        loop {
            match socket.read() {
                Ok(Message::Text(text)) => return serde_json::from_str(text.as_str()).unwrap(),
                Ok(_) => {}
                Err(tungstenite::Error::Io(err))
                    if matches!(
                        err.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    assert!(Instant::now() < deadline, "nothing arrived");
                }
                Err(err) => panic!("{err}"),
            }
        }
    }

    fn ack_of(cmd: &str) -> Value {
        json!({ "cmd": cmd, "cmdType": "NOTIFY", "code": 0, "param": null })
    }

    /// Sends a command and waits for its ack, returning what arrived before it. The host handles
    /// the messages of a connection in order, so whatever a command routes is out once its ack is.
    fn request(socket: &mut Socket, message: Value) -> Vec<Value> {
        let ack = ack_of(message["cmd"].as_str().unwrap());
        send(socket, message);
        let mut before = Vec::new();
        loop {
            let got = next(socket);
            if got == ack {
                return before;
            }
            before.push(got);
        }
    }

    /// What was routed to this connection so far.
    fn drain(socket: &mut Socket) -> Vec<Value> {
        request(socket, json!({ "cmd": "probe" }))
    }

    fn http(port: u16, request: &str) -> (u16, HashMap<String, String>, Vec<u8>) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        let end = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8_lossy(&bytes[..end]).into_owned();
        let mut lines = head.split("\r\n");
        let status = lines
            .next()
            .unwrap()
            .split(' ')
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
            .collect();
        (status, headers, bytes[end + 4..].to_vec())
    }

    fn get(port: u16, path: &str) -> (u16, HashMap<String, String>, Vec<u8>) {
        http(
            port,
            &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"),
        )
    }

    fn token(port: u16) -> String {
        let (status, _, body) = get(port, &format!("/plugins/{FOLDER}/inspector/inspector.html"));
        assert_eq!(status, 200);
        let body = String::from_utf8(body).unwrap();
        let start = body.find("token:\"").unwrap() + "token:\"".len();
        body[start..start + body[start..].find('"').unwrap()].to_owned()
    }

    #[test]
    fn tells_main_services_from_inspectors_and_acks_every_command() {
        let host = host();
        let mut main = host.main();
        assert_eq!(host.host.connected_plugins(), vec![PLUGIN.to_owned()]);
        let (mut inspector, _) = host.inspector("A1");
        assert_eq!(host.host.connected_plugins(), vec![PLUGIN.to_owned()]);

        assert!(request(&mut main, json!({ "cmd": "toast", "msg": "hi" })).is_empty());
        assert!(
            request(
                &mut inspector,
                json!({ "cmd": "openview", "url": "inspector/inspector.html" })
            )
            .is_empty()
        );
        assert!(request(&mut main, json!({ "cmd": "selectdialog", "type": "file" })).is_empty());

        send(
            &mut main,
            json!({ "cmd": "add", "code": 0, "uuid": ACTION, "actionid": "A1" }),
        );
        send(
            &mut main,
            json!({ "cmd": "setSettings", "code": 0, "actionid": "A1", "settings": {} }),
        );
        assert!(drain(&mut main).is_empty(), "echoes get no ack");
        host.no_input();

        let mut stranger = connect(host.port, None).unwrap();
        assert!(drain(&mut stranger).is_empty());
        main.send(Message::text("not json")).unwrap();
        assert!(drain(&mut main).is_empty());

        assert!(host.host.to_main(PLUGIN, &json!({ "cmd": "hello" })));
        assert!(
            !host
                .host
                .to_main("com.other.plugin.id", &json!({ "cmd": "hello" }))
        );
        assert_eq!(next(&mut main), json!({ "cmd": "hello" }));

        main.close(None).unwrap();
        let deadline = Instant::now() + WAIT;
        while !host.host.connected_plugins().is_empty() {
            let _ = main.read();
            assert!(
                Instant::now() < deadline,
                "the main service stays connected"
            );
        }
        host.host.drop_plugin(PLUGIN);
        assert!(drain(&mut stranger).is_empty());
    }

    #[test]
    fn what_a_plugin_draws_goes_to_the_engine() {
        let host = host();
        let mut main = host.main();
        let svg = std::fs::read(fixture().join("images/on.svg")).unwrap();
        let png = b"\x89PNG\r\n\x1a\n".to_vec();
        let written = host.dir.path().join("drawn.png");
        std::fs::write(&written, &png).unwrap();
        let svg_uri = format!(
            "data:image/svg+xml;base64,{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &svg)
        );
        request(
            &mut main,
            json!({ "cmd": "state", "uuid": PLUGIN, "param": { "statelist": [
                { "actionid": "A1", "type": 0, "state": 1, "showtext": true, "textData": "Hi" },
                { "actionid": "A1", "type": 0, "showtext": false, "textData": "Hidden" },
                { "actionid": "A1", "type": 1, "data": svg_uri },
                { "actionid": "A1", "type": 2, "path": "images/on.svg", "showtext": true, "textData": "File" },
                { "actionid": "A1", "type": 2, "path": written.to_string_lossy() },
                { "actionid": "A1", "type": 2, "path": "../../plugin-settings.json" },
                { "actionid": "A1", "type": 1, "data": "!!!" },
                { "type": 0, "state": 1 },
            ] } }),
        );
        let visual = |host: &Host| match host.input() {
            PluginInput::Visual { actionid, change } => {
                assert_eq!(actionid, "A1");
                change
            }
            other => panic!("{other:?}"),
        };
        assert!(matches!(
            visual(&host),
            VisualChange::State { state: 1, title: Some(t) } if t == "Hi"
        ));
        assert!(matches!(
            visual(&host),
            VisualChange::State {
                state: 0,
                title: None
            }
        ));
        assert!(matches!(
            visual(&host),
            VisualChange::Image { image, title: None } if image.format == ImageFormat::Svg && *image.bytes == *svg
        ));
        assert!(matches!(
            visual(&host),
            VisualChange::Image { image, title: Some(t) } if image.format == ImageFormat::Svg && t == "File"
        ));
        assert!(matches!(
            visual(&host),
            VisualChange::Image { image, .. } if image.format == ImageFormat::Png
        ));
        host.no_input();

        request(
            &mut main,
            json!({ "cmd": "setState", "param": { "actionid": "A1", "state": 1 } }),
        );
        assert!(matches!(visual(&host), VisualChange::SetState(1)));
        request(
            &mut main,
            json!({ "cmd": "setTitle", "param": { "actionid": "A1", "text": "T" } }),
        );
        assert!(matches!(visual(&host), VisualChange::Title(Some(t)) if t == "T"));
        request(
            &mut main,
            json!({ "cmd": "setTitle", "actionid": "A1", "text": "" }),
        );
        assert!(matches!(visual(&host), VisualChange::Title(None)));
        request(
            &mut main,
            json!({ "cmd": "setTitle", "param": { "text": "nobody" } }),
        );
        host.no_input();

        request(
            &mut main,
            json!({ "cmd": "setImage", "param": { "actionid": "A1", "clear": true } }),
        );
        assert!(matches!(visual(&host), VisualChange::ClearImages));
        request(
            &mut main,
            json!({ "cmd": "setImage", "param": { "actionid": "A1", "isDefault": true, "icons": [
                { "state": 0, "source": "base64", "base64": svg_uri },
                { "state": 1, "path": "images/off.svg" },
                { "state": 2, "path": "images/missing.svg" },
            ] } }),
        );
        match visual(&host) {
            VisualChange::StateImages(images) => {
                assert_eq!(
                    images.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
                    vec![0, 1]
                );
            }
            other => panic!("{other:?}"),
        }
        request(
            &mut main,
            json!({ "cmd": "setImage", "param": { "actionid": "A1", "icons": [{ "path": "images/off.svg" }] } }),
        );
        assert!(matches!(
            visual(&host),
            VisualChange::Image { title: None, .. }
        ));

        request(&mut main, json!({ "cmd": "showAlert", "actionid": "A1" }));
        assert!(matches!(host.input(), PluginInput::Alert { actionid } if actionid == "A1"));
        host.no_input();
    }

    #[test]
    fn settings_and_messages_travel_between_both_sides() {
        let host = host();
        let mut main = host.main();
        let (mut first, first_conn) = host.inspector("A1");
        let (mut second, _) = host.inspector("A2");

        request(
            &mut first,
            json!({ "cmd": "setSettings", "uuid": ACTION, "key": "2_1", "actionid": "A1", "settings": { "n": 1 } }),
        );
        match host.input() {
            PluginInput::SaveSettings {
                actionid,
                settings,
                from,
                forward,
            } => {
                assert_eq!(actionid, "A1");
                assert_eq!(settings, json!({ "n": 1 }));
                assert_eq!(from, first_conn);
                assert_eq!(forward, "didReceiveSettings");
            }
            other => panic!("{other:?}"),
        }
        request(
            &mut main,
            json!({ "cmd": "paramfromplugin", "uuid": ACTION, "actionid": "A1", "param": { "n": 2 } }),
        );
        match host.input() {
            PluginInput::SaveSettings {
                settings,
                from,
                forward,
                ..
            } => {
                assert_eq!(settings, json!({ "n": 2 }));
                assert_ne!(from, first_conn);
                assert_eq!(forward, "paramfromplugin");
            }
            other => panic!("{other:?}"),
        }
        request(
            &mut main,
            json!({ "cmd": "setSettings", "actionid": "A1", "settings": [1] }),
        );
        assert!(
            matches!(host.input(), PluginInput::SaveSettings { settings, .. } if settings == json!({}))
        );
        request(
            &mut first,
            json!({ "cmd": "getSettings", "actionid": "A1" }),
        );
        assert!(matches!(
            host.input(),
            PluginInput::GetSettings { actionid, conn } if actionid == "A1" && conn == first_conn
        ));

        let to_plugin = json!({ "cmd": "sendToPlugin", "uuid": ACTION, "key": "2_1", "actionid": "A1", "payload": { "x": 1 } });
        assert!(request(&mut first, to_plugin.clone()).is_empty());
        let no_uuid =
            json!({ "cmd": "sendToPlugin", "uuid": "", "actionid": "A1", "payload": { "x": 2 } });
        request(&mut first, no_uuid.clone());
        let mut with_uuid = no_uuid;
        with_uuid["uuid"] = PLUGIN.into();
        assert_eq!(drain(&mut main), vec![to_plugin, with_uuid]);
        assert!(drain(&mut second).is_empty());

        let to_inspector = json!({ "cmd": "sendToPropertyInspector", "uuid": ACTION, "key": "2_1", "actionid": "A1", "payload": { "y": 1 } });
        assert!(request(&mut main, to_inspector.clone()).is_empty());
        assert_eq!(drain(&mut first), vec![to_inspector]);
        assert!(drain(&mut second).is_empty());

        let global = json!({ "cmd": "setGlobalSettings", "uuid": ACTION, "actionid": "A1", "settings": { "token": "t" } });
        request(&mut first, global.clone());
        let mut expected = global;
        expected["cmd"] = "didReceiveGlobalSettings".into();
        for field in ["param", "payload"] {
            expected[field] = json!({ "token": "t" });
        }
        assert_eq!(drain(&mut main), vec![expected.clone()]);
        assert_eq!(drain(&mut second), vec![expected]);
        assert!(drain(&mut first).is_empty(), "not back to who saved them");
        match host.input() {
            PluginInput::SaveGlobalSettings { plugin, settings } => {
                assert_eq!(
                    (plugin.as_str(), settings),
                    (PLUGIN, json!({ "token": "t" }))
                );
            }
            other => panic!("{other:?}"),
        }

        let get = json!({ "cmd": "getGlobalSettings", "uuid": PLUGIN });
        assert!(request(&mut main, get.clone()).is_empty());
        match host.input() {
            PluginInput::GetGlobalSettings {
                plugin,
                conn,
                message,
            } => {
                assert_eq!((plugin.as_str(), message), (PLUGIN, get));
                host.host.to_conn(conn, &json!({ "cmd": "z" }));
                assert_eq!(next(&mut main), json!({ "cmd": "z" }));
            }
            other => panic!("{other:?}"),
        }

        host.host
            .to_instance_except(first_conn, PLUGIN, "A1", &json!({ "cmd": "x" }));
        assert_eq!(drain(&mut main), vec![json!({ "cmd": "x" })]);
        assert!(drain(&mut first).is_empty());
        host.host.to_conn(first_conn, &json!({ "cmd": "y" }));
        assert_eq!(drain(&mut first), vec![json!({ "cmd": "y" })]);
        host.no_input();
    }

    #[test]
    fn plugins_write_their_own_log() {
        let host = host();
        let mut main = host.main();
        request(
            &mut main,
            json!({ "cmd": "logMessage", "uuid": PLUGIN, "message": "hello", "level": "warn" }),
        );
        request(
            &mut main,
            json!({ "cmd": "logMessage", "uuid": PLUGIN, "message": "again" }),
        );
        let log =
            std::fs::read_to_string(host.dir.path().join("logs").join(format!("{PLUGIN}.log")))
                .unwrap();
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 2, "{log}");
        assert!(lines[0].ends_with(&format!("[{PLUGIN}][W] hello")), "{log}");
        assert!(lines[1].ends_with(&format!("[{PLUGIN}][I] again")), "{log}");
    }

    #[test]
    fn pages_of_other_sites_cannot_connect() {
        let host = host();
        match connect(host.port, Some("http://evil.example")) {
            Err(tungstenite::Error::Http(response)) => assert_eq!(response.status(), 403),
            other => panic!("{:?}", other.map(|_| ())),
        }
        for origin in [
            format!("http://127.0.0.1:{}", host.port),
            format!("http://localhost:{}", host.port),
        ] {
            let mut socket = connect(host.port, Some(&origin)).unwrap();
            assert!(drain(&mut socket).is_empty());
        }
        let mut plain = connect(host.port, None).unwrap();
        assert!(drain(&mut plain).is_empty());
    }

    #[test]
    fn serves_the_files_of_plugins_with_the_shim_in_pages() {
        let host = host();
        let original = std::fs::read_to_string(fixture().join("inspector/inspector.html")).unwrap();
        let (status, headers, body) = get(
            host.port,
            &format!("/plugins/{FOLDER}/inspector/inspector.html?uuid=x"),
        );
        assert_eq!(status, 200);
        assert_eq!(headers["content-type"], "text/html; charset=utf-8");
        let token = token(host.port);
        assert_eq!(token.len(), 32);
        let tag = format!(
            r#"<script>window.__TAURILANZI__={{proxy:"/__taurilanzi/proxy",token:"{token}"}};</script><script src="/__taurilanzi/shim.js"></script>"#
        );
        assert_eq!(
            String::from_utf8(body).unwrap(),
            original.replacen("<head>", &format!("<head>{tag}"), 1)
        );

        let (status, headers, body) = get(host.port, &format!("/plugins/{FOLDER}/images/on.svg"));
        assert_eq!(status, 200);
        assert_eq!(headers["content-type"], "image/svg+xml");
        assert_eq!(
            body,
            std::fs::read(fixture().join("images/on.svg")).unwrap()
        );
        let (status, _, body) = get(host.port, "/__taurilanzi/shim.js");
        assert_eq!(status, 200);
        assert_eq!(body, SHIM.as_bytes());

        for path in [
            format!("/plugins/{FOLDER}/../{FOLDER}/manifest.json"),
            format!("/plugins/{FOLDER}/%2E%2E/{FOLDER}/manifest.json"),
            format!("/plugins/{FOLDER}/images/missing.svg"),
            format!("/plugins/{FOLDER}/images"),
            "/plugins/other.ulanziPlugin/manifest.json".into(),
            "/".into(),
        ] {
            assert_eq!(get(host.port, &path).0, 404, "{path}");
        }
    }

    #[test]
    fn the_shim_goes_right_after_the_head_tag() {
        let page =
            String::from_utf8(with_shim(b"<html><HEAD lang=\"es\"><title>x</title>", "t")).unwrap();
        assert!(
            page.starts_with("<html><HEAD lang=\"es\"><script>window.__TAURILANZI__="),
            "{page}"
        );
        assert!(page.ends_with("</script><title>x</title>"), "{page}");
        let bare = String::from_utf8(with_shim(b"<p>x</p>", "t")).unwrap();
        assert!(
            bare.starts_with("<script>") && bare.ends_with("<p>x</p>"),
            "{bare}"
        );
    }

    #[test]
    fn the_proxy_needs_the_token_and_forwards_the_request() {
        let host = host();
        let server = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let target = format!(
            "http://127.0.0.1:{}/echo?q=1",
            server.local_addr().unwrap().port()
        );
        let (seen, received) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = server.accept().unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = stream.read(&mut chunk).unwrap();
                bytes.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&bytes).into_owned();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length: usize = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length || n == 0 {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            seen.send(String::from_utf8(bytes).unwrap()).unwrap();
            let body = r#"{"ok":true}"#;
            write!(
                stream,
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });

        let call = |token: &str, target: &str| {
            http(
                host.port,
                &format!(
                    "PUT /__taurilanzi/proxy HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Taurilanzi-Token: {token}\r\nX-Taurilanzi-Target: {target}\r\nX-Custom: v\r\nContent-Type: text/plain\r\nCookie: c=1\r\nOrigin: http://127.0.0.1:{}\r\nContent-Length: 5\r\n\r\nhello",
                    host.port
                ),
            )
        };
        assert_eq!(call("wrong", &target).0, 403);
        assert_eq!(
            http(
                host.port,
                &format!(
                    "GET /__taurilanzi/proxy HTTP/1.1\r\nX-Taurilanzi-Target: {target}\r\n\r\n"
                )
            )
            .0,
            403
        );
        let token = token(host.port);
        assert_eq!(call(&token, "file:///etc/passwd").0, 400);

        let (status, headers, body) = call(&token, &target);
        assert_eq!(status, 201);
        assert_eq!(headers["content-type"], "application/json");
        assert_eq!(body, br#"{"ok":true}"#);
        let forwarded = received.recv_timeout(WAIT).unwrap();
        let lower = forwarded.to_ascii_lowercase();
        assert!(
            forwarded.starts_with("PUT /echo?q=1 HTTP/1.1\r\n"),
            "{forwarded}"
        );
        assert!(lower.contains("\r\nx-custom: v\r\n"), "{forwarded}");
        assert!(
            lower.contains("\r\ncontent-type: text/plain\r\n"),
            "{forwarded}"
        );
        assert!(forwarded.ends_with("\r\n\r\nhello"), "{forwarded}");
        for header in ["x-taurilanzi-", "cookie:", "origin:"] {
            assert!(!lower.contains(header), "{header} in {forwarded}");
        }
    }
}
