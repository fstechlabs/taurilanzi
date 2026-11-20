//! Values that look like passwords or tokens never reach the assistant: plugin settings and the
//! headers and form fields of HTTP actions are read with them hidden, and what the assistant
//! sends back with a hidden value keeps the value it had, as long as it still goes to the same
//! place: a secret never follows a request or a plugin to another address.

use serde_json::Value;

/// What a hidden value reads as. Sent back unchanged, it keeps the value it stands for.
pub const HIDDEN: &str = "<hidden by Taurilanzi>";

/// Names of fields and headers that usually hold a secret, compared without case or separators.
const SECRET_PARTS: [&str; 12] = [
    "token",
    "passw",
    "secret",
    "apikey",
    "authoriz",
    "oauth",
    "authkey",
    "credential",
    "cookie",
    "privatekey",
    "accesskey",
    "sessionid",
];

pub fn is_secret_name(name: &str) -> bool {
    let plain: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    plain == "auth"
        || plain == "pin"
        || plain == "code"
        || SECRET_PARTS.iter().any(|part| plain.contains(part))
}

/// Hides every secret-looking field of a plugin's settings, at any depth.
pub fn hide_settings(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (name, field) in fields.iter_mut() {
                if is_secret_name(name) && holds_something(field) {
                    *field = HIDDEN.into();
                } else {
                    hide_settings(field);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(hide_settings),
        _ => {}
    }
}

/// Hides the secrets of every action in a key, configuration or any part of them, as JSON.
pub fn hide_in_actions(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            match fields.get("type").and_then(Value::as_str) {
                Some("plugin") => {
                    for part in ["settings", "resources"] {
                        if let Some(settings) = fields.get_mut(part) {
                            hide_settings(settings);
                        }
                    }
                    return;
                }
                Some("http") => {
                    if let Some(request) = fields.get_mut("request") {
                        hide_pairs(request);
                    }
                    return;
                }
                _ => {}
            }
            fields.values_mut().for_each(hide_in_actions);
        }
        Value::Array(items) => items.iter_mut().for_each(hide_in_actions),
        _ => {}
    }
}

fn hide_pairs(request: &mut Value) {
    if let Some(headers) = request.get_mut("headers") {
        hide_pair_list(headers);
    }
    if let Some(fields) = request
        .get_mut("body")
        .and_then(|body| body.get_mut("fields"))
    {
        hide_pair_list(fields);
    }
}

fn hide_pair_list(list: &mut Value) {
    for pair in list.as_array_mut().into_iter().flatten() {
        let secret = pair
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(is_secret_name);
        if secret && pair.get("value").is_some_and(holds_something) {
            pair["value"] = HIDDEN.into();
        }
    }
}

fn holds_something(value: &Value) -> bool {
    match value {
        Value::String(text) => !text.is_empty(),
        Value::Number(_) => true,
        _ => false,
    }
}

/// Puts back what `old` had wherever `new` holds a hidden value, matching by path.
pub fn restore(new: &mut Value, old: &Value) -> Result<(), String> {
    restore_at(new, Some(old), "")
}

