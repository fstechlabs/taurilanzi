use std::io::Read;
use std::time::{Duration, Instant};

use serde::Serialize;
use ureq::http::{Request, Response};
use ureq::tls::TlsConfig;
use ureq::{Agent, Body};

use super::error::ActionError;
use crate::config::{HttpBody, HttpRequest};

/// Enough of the answer to see what came back, without holding a large download in memory.
const BODY_LIMIT: usize = 64 * 1024;

/// What a request got back, for the editor to show.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct HttpResponse {
    pub status: u16,
    pub reason: String,
    pub duration_ms: u32,
    pub content_type: Option<String>,
    /// The start of the body, as text.
    pub body: String,
    /// The body was longer than what is kept.
    pub truncated: bool,
}

impl HttpResponse {
    /// Redirects are followed, so anything below 400 is an answer the server was happy with.
    pub fn succeeded(&self) -> bool {
        self.status < 400
    }
}

/// Sends the request from a key: an answer of 400 or above counts as a failure.
pub fn run(request: &HttpRequest) -> Result<(), ActionError> {
    let response = send(request)?;
    if response.succeeded() {
        Ok(())
    } else {
        Err(ActionError::HttpStatus(response.status))
    }
}

pub fn send(request: &HttpRequest) -> Result<HttpResponse, ActionError> {
    if let Some(problem) = request.problem() {
        return Err(ActionError::Http(problem.into()));
    }
    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(u64::from(request.timeout_ms))))
        .http_status_as_error(false)
        .allow_non_standard_methods(true)
        .user_agent(crate::version::user_agent())
        .tls_config(
            TlsConfig::builder()
                .disable_verification(request.insecure)
                .build(),
        )
        .build()
        .into();

    let mut builder = Request::builder()
        .method(request.method.as_str())
        .uri(request.url.trim());
    let named = |name: &str| {
        request
            .headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case(name))
    };
    let (body, content_type) = encode(&request.body);
    if let Some(content_type) = content_type.filter(|_| !named("content-type")) {
        builder = builder.header("content-type", content_type);
    }
    for header in &request.headers {
        builder = builder.header(header.name.trim(), header.value.trim());
    }
    let prepared = builder
        .body(body)
        .map_err(|err| ActionError::Http(err.to_string()))?;

    let started = Instant::now();
    let response = agent
        .run(prepared)
        .map_err(|err| ActionError::Http(err.to_string()))?;
    let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
    Ok(read(response, duration_ms))
}

/// The bytes to send and the type they are, unless the user says otherwise in a header.
fn encode(body: &HttpBody) -> (Vec<u8>, Option<String>) {
    match body {
        HttpBody::None => (Vec::new(), None),
        HttpBody::Json { json } => (json.as_bytes().to_vec(), Some("application/json".into())),
        HttpBody::Text { text, content_type } => {
            (text.as_bytes().to_vec(), Some(content_type.trim().into()))
        }
        HttpBody::Form { fields } => {
            let encoded = fields
                .iter()
                .map(|f| format!("{}={}", form_encode(f.name.trim()), form_encode(&f.value)))
                .collect::<Vec<_>>()
                .join("&");
            (
                encoded.into_bytes(),
                Some("application/x-www-form-urlencoded".into()),
            )
        }
    }
}

fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'*' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn read(response: Response<Body>, duration_ms: u32) -> HttpResponse {
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let mut bytes = Vec::new();
    let mut reader = response
        .into_body()
        .into_reader()
        .take(BODY_LIMIT as u64 + 1);
    // Whatever arrived before a failure mid-body is still worth showing.
    let _ = reader.read_to_end(&mut bytes);
    let truncated = bytes.len() > BODY_LIMIT;
    bytes.truncate(BODY_LIMIT);
    HttpResponse {
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or_default().into(),
        duration_ms,
        content_type,
        body: String::from_utf8_lossy(&bytes).into_owned(),
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;
    use crate::config::{HttpMethod, HttpPair};

    /// Answers one request with `status` and gives back what it received, head and body.
    fn serve_once(status: u16) -> (String, thread::JoinHandle<(String, String)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let answer = r#"{"ok":true}"#;
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                answer.len()
            )
            .unwrap();
            (head, String::from_utf8(body).unwrap())
        });
        (address, handle)
    }

    fn request(method: HttpMethod, url: String, body: HttpBody) -> HttpRequest {
        HttpRequest {
            method,
            url,
            headers: vec![HttpPair {
                name: "Authorization".into(),
                value: "Bearer abc".into(),
            }],
            body,
            timeout_ms: 5_000,
            insecure: false,
            show_result: true,
        }
    }

    #[test]
    fn sends_method_headers_and_a_json_body() {
        let (address, server) = serve_once(200);
        let answer = send(&request(
            HttpMethod::Patch,
            format!("{address}/api?x=1"),
            HttpBody::Json {
                json: r#"{"on":true}"#.into(),
            },
        ))
        .unwrap();
        let (head, body) = server.join().unwrap();
        let head = head.to_ascii_lowercase();
        assert!(head.starts_with("patch /api?x=1 http/1.1"), "{head}");
        assert!(head.contains("authorization: bearer abc"), "{head}");
        assert!(head.contains("content-type: application/json"), "{head}");
        assert_eq!(body, r#"{"on":true}"#);
        assert_eq!(answer.status, 200);
        assert!(answer.succeeded());
        assert_eq!(answer.body, r#"{"ok":true}"#);
        assert_eq!(answer.content_type.as_deref(), Some("application/json"));
    }

    #[test]
    fn a_header_of_the_user_wins_over_the_body_type() {
        let (address, server) = serve_once(200);
        let mut sent = request(
            HttpMethod::Post,
            address,
            HttpBody::Json { json: "{}".into() },
        );
        sent.headers.push(HttpPair {
            name: "Content-Type".into(),
            value: "application/vnd.api+json".into(),
        });
        send(&sent).unwrap();
        let (head, _) = server.join().unwrap();
        let head = head.to_ascii_lowercase();
        assert!(head.contains("content-type: application/vnd.api+json"));
        assert!(!head.contains("content-type: application/json\r"));
    }

    #[test]
    fn an_error_answer_fails_the_key() {
        let (address, server) = serve_once(500);
        let result = run(&request(HttpMethod::Get, address, HttpBody::None));
        server.join().unwrap();
        assert!(matches!(result, Err(ActionError::HttpStatus(500))));
    }

    #[test]
    fn says_why_a_request_could_not_be_made() {
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", closed.local_addr().unwrap());
        drop(closed);
        let failed = send(&request(HttpMethod::Get, address, HttpBody::None));
        assert!(matches!(failed, Err(ActionError::Http(_))));
        let invalid = send(&request(HttpMethod::Get, "ftp://x".into(), HttpBody::None));
        assert!(matches!(invalid, Err(ActionError::Http(_))));
    }

    #[test]
    fn encodes_forms_and_bodies() {
        assert_eq!(form_encode("a b&ñ=*"), "a+b%26%C3%B1%3D*");
        let (bytes, kind) = encode(&HttpBody::Form {
            fields: vec![
                HttpPair {
                    name: " q ".into(),
                    value: "x y".into(),
                },
                HttpPair {
                    name: "n".into(),
                    value: "1".into(),
                },
            ],
        });
        assert_eq!(bytes, b"q=x+y&n=1");
        assert_eq!(kind.as_deref(), Some("application/x-www-form-urlencoded"));
        assert_eq!(encode(&HttpBody::None), (Vec::new(), None));
        let (_, text) = encode(&HttpBody::Text {
            text: "hola".into(),
            content_type: " text/csv ".into(),
        });
        assert_eq!(text.as_deref(), Some("text/csv"));
    }
}
