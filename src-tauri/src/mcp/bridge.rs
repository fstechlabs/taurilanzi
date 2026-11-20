//! For assistants that only start commands (Claude Desktop): the app's own program, started with
//! `--mcp-bridge <url>`, passes each message of its standard input to the running app's MCP
//! server and writes the answers to its standard output. The token comes in `TOKEN_ENV`.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

pub const BRIDGE_ARG: &str = "--mcp-bridge";
const TOKEN_ENV: &str = "TAURILANZI_MCP_TOKEN";
/// What JSON-RPC calls an error of the server.
const INTERNAL_ERROR: i64 = -32603;

#[derive(Default)]
struct Session {
    id: Option<String>,
    protocol: Option<String>,
    /// The assistant behind the bridge, as it introduced itself, for the window to name it.
    client: Option<String>,
}

type Output = Arc<Mutex<std::io::Stdout>>;

/// Runs until standard input ends. `url` is the server's address, as the app shows it.
pub fn run_bridge(url: &str) {
    let auth = format!("Bearer {}", std::env::var(TOKEN_ENV).unwrap_or_default());
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let session = Arc::new(Mutex::new(Session::default()));
    let out: Output = Arc::new(Mutex::new(std::io::stdout()));
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let message = serde_json::from_str::<Value>(&line).unwrap_or(Value::Null);
        let in_order = message.get("id").is_none()
            || message.get("method").and_then(Value::as_str) == Some("initialize");
        let (agent, auth, url, session, out) = (
            agent.clone(),
            auth.clone(),
            url.to_owned(),
            session.clone(),
            out.clone(),
        );
        let send = move || {
            if let Err(error) = forward(&agent, &url, &auth, &line, &message, &session, &out) {
                eprintln!("taurilanzi mcp bridge: {error}");
                if let Some(id) = message.get("id").cloned() {
                    emit(
                        &out,
                        &json!({ "jsonrpc": "2.0", "id": id, "error": { "code": INTERNAL_ERROR, "message": error } }),
                    );
                }
            }
        };
        // `initialize` settles the session the next requests carry, and notifications answer
        // at once; requests run apart so a long call does not hold back a later cancellation.
        if in_order {
            send();
        } else {
            std::thread::spawn(send);
        }
    }
}

fn forward(
    agent: &ureq::Agent,
    url: &str,
    auth: &str,
    line: &str,
    message: &Value,
    session: &Mutex<Session>,
    out: &Output,
) -> Result<(), String> {
    let mut request = agent
        .post(url)
        .header("Authorization", auth)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream");
    let params = message.get("params");
    if let Some(client) = params
        .and_then(|p| p.pointer("/clientInfo/name"))
        .and_then(Value::as_str)
    {
        session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .client = Some(client.to_owned());
    }
    // Newer versions of the protocol want the method and what it targets in headers too.
    if let Some(method) = message.get("method").and_then(Value::as_str) {
        request = request.header("Mcp-Method", method);
        let target = match method {
            "tools/call" | "prompts/get" => params.and_then(|p| p.get("name")),
            "resources/read" | "resources/subscribe" | "resources/unsubscribe" => {
                params.and_then(|p| p.get("uri"))
            }
            _ => None,
        };
        if let Some(target) = target.and_then(Value::as_str) {
            request = request.header("Mcp-Name", header_value(target));
        }
    }
    let per_request = params
        .and_then(|p| p.pointer("/_meta/io.modelcontextprotocol~1protocolVersion"))
        .and_then(Value::as_str);
    {
        let session = session.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(id) = &session.id {
            request = request.header("Mcp-Session-Id", id);
        }
        if let Some(protocol) = per_request.or(session.protocol.as_deref()) {
            request = request.header("MCP-Protocol-Version", protocol);
        }
        if let Some(client) = &session.client {
            request = request.header("User-Agent", header_value(&format!("{client} (bridge)")));
        }
    }
    let mut response = request
        .send(line)
        .map_err(|err| format!("{err}: is the app open, with its MCP server on?"))?;
    let status = response.status().as_u16();
    if let Some(id) = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
    {
        session.lock().unwrap_or_else(PoisonError::into_inner).id = Some(id.to_owned());
    }
    if status == 202 {
        return Ok(());
    }
    if !(200..300).contains(&status) {
        let body = response.body_mut().read_to_string().unwrap_or_default();
        return Err(format!("HTTP {status}: {}", body.trim()));
    }
    let events = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream"));
    // Images of keys and pages can be large: no limit on what the app answers.
    let mut reader = response.body_mut().with_config().limit(u64::MAX).reader();
    if events {
        let mut data = String::new();
        for line in BufReader::new(reader).lines() {
            let line = line.map_err(|err| err.to_string())?;
            if line.is_empty() {
                if !data.is_empty() {
                    deliver(&std::mem::take(&mut data), session, out);
                }
            } else if let Some(chunk) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(chunk.strip_prefix(' ').unwrap_or(chunk));
            }
        }
        if !data.is_empty() {
            deliver(&data, session, out);
        }
    } else {
        let mut body = String::new();
        reader
            .read_to_string(&mut body)
            .map_err(|err| err.to_string())?;
        if !body.trim().is_empty() {
            deliver(&body, session, out);
        }
    }
    Ok(())
}

fn deliver(text: &str, session: &Mutex<Session>, out: &Output) {
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return;
    };
    if let Some(version) = message
        .pointer("/result/protocolVersion")
        .and_then(Value::as_str)
    {
        session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .protocol = Some(version.to_owned());
    }
    emit(out, &message);
}

fn emit(out: &Output, message: &Value) {
    let mut out = out.lock().unwrap_or_else(PoisonError::into_inner);
    let _ = writeln!(out, "{message}");
    let _ = out.flush();
}

/// Header values are ASCII: anything else travels as base64, as the protocol says.
fn header_value(text: &str) -> String {
    let plain = text.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
        && !text.starts_with(' ')
        && !text.ends_with(' ');
    if plain {
        text.to_owned()
    } else {
        format!("=?base64?{}?=", STANDARD.encode(text))
    }
}