fn restore_at(new: &mut Value, old: Option<&Value>, path: &str) -> Result<(), String> {
    match new {
        Value::String(text) if text == HIDDEN => match old.filter(|old| !is_hidden(old)) {
            Some(old) => {
                *new = old.clone();
                Ok(())
            }
            None => Err(no_value(path)),
        },
        Value::Object(fields) => {
            for (name, field) in fields.iter_mut() {
                let before = old.and_then(|old| old.get(name.as_str()));
                restore_at(field, before, &format!("{path}/{name}"))?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for (at, item) in items.iter_mut().enumerate() {
                let before = old.and_then(|old| old.get(at));
                restore_at(item, before, &format!("{path}/{at}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Puts back the hidden headers and form fields of a key's HTTP actions, each from an HTTP
/// action of the key it replaces that went to the same place (scheme, host and port). An
/// action pointed somewhere else gets nothing back: the secret would go with it.
pub fn restore_http(new: &mut Value, old: &Value) -> Result<(), String> {
    let mut before = Vec::new();
    collect_requests(old, &mut before);
    restore_requests(new, &before)
}

fn collect_requests<'a>(value: &'a Value, found: &mut Vec<&'a Value>) {
    match value {
        Value::Object(fields) => {
            if fields.get("type").and_then(Value::as_str) == Some("http")
                && let Some(request) = fields.get("request")
            {
                found.push(request);
            }
            fields
                .values()
                .for_each(|field| collect_requests(field, found));
        }
        Value::Array(items) => items.iter().for_each(|item| collect_requests(item, found)),
        _ => {}
    }
}

fn restore_requests(value: &mut Value, before: &[&Value]) -> Result<(), String> {
    match value {
        Value::Object(fields) => {
            if fields.get("type").and_then(Value::as_str) == Some("http")
                && let Some(request) = fields.get_mut("request")
            {
                return restore_request(request, before);
            }
            fields
                .values_mut()
                .try_for_each(|field| restore_requests(field, before))
        }
        Value::Array(items) => items
            .iter_mut()
            .try_for_each(|item| restore_requests(item, before)),
        _ => Ok(()),
    }
}

fn restore_request(request: &mut Value, before: &[&Value]) -> Result<(), String> {
    let place = request.get("url").and_then(Value::as_str).and_then(origin);
    let same_place: Vec<&Value> = before
        .iter()
        .copied()
        .filter(|old| {
            place.is_some() && old.get("url").and_then(Value::as_str).and_then(origin) == place
        })
        .collect();
    for list in [&["headers"][..], &["body", "fields"][..]] {
        let Some(pairs) = pointer_mut(request, list).and_then(Value::as_array_mut) else {
            continue;
        };
        for pair in pairs {
            let Some(name) = pair.get("name").and_then(Value::as_str).map(str::to_owned) else {
                continue;
            };
            let Some(value) = pair.get_mut("value").filter(|value| is_hidden(value)) else {
                continue;
            };
            let saved = same_place
                .iter()
                .find_map(|old| pair_value(old, list, &name));
            *value = match saved {
                Some(saved) => saved.clone(),
                None if before
                    .iter()
                    .any(|old| pair_value(old, list, &name).is_some()) =>
                {
                    return Err(format!(
                        "{name} is hidden and the request now goes somewhere else, so its saved value is not sent there: the user must enter it again in the app"
                    ));
                }
                None => return Err(no_value(&name)),
            };
        }
    }
    Ok(())
}

fn pointer_mut<'a>(value: &'a mut Value, path: &[&str]) -> Option<&'a mut Value> {
    path.iter()
        .try_fold(value, |value, part| value.get_mut(*part))
}

/// The saved value of a header or form field of an old request, by name in any case.
fn pair_value<'a>(request: &'a Value, list: &[&str], name: &str) -> Option<&'a Value> {
    list.iter()
        .try_fold(request, |value, part| value.get(*part))?
        .as_array()?
        .iter()
        .find(|pair| {
            pair.get("name")
                .and_then(Value::as_str)
                .is_some_and(|old| old.eq_ignore_ascii_case(name))
        })
        .and_then(|pair| pair.get("value"))
        .filter(|value| !is_hidden(value))
}

/// Scheme, host and port of an address, as a browser tells sites apart; the user and password
/// part, if any, does not count.
fn origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?.to_ascii_lowercase();
    if host_port.is_empty() {
        return None;
    }
    let has_port = match host_port.rsplit_once(':') {
        Some((_, port)) => !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    };
    let default = match scheme.as_str() {
        "http" => "80",
        "https" => "443",
        _ => return None,
    };
    Some(if has_port {
        format!("{scheme}://{host_port}")
    } else {
        format!("{scheme}://{host_port}:{default}")
    })
}

/// Words of setting names that say where a plugin connects.
const ADDRESS_WORDS: [&str; 14] = [
    "host",
    "hostname",
    "url",
    "uri",
    "server",
    "address",
    "addr",
    "ip",
    "port",
    "endpoint",
    "domain",
    "baseurl",
    "websocket",
    "ws",
];

/// Whether a setting says where a plugin connects, by the words of its name: `serverUrl`,
/// `obs_host` and `ipAddress` do, `report` and `zip` do not.
fn is_address_name(name: &str) -> bool {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut previous_lower = false;
    for c in name.chars() {
        let boundary = !c.is_ascii_alphanumeric() || (c.is_ascii_uppercase() && previous_lower);
        if boundary && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        if c.is_ascii_alphanumeric() {
            word.push(c.to_ascii_lowercase());
        }
        previous_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
    }
    words.push(word);
    words
        .iter()
        .any(|word| ADDRESS_WORDS.contains(&word.as_str()))
}

/// Like `restore`, for a plugin's settings: hidden values are put back only if nothing that
/// says where the plugin connects changed, so a secret never follows the settings elsewhere.
pub fn restore_settings(new: &mut Value, old: &Value) -> Result<(), String> {
    if find_hidden(new).is_none() {
        return Ok(());
    }
    let (mut before, mut after) = (Vec::new(), Vec::new());
    collect_addresses(old, "", &mut before);
    collect_addresses(new, "", &mut after);
    before.sort();
    after.sort();
    if before != after {
        let changed = after
            .iter()
            .find(|field| !before.contains(field))
            .or_else(|| before.iter().find(|field| !after.contains(field)))
            .map_or_else(String::new, |(path, _)| path.clone());
        return Err(format!(
            "{changed} changed, so the hidden values are not sent along: the user must enter them again in the plugin's settings in the app"
        ));
    }
    restore(new, old)
}

fn collect_addresses(value: &Value, path: &str, found: &mut Vec<(String, String)>) {
    match value {
        Value::Object(fields) => {
            for (name, field) in fields {
                let at = format!("{path}/{name}");
                if is_address_name(name) && !field.is_object() && !field.is_array() {
                    found.push((at, field.to_string()));
                } else {
                    collect_addresses(field, &at, found);
                }
            }
        }
        Value::Array(items) => {
            for (at, item) in items.iter().enumerate() {
                collect_addresses(item, &format!("{path}/{at}"), found);
            }
        }
        _ => {}
    }
}

/// Where a hidden value is left after restoring, which would save the placeholder itself.
pub fn find_hidden(value: &Value) -> Option<String> {
    find_hidden_at(value, "")
}

fn find_hidden_at(value: &Value, path: &str) -> Option<String> {
    match value {
        Value::String(_) if is_hidden(value) => Some(if path.is_empty() {
            "/".into()
        } else {
            path.into()
        }),
        Value::Object(fields) => fields
            .iter()
            .find_map(|(name, field)| find_hidden_at(field, &format!("{path}/{name}"))),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(at, item)| find_hidden_at(item, &format!("{path}/{at}"))),
        _ => None,
    }
}

