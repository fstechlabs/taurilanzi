//! The MCP server as an assistant meets it: over HTTP, with its token, against a real engine on a
//! data folder of its own.

use std::net::TcpListener as PortFinder;
use std::sync::Arc;

use serde_json::{Value, json};

use super::listener::{Listener, PATH};
use super::tools::Tools;
use crate::engine::{self, Launch};

const TOKEN: &str = "test-token";

pub(super) struct Server {
    url: String,
    agent: ureq::Agent,
    engine: crate::engine::EngineHandle,
    dir: tempfile::TempDir,
}

impl Server {
    pub(super) fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let running = engine::start(Launch {
            data_dir: dir.path().to_path_buf(),
            identifier: "com.taurilanzi.test".into(),
            dock: false,
            watch_apps: false,
            plugins: None,
        })
        .unwrap();
        let engine = running.engine.clone();
        let tools = Arc::new(Tools {
            engine: running.engine,
            plugins: None,
            assets: running.assets,
            notify: Arc::new(|_| {}),
        });
        let port = PortFinder::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let listener =
            tauri::async_runtime::block_on(Listener::start(port, TOKEN, tools, Arc::new(|_| {})))
                .unwrap();
        // Dropping it would stop it: it serves until the process ends, like the app's.
        std::mem::forget(listener);
        Self {
            url: format!("http://127.0.0.1:{port}{PATH}"),
            agent: ureq::Agent::config_builder()
                .http_status_as_error(false)
                .build()
                .into(),
            engine,
            dir,
        }
    }

    fn call(&self, method: &str, params: Value) -> Value {
        let mut response = self
            .agent
            .post(&self.url)
            .header("Authorization", format!("Bearer {TOKEN}"))
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .send(
                json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
                    .to_string(),
            )
            .unwrap();
        assert_eq!(response.status(), 200, "{method}");
        serde_json::from_str(&response.body_mut().read_to_string().unwrap()).unwrap()
    }

    /// The whole result of a tool, refusals included.
    fn result(&self, name: &str, arguments: Value) -> Value {
        self.call(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )["result"]
            .clone()
    }

    /// The JSON a tool answered, or the reason it refused.
    pub(super) fn tool(&self, name: &str, arguments: Value) -> Result<Value, String> {
        let result = self.result(name, arguments);
        let text = result["content"][0]["text"].as_str().unwrap_or_default();
        if result["isError"] != false {
            return Err(if text.is_empty() {
                result.to_string()
            } else {
                text.to_owned()
            });
        }
        Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.into())))
    }

    pub(super) fn ok(&self, name: &str, arguments: Value) -> Value {
        self.tool(name, arguments)
            .unwrap_or_else(|err| panic!("{name}: {err}"))
    }
}

#[test]
fn only_assistants_with_the_token_from_this_computer_get_in() {
    let server = Server::start();
    let ask = |token: Option<&str>, origin: Option<&str>| {
        let mut request = server
            .agent
            .post(&server.url)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json");
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        if let Some(origin) = origin {
            request = request.header("Origin", origin);
        }
        request
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
            .unwrap()
            .status()
            .as_u16()
    };
    assert_eq!(ask(None, None), 401);
    assert_eq!(ask(Some("wrong"), None), 401);
    assert_eq!(ask(Some(TOKEN), Some("https://evil.example")), 403);
    assert_eq!(ask(Some(TOKEN), None), 200);
}

