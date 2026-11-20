//! The HTTP side of the MCP server: loopback only, a bearer token on every request, and the
//! Host and Origin checks the MCP specification asks for against DNS rebinding.

use std::convert::Infallible;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode, header};
use hyper_util::rt::TokioIo;
use hyper_util::server::graceful::GracefulShutdown;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::handler::Assistant;
use super::tools::Tools;

pub const PATH: &str = "/mcp";
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const ACCEPT_RETRY: Duration = Duration::from_millis(50);

type Body = BoxBody<Bytes, Infallible>;
type Service = StreamableHttpService<Assistant, NeverSessionManager>;

/// Who called last, from the `User-Agent` of each authorized request.
pub type OnRequest = Arc<dyn Fn(Option<String>) + Send + Sync>;

pub struct Listener {
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl Listener {
    /// Binds the port on 127.0.0.1, and on ::1 when the system has it. Must run inside the
    /// async runtime, as the MCP library spawns tasks of its own.
    pub async fn start(
        port: u16,
        token: &str,
        tools: Arc<Tools>,
        on_request: OnRequest,
    ) -> std::io::Result<Self> {
        let cancel = CancellationToken::new();
        let config = StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_sse_keep_alive(None)
            .with_allowed_hosts(["127.0.0.1", "localhost", "[::1]"].map(|h| format!("{h}:{port}")))
            .with_allowed_origins([
                "null",
                "http://localhost:*",
                "http://127.0.0.1:*",
                "http://[::1]:*",
            ])
            .with_cancellation_token(cancel.child_token());
        let assistant = Assistant::new(tools);
        let service = StreamableHttpService::new(
            move || Ok(assistant.clone()),
            Arc::new(NeverSessionManager::default()),
            config,
        );
        let mut listeners = vec![TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?];
        if let Ok(v6) = TcpListener::bind((Ipv6Addr::LOCALHOST, port)).await {
            listeners.push(v6);
        }
        let expected = Arc::new(format!("Bearer {token}"));
        let task = tokio::spawn(serve(
            listeners,
            service,
            expected,
            on_request,
            cancel.clone(),
        ));
        Ok(Self { cancel, task })
    }

    pub async fn stop(mut self) {
        self.cancel.cancel();
        let _ = (&mut self.task).await;
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

async fn serve(
    listeners: Vec<TcpListener>,
    service: Service,
    expected: Arc<String>,
    on_request: OnRequest,
    stop: CancellationToken,
) {
    let graceful = GracefulShutdown::new();
    let http = hyper::server::conn::http1::Builder::new();
    let (streams, mut incoming) = tokio::sync::mpsc::channel(16);
    let mut accepting = Vec::new();
    for listener in listeners {
        let streams = streams.clone();
        let stop = stop.clone();
        accepting.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = stop.cancelled() => break,
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _)) => {
                            let _ = streams.send(stream).await;
                        }
                        // A client that gave up, or no descriptors left for a moment: the next
                        // one may well get through.
                        Err(err) => {
                            log::debug!("the MCP server could not accept a connection: {err}");
                            tokio::time::sleep(ACCEPT_RETRY).await;
                        }
                    },
                }
            }
        }));
    }
    drop(streams);
    loop {
        let stream = tokio::select! {
            () = stop.cancelled() => break,
            stream = incoming.recv() => match stream {
                Some(stream) => stream,
                None => break,
            },
        };
        let service = service.clone();
        let expected = expected.clone();
        let on_request = on_request.clone();
        let handle = service_fn(move |request: Request<Incoming>| {
            let service = service.clone();
            let expected = expected.clone();
            let on_request = on_request.clone();
            async move { Ok::<_, Infallible>(route(request, &service, &expected, &on_request).await) }
        });
        let connection = graceful.watch(http.serve_connection(TokioIo::new(stream), handle));
        tokio::spawn(async move {
            let _ = connection.await;
        });
    }
    let _ = tokio::time::timeout(SHUTDOWN_GRACE, graceful.shutdown()).await;
    // Each one holds its port until it ends, and the server may start again on it right away.
    for task in accepting {
        let _ = task.await;
    }
}

async fn route(
    request: Request<Incoming>,
    service: &Service,
    expected: &str,
    on_request: &OnRequest,
) -> Response<Body> {
    if request.uri().path() != PATH {
        return plain(StatusCode::NOT_FOUND, "Not Found");
    }
    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| same(value.as_bytes(), expected.as_bytes()));
    if !authorized {
        let mut response = plain(StatusCode::UNAUTHORIZED, "Unauthorized");
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            "Bearer".parse().expect("a header"),
        );
        return response;
    }
    let agent = request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    on_request(agent);
    service.handle(request).await
}

fn plain(status: StatusCode, text: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Full::new(Bytes::from_static(text.as_bytes())).boxed())
        .expect("a response")
}

/// Compares in constant time, so the token cannot be guessed from how long a refusal takes.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}