fn is_hidden(value: &Value) -> bool {
    value.as_str() == Some(HIDDEN)
}

fn no_value(path: &str) -> String {
    format!(
        "{path} is hidden and there is no saved value to keep: the user must enter it in the app"
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn hides_what_looks_like_a_secret_and_keeps_the_rest() {
        let mut key = json!({
            "actions": [
                { "type": "plugin", "action": "com.x.y.z", "id": "1",
                  "settings": { "apiKey": "abc", "city": "Madrid", "nested": { "access_token": "t" } } },
                { "type": "http", "request": {
                    "url": "https://example.com",
                    "headers": [ { "name": "Authorization", "value": "Bearer x" },
                                 { "name": "Accept", "value": "text/plain" } ],
                    "body": { "type": "form", "fields": [ { "name": "password", "value": "p" } ] } } }
            ]
        });
        hide_in_actions(&mut key);
        let settings = &key["actions"][0]["settings"];
        assert_eq!(settings["apiKey"], HIDDEN);
        assert_eq!(settings["nested"]["access_token"], HIDDEN);
        assert_eq!(settings["city"], "Madrid");
        let request = &key["actions"][1]["request"];
        assert_eq!(request["headers"][0]["value"], HIDDEN);
        assert_eq!(request["headers"][1]["value"], "text/plain");
        assert_eq!(request["body"]["fields"][0]["value"], HIDDEN);
    }

    #[test]
    fn hidden_values_sent_back_keep_what_they_had() {
        let old = json!({ "token": "real", "list": [ { "password": "p" } ] });
        let mut new = old.clone();
        hide_settings(&mut new);
        new["city"] = "Sevilla".into();
        restore(&mut new, &old).unwrap();
        assert_eq!(new["token"], "real");
        assert_eq!(new["list"][0]["password"], "p");

        let mut invented = json!({ "token": HIDDEN });
        assert!(restore(&mut invented, &json!({})).is_err());
    }

    fn http(url: &str, headers: Value) -> Value {
        json!({ "type": "http", "request": { "method": "GET", "url": url, "headers": headers } })
    }

    #[test]
    fn hidden_headers_come_back_only_to_the_same_place() {
        let old = json!({ "actions": [ http("https://api.example.com/a", json!([
            { "name": "Accept", "value": "a" }, { "name": "X-Api-Key", "value": "k" } ])) ] });

        let mut moved_path = json!({ "long_press": [ http("https://API.example.com:443/other", json!([
            { "name": "x-api-key", "value": HIDDEN } ])) ] });
        restore_http(&mut moved_path, &old).unwrap();
        assert_eq!(
            moved_path["long_press"][0]["request"]["headers"][0]["value"],
            "k"
        );

        for elsewhere in [
            "https://attacker.example/a",
            "http://api.example.com/a",
            "https://api.example.com:8443/a",
            "https://api.example.com@attacker.example/a",
        ] {
            let mut new = json!({ "actions": [ http(elsewhere, json!([
                { "name": "X-Api-Key", "value": HIDDEN } ])) ] });
            let err = restore_http(&mut new, &old).unwrap_err();
            assert!(err.contains("somewhere else"), "{elsewhere}: {err}");
        }

        let mut added = json!({ "actions": [
            http("https://api.example.com/a", json!([ { "name": "X-Api-Key", "value": HIDDEN } ])),
            http("https://attacker.example/x", json!([ { "name": "X-Api-Key", "value": HIDDEN } ])),
        ] });
        assert!(
            restore_http(&mut added, &old).is_err(),
            "a second action elsewhere gets nothing"
        );
    }

    #[test]
    fn a_header_never_becomes_a_form_field() {
        let old = json!({ "actions": [ http("https://api.example.com", json!([
            { "name": "Authorization", "value": "Bearer k" } ])) ] });
        let mut new = json!({ "actions": [ { "type": "http", "request": {
            "url": "https://api.example.com", "headers": [],
            "body": { "type": "form", "fields": [ { "name": "Authorization", "value": HIDDEN } ] } } } ] });
        assert!(restore_http(&mut new, &old).is_err());
    }

    #[test]
    fn plugin_secrets_stay_where_the_plugin_connected() {
        let old = json!({ "serverUrl": "wss://home.local", "password": "p", "zip": "28001" });
        let mut city =
            json!({ "serverUrl": "wss://home.local", "password": HIDDEN, "zip": "41001" });
        restore_settings(&mut city, &old).unwrap();
        assert_eq!(
            city["password"], "p",
            "changing what is not an address keeps it"
        );

        let mut moved =
            json!({ "serverUrl": "wss://attacker", "password": HIDDEN, "zip": "28001" });
        assert!(restore_settings(&mut moved, &old).is_err());
        let mut added =
            json!({ "serverUrl": "wss://home.local", "obs_host": "x", "password": HIDDEN });
        assert!(restore_settings(&mut added, &old).is_err());
        let mut unhidden = json!({ "serverUrl": "wss://attacker", "password": "typed" });
        restore_settings(&mut unhidden, &old).unwrap();

        for (name, address) in [
            ("ipAddress", true),
            ("obs_host", true),
            ("baseURL", true),
            ("report", false),
            ("zip", false),
            ("support", false),
        ] {
            assert_eq!(is_address_name(name), address, "{name}");
        }
    }
}
