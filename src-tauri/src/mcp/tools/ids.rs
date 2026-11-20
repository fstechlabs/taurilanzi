//! Ids the assistant writes: real ones, or names it gives what a lot of changes creates so later
//! changes of the same lot can point to it before it has an id.

use std::collections::BTreeMap;

use serde_json::Value;
use uuid::Uuid;

/// Names start with this, so they never pass for a real id.
const NAME_MARK: char = '$';

pub fn parse_id(text: &str, what: &str) -> Result<Uuid, String> {
    Uuid::parse_str(text.trim()).map_err(|_| {
        format!("{text:?} is not the id of a {what}: ids look like 0b3c…, from get_overview")
    })
}

#[derive(Default)]
pub struct Names(BTreeMap<String, Uuid>);

impl Names {
    /// An id for something new, remembered under `name` if there is one.
    pub fn declare(&mut self, name: Option<&str>) -> Result<Uuid, String> {
        let id = Uuid::new_v4();
        if let Some(name) = name {
            if !name.starts_with(NAME_MARK) || name.len() < 2 {
                return Err(format!(
                    "names start with {NAME_MARK}, like \"$scenes\": {name:?}"
                ));
            }
            if self.0.insert(name.to_owned(), id).is_some() {
                return Err(format!("{name} is named twice"));
            }
        }
        Ok(id)
    }

    pub fn resolve(&self, text: &str, what: &str) -> Result<Uuid, String> {
        if text.starts_with(NAME_MARK) {
            return self.0.get(text).copied().ok_or_else(|| {
                format!("{text} was not given to anything created before in this call")
            });
        }
        parse_id(text, what)
    }

    /// Replaces the names in the actions of a key that point to a page or a profile with their
    /// ids, and nowhere else: a text that reads like a name stays as it is.
    pub fn resolve_in(&self, value: &mut Value) {
        match value {
            Value::Object(fields) => {
                let target = match fields.get("type").and_then(Value::as_str) {
                    Some("open_page") => Some("page"),
                    Some("switch_profile") => Some("profile"),
                    _ => None,
                };
                if let Some(Value::String(text)) = target.and_then(|field| fields.get_mut(field))
                    && let Some(id) = self.0.get(text.as_str())
                {
                    *text = id.to_string();
                }
                fields.values_mut().for_each(|field| self.resolve_in(field));
            }
            Value::Array(items) => items.iter_mut().for_each(|item| self.resolve_in(item)),
            _ => {}
        }
    }

    pub fn created(&self) -> &BTreeMap<String, Uuid> {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn names_only_stand_for_pages_and_profiles_where_actions_point() {
        let mut names = Names::default();
        let page = names.declare(Some("$home")).unwrap();
        let profile = names.declare(Some("$work")).unwrap();
        let mut key = json!({
            "appearance": { "layers": [ { "type": "text", "content": "$home" } ] },
            "actions": [
                { "type": "open_page", "page": "$home" },
                { "type": "type_text", "text": "$work" },
            ],
            "long_press": [ { "type": "switch_profile", "profile": "$work" } ],
        });
        names.resolve_in(&mut key);
        assert_eq!(key["actions"][0]["page"], page.to_string());
        assert_eq!(key["long_press"][0]["profile"], profile.to_string());
        assert_eq!(key["appearance"]["layers"][0]["content"], "$home");
        assert_eq!(key["actions"][1]["text"], "$work");
    }
}