#[test]
fn a_refused_request_leaves_its_connection_usable_for_the_next() {
    use std::io::{BufRead, BufReader, Read, Write};

    let server = Server::start();
    let address = server
        .url
        .trim_start_matches("http://")
        .trim_end_matches(PATH);
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
    let head = |token: &str| {
        format!(
            "POST {PATH} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\n\
             Accept: application/json, text/event-stream\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\n\r\n",
            body.len()
        )
    };
    let status = |reader: &mut BufReader<std::net::TcpStream>| {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let status = line.split(' ').nth(1).unwrap_or_default().to_owned();
        let mut length = 0;
        loop {
            line.clear();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap();
            }
        }
        reader.read_exact(&mut vec![0; length]).unwrap();
        status
    };

    // The body comes late, as from a client that writes it apart from the head.
    stream.write_all(head("wrong").as_bytes()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    stream.write_all(body.as_bytes()).unwrap();
    assert_eq!(status(&mut reader), "401");

    stream.write_all(head(TOKEN).as_bytes()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
    assert_eq!(status(&mut reader), "200");
}

#[test]
fn an_assistant_builds_a_profile_sees_it_and_undoes_it() {
    let server = Server::start();
    let init = server.call(
        "initialize",
        json!({ "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }),
    );
    assert!(
        init["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("apply_changes")
    );
    let tools = server.call("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"apply_changes") && names.contains(&"render_page"));

    let before = server.ok("get_overview", json!({}));
    let profiles = before["profiles"].as_array().unwrap().len();

    let created = server.ok(
        "apply_changes",
        json!({ "changes": [
            { "op": "create_profile", "profile_name": "Streaming", "name": "$p", "home_name": "$home" },
            { "op": "create_page", "profile": "$p", "page_name": "Escenas", "name": "$scenes" },
            { "op": "set_key", "page": "$home", "index": 13, "key": {
                "kind": "action",
                "actions": [ { "type": "open_page", "page": "$scenes" } ],
            } },
        ] }),
    );
    let home = created["created"]["$home"].as_str().unwrap().to_owned();
    let scenes = created["created"]["$scenes"].as_str().unwrap().to_owned();

    let key = server.ok("get_key", json!({ "page": home, "index": 13 }));
    assert_eq!(key["actions"][0]["page"], scenes.as_str());
    let overview = server.ok("get_overview", json!({}));
    assert_eq!(overview["profiles"].as_array().unwrap().len(), profiles + 1);
    assert_ne!(
        overview["active_profile"], created["created"]["$p"],
        "building a profile does not take the dock to it"
    );

    let image = server.result("render_page", json!({ "page": home }));
    assert_eq!(image["content"][0]["type"], "image");
    assert_eq!(image["content"][0]["mimeType"], "image/png");

    let refused = server.result(
        "apply_changes",
        json!({ "changes": [
            { "op": "set_key", "page": home, "index": 1, "key": { "actions": [] } },
            { "op": "set_key", "page": home, "index": 2, "key": {
                "actions": [ { "type": "home" }, { "type": "back" } ],
            } },
        ] }),
    );
    assert_eq!(refused["isError"], true);
    let message = refused["content"][0]["text"].as_str().unwrap();
    assert!(message.starts_with("change 1"), "{message}");
    let page = server.ok("get_page", json!({ "page": home }));
    assert!(
        page["keys"].get("1").is_none(),
        "nothing of a refused call stays"
    );

    server.ok("undo", json!({}));
    let after = server.ok("get_overview", json!({}));
    assert_eq!(
        after["profiles"].as_array().unwrap().len(),
        profiles,
        "one undo takes the whole call back"
    );
}

#[test]
fn the_schema_and_the_guide_are_there_to_read() {
    let server = Server::start();
    let schema = server.result("get_schema", json!({ "type": "Key" }));
    let text = schema["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("export type Key ="));
    let guide = server.result("get_guide", json!({}));
    assert_eq!(guide["isError"], false);
}

#[test]
fn secrets_in_keys_stay_hidden_and_keep_their_value() {
    let server = Server::start();
    let overview = server.ok("get_overview", json!({}));
    let home = overview["profiles"][0]["home_page"]
        .as_str()
        .unwrap()
        .to_owned();
    let request = |authorization: &str| {
        json!({ "type": "http", "request": {
            "method": "GET",
            "url": "https://example.com/hook",
            "headers": [
                { "name": "Authorization", "value": authorization },
                { "name": "Accept", "value": "text/plain" },
            ],
        } })
    };
    server.ok(
        "set_key",
        json!({ "page": home, "index": 4, "key": { "actions": [request("Bearer real")] } }),
    );

    let read = server.ok("get_key", json!({ "page": home, "index": 4 }));
    let headers = &read["actions"][0]["request"]["headers"];
    assert_eq!(headers[0]["value"], crate::mcp::redaction::HIDDEN);
    assert_eq!(headers[1]["value"], "text/plain");

    server.ok("set_key", json!({ "page": home, "index": 4, "key": read }));
    let config = tauri::async_runtime::block_on(server.engine.config()).unwrap();
    let page: uuid::Uuid = home.parse().unwrap();
    let saved =
        serde_json::to_value(&config.page_owner(page).unwrap().page(page).unwrap().keys[&4])
            .unwrap();
    assert_eq!(
        saved["actions"][0]["request"]["headers"][0]["value"], "Bearer real",
        "sent back hidden, it keeps what it had"
    );

    let mut redirected = read.clone();
    redirected["actions"][0]["request"]["url"] = "https://attacker.example/hook".into();
    let refused = server.tool(
        "set_key",
        json!({ "page": home, "index": 4, "key": redirected }),
    );
    assert!(
        refused.unwrap_err().contains("somewhere else"),
        "a hidden header never follows a request to another place"
    );

    let invented = server.tool(
        "set_key",
        json!({ "page": home, "index": 6, "key": { "actions": [request(crate::mcp::redaction::HIDDEN)] } }),
    );
    assert!(
        invented.is_err(),
        "a hidden value with nothing behind it is refused"
    );
}

#[test]
fn moving_an_integration_forgets_its_secret_first() {
    let server = Server::start();
    let obs = |host: &str, id: Option<&str>| {
        let mut integration = json!({ "kind": "obs", "name": "OBS", "host": host, "port": 1 });
        if let Some(id) = id {
            integration["id"] = id.into();
        }
        server.ok("save_integration", json!({ "integration": integration }))
    };
    let id = obs("127.0.0.1", None)["id"].as_str().unwrap().to_owned();
    let server_id: uuid::Uuid = id.parse().unwrap();
    let client = server.engine.obs(server_id).unwrap();
    tauri::async_runtime::block_on(client.set_password(Some("secret".into()))).unwrap();
    let secrets = crate::secrets::Secrets::new(server.dir.path(), "com.taurilanzi.test");
    let account = crate::obs::password_account(server_id);
    assert!(secrets.get(&account).unwrap().is_some());

    server.ok(
        "save_integration",
        json!({ "integration": { "kind": "obs", "id": id, "name": "Estudio", "host": "127.0.0.1", "port": 1 } }),
    );
    assert!(
        secrets.get(&account).unwrap().is_some(),
        "renaming keeps it"
    );

    obs("192.0.2.1", Some(&id));
    assert!(
        secrets.get(&account).unwrap().is_none(),
        "another address never gets it"
    );
}
