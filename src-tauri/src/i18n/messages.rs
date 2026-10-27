//! The texts of the engine, in `locales/es.json` and `locales/en.json`: one file per language,
//! nested by area like the UI's, built into the binary and read by key (`widgets.battery.charging`).

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::LazyLock;

use serde_json::Value;

use super::language::{Language, current};

static ES: LazyLock<HashMap<String, String>> =
    LazyLock::new(|| flatten(include_str!("../../locales/es.json")));
static EN: LazyLock<HashMap<String, String>> =
    LazyLock::new(|| flatten(include_str!("../../locales/en.json")));

/// `{"a": {"b": "x"}}` → `a.b` = `x`.
fn flatten(json: &str) -> HashMap<String, String> {
    fn walk(prefix: &str, value: &Value, out: &mut HashMap<String, String>) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    walk(&path, value, out);
                }
            }
            Value::String(text) => {
                out.insert(prefix.to_owned(), text.clone());
            }
            _ => {}
        }
    }
    let value: Value = serde_json::from_str(json).expect("a locale file is valid JSON");
    let mut out = HashMap::new();
    walk("", &value, &mut out);
    out
}

fn table(language: Language) -> &'static HashMap<String, String> {
    match language {
        Language::Es => &ES,
        Language::En => &EN,
    }
}

/// The text of `key` in the current language, then in English, then the key itself.
pub fn text(key: &'static str) -> &'static str {
    table(current())
        .get(key)
        .or_else(|| EN.get(key))
        .map_or(key, String::as_str)
}

/// `text(key)` with each `{name}` replaced by its value.
pub fn format(key: &'static str, values: &[(&str, &dyn Display)]) -> String {
    let mut out = text(key).to_owned();
    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

#[cfg(test)]
pub fn keys(language: Language) -> Vec<(&'static str, &'static str)> {
    let mut keys: Vec<_> = table(language)
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    keys.sort_unstable();
    keys
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::i18n::set;

    fn placeholders(text: &str) -> BTreeSet<String> {
        text.split('{')
            .skip(1)
            .filter_map(|rest| rest.split_once('}').map(|(name, _)| name.to_owned()))
            .collect()
    }

    #[test]
    fn spanish_and_english_have_the_same_keys_and_placeholders() {
        let es = keys(Language::Es);
        let en = keys(Language::En);
        fn names<'a>(list: &[(&'a str, &str)]) -> Vec<&'a str> {
            list.iter().map(|(k, _)| *k).collect()
        }
        assert_eq!(names(&es), names(&en));
        for ((key, es), (_, en)) in es.iter().zip(&en) {
            assert!(
                !es.trim().is_empty() && !en.trim().is_empty(),
                "{key} is empty"
            );
            assert_eq!(placeholders(es), placeholders(en), "{key}");
        }
    }

    /// Every `tr!` and `trf!` in the code names a key the files have.
    #[test]
    fn every_key_in_the_code_exists() {
        fn sources(dir: &Path, out: &mut Vec<String>) {
            for entry in fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(fs::read_to_string(path).unwrap());
                }
            }
        }
        let mut files = Vec::new();
        sources(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        let known: BTreeSet<&str> = keys(Language::En).into_iter().map(|(k, _)| k).collect();
        let mut missing = Vec::new();
        for file in &files {
            for marker in ["tr!(", "trf!("] {
                for (at, _) in file.match_indices(marker) {
                    let before = file[..at].chars().next_back();
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let Some(rest) = file[at + marker.len()..].trim_start().strip_prefix('"')
                    else {
                        continue;
                    };
                    let key = rest.split('"').next().unwrap_or_default();
                    let looks_like_key = key.contains('.')
                        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '.');
                    if looks_like_key && !known.contains(key) {
                        missing.push(key.to_owned());
                    }
                }
            }
        }
        assert_eq!(missing, Vec::<String>::new());
    }

    #[test]
    fn texts_follow_the_language_and_fill_their_placeholders() {
        let (key, _) = keys(Language::En)
            .into_iter()
            .find(|(key, text)| {
                text.contains('{') && ES.get(*key).map(String::as_str) != Some(*text)
            })
            .expect("some message has a placeholder and differs between languages");
        let name = placeholders(text(key)).into_iter().next().unwrap();
        set(Language::En);
        let english = format(key, &[(&name, &"VALUE")]);
        assert!(
            english.contains("VALUE") && !english.contains('{'),
            "{english}"
        );
        set(Language::Es);
        assert_eq!(text(key), ES[key]);
        assert_eq!(text("no.such.key"), "no.such.key");
    }
}
