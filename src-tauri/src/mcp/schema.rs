//! The shapes of the configuration for the assistant, as the TypeScript the UI is generated with:
//! exact, commented, and always in step with the Rust types.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::OnceLock;

const BINDINGS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../src-nuxt/app/ipc/bindings.ts"
));
const TYPES_START: &str = "/* Types */";
const TYPES_END: &str = "/* Tauri Specta runtime */";
const DECLARATION: &str = "\nexport type ";

/// A type and every type it uses, each with its doc comment; `None` if there is no such type.
pub fn describe(name: &str) -> Option<String> {
    let types = types();
    types.get(name)?;
    let mut seen = BTreeSet::from([name.to_owned()]);
    let mut queue = VecDeque::from([name.to_owned()]);
    let mut out = Vec::new();
    while let Some(next) = queue.pop_front() {
        let text = &types[next.as_str()];
        for word in words(text) {
            if types.contains_key(word) && seen.insert(word.to_owned()) {
                queue.push_back(word.to_owned());
            }
        }
        out.push(text.as_str());
    }
    Some(out.join("\n\n"))
}

fn types() -> &'static BTreeMap<String, String> {
    static TYPES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    TYPES.get_or_init(|| parse(BINDINGS))
}

fn parse(bindings: &str) -> BTreeMap<String, String> {
    let start = bindings
        .find(TYPES_START)
        .map_or(0, |at| at + TYPES_START.len());
    let end = bindings.find(TYPES_END).unwrap_or(bindings.len());
    let section = &bindings[start..end];
    let mut starts: Vec<usize> = section
        .match_indices(DECLARATION)
        .map(|(at, _)| at + 1)
        .collect();
    // A doc comment right before a declaration belongs to it, not to the type above.
    for at in &mut starts {
        let before = section[..*at].trim_end();
        if before.ends_with("*/")
            && let Some(comment) = before.rfind("/**")
            && {
                let above = before[..comment].trim_end();
                above.is_empty() || above.ends_with([';', '}', '/'])
            }
        {
            *at = comment;
        }
    }
    let mut types = BTreeMap::new();
    for (i, &at) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(section.len());
        let text = section[at..end].trim();
        if let Some(name) = text
            .split(DECLARATION.trim_start())
            .nth(1)
            .and_then(|rest| words(rest).next())
        {
            types.insert(name.to_owned(), text.to_owned());
        }
    }
    types
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_comes_with_everything_it_is_made_of() {
        let key = describe("Key").unwrap();
        assert!(key.starts_with("export type Key = ") || key.starts_with("/**"));
        for used in [
            "export type Action =",
            "export type Widget =",
            "export type Color =",
        ] {
            assert!(key.contains(used), "{used} is missing");
        }
        assert!(
            !key.contains("export type Snapshot ="),
            "only what a key uses"
        );
        assert!(describe("Nothing").is_none());
    }

    #[test]
    fn doc_comments_stay_with_their_type() {
        let action = &types()["Action"];
        assert!(
            action.starts_with("/**  What a key does when pressed."),
            "{action}"
        );
    }
}
