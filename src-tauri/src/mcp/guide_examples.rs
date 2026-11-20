//! Every example of the guide, sent through the MCP server to a real engine as an assistant would
//! send it, with its placeholders replaced by ids of a data folder of the test's own.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::end_to_end::Server;

const GUIDE: &str = include_str!("guide.md");
const KEY_INDEX: u8 = 5;
const WIDE_KEY: u8 = 13;
const ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2h20v20H2z" fill="currentColor"/></svg>"#;

/// A fenced block of JSON, with the words after `json` that say what it is.
struct Example {
    line: usize,
    tag: String,
    body: String,
}

/// Blocks of other languages are left alone: plain ones and `text` hold drawings and shapes.
fn examples(markdown: &str) -> Vec<Example> {
    let mut found = Vec::new();
    // Inside a block, and the example it is if it is JSON.
    let mut open: Option<Option<Example>> = None;
    for (at, line) in markdown.lines().enumerate() {
        let fence = line.trim_start().strip_prefix("```").map(str::trim);
        match (&mut open, fence) {
            (Some(_), Some(_)) => found.extend(open.take().flatten()),
            (Some(Some(example)), None) => {
                example.body.push_str(line);
                example.body.push('\n');
            }
            (Some(None), None) | (None, None) => {}
            (None, Some(info)) => {
                open = Some(info.strip_prefix("json").map(|tag| Example {
                    line: at + 1,
                    tag: tag.trim().to_owned(),
                    body: String::new(),
                }));
            }
        }
    }
    found
}

/// Real ids for the placeholders the guide writes, from a data folder set up as a user's would be.
fn placeholders(server: &Server, files: &std::path::Path) -> BTreeMap<&'static str, String> {
    let icon = files.join("icon.svg");
    std::fs::write(&icon, ICON).unwrap();
    let sound = files.join("aplausos.wav");
    std::fs::write(&sound, b"RIFF\0\0\0\0WAVEfmt ").unwrap();
    let asset = server.ok("import_image", json!({ "path": icon }))["asset"].clone();
    let sound = server.ok("import_sound", json!({ "path": sound }))["sound"].clone();
    let obs = server.ok(
        "save_integration",
        json!({ "integration": { "kind": "obs", "name": "OBS", "host": "127.0.0.1", "port": 1 } }),
    )["id"]
        .clone();
    let hass = server.ok(
        "save_integration",
        json!({ "integration": { "kind": "home_assistant", "name": "Casa", "url": "http://127.0.0.1:1", "insecure": false } }),
    )["id"]
        .clone();
    let overview = server.ok("get_overview", json!({}));
    let profile = overview["active_profile"].clone();
    let home = overview["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == profile)
        .unwrap()["home_page"]
        .clone();
    let target = server.ok(
        "apply_changes",
        json!({ "changes": [{ "op": "create_page", "profile": profile, "page_name": "Destino", "name": "$target" }] }),
    )["created"]["$target"]
        .clone();
    let text = |value: Value| value.as_str().unwrap().to_owned();
    BTreeMap::from([
        ("<asset id>", text(asset)),
        ("<sound id>", text(sound)),
        ("<obs server id>", text(obs)),
        ("<hass server id>", text(hass)),
        ("<profile id>", text(profile)),
        ("<page id>", text(home)),
        ("<target page id>", text(target)),
        ("<plugin action id>", "com.example.guide.action".into()),
    ])
}

fn fill(value: &mut Value, ids: &BTreeMap<&str, String>) -> Result<(), String> {
    match value {
        Value::String(text) if text.starts_with('<') && text.ends_with('>') => {
            *text = ids
                .get(text.as_str())
                .ok_or_else(|| format!("{text} is not a placeholder the test knows"))?
                .clone();
            Ok(())
        }
        Value::Array(items) => items.iter_mut().try_for_each(|item| fill(item, ids)),
        Value::Object(fields) => fields.values_mut().try_for_each(|field| fill(field, ids)),
        _ => Ok(()),
    }
}

fn send(server: &Server, example: &Example, ids: &BTreeMap<&str, String>) -> Result<(), String> {
    let mut value: Value =
        serde_json::from_str(&example.body).map_err(|err| format!("not JSON: {err}"))?;
    fill(&mut value, ids)?;
    let page = &ids["<page id>"];
    let (tool, arguments) = match example.tag.as_str() {
        "key" => (
            "set_key",
            json!({ "page": page, "index": KEY_INDEX, "key": value }),
        ),
        "key wide" => (
            "set_key",
            json!({ "page": page, "index": WIDE_KEY, "key": value }),
        ),
        "changes" => ("apply_changes", value),
        "preferences" => ("set_preferences", json!({ "changes": value })),
        "device" => ("set_device_settings", value),
        other => {
            return Err(format!(
                "`json {other}` is not a kind of example the test knows"
            ));
        }
    };
    server.tool(tool, arguments).map(|_| ())
}

#[test]
fn every_example_of_the_guide_is_accepted_as_written() {
    let server = Server::start();
    let files = tempfile::tempdir().unwrap();
    let ids = placeholders(&server, files.path());
    let examples = examples(GUIDE);
    let keys = examples.iter().filter(|e| e.tag.starts_with("key")).count();
    assert!(keys >= 20, "the guide shows {keys} keys");
    assert!(examples.iter().any(|e| e.tag == "changes"));

    let refused: Vec<String> = examples
        .iter()
        .filter_map(|example| {
            send(&server, example, &ids)
                .err()
                .map(|err| format!("guide.md:{} (json {}): {err}", example.line, example.tag))
        })
        .collect();
    assert!(refused.is_empty(), "\n{}", refused.join("\n"));
}

#[test]
fn examples_are_found_with_their_tags_and_lines() {
    let markdown =
        "# A\n```json key\n{}\n```\n```\nplain\n```\n```text\nx\n```\n```json changes\n[]\n```\n";
    let found: Vec<(usize, String, String)> = examples(markdown)
        .into_iter()
        .map(|e| (e.line, e.tag, e.body))
        .collect();
    assert_eq!(
        found,
        [
            (2, "key".into(), "{}\n".into()),
            (11, "changes".into(), "[]\n".into())
        ]
    );
}
