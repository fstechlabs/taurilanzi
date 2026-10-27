//! A plugin package as Ulanzi Studio reads it: `manifest.json` at the root of a folder ending in
//! `.ulanziPlugin`, with optional `<language>.json` translations next to it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

/// Main service of the plugin: the file extension of `CodePath` decides how it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Runtime {
    Node(PathBuf),
    Html(String),
    Python(PathBuf),
    /// Nothing runs: a plugin made only of images, or one of Ulanzi's built in ones.
    None,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Manifest {
    #[serde(rename = "UUID", default)]
    pub uuid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub code_path: String,
    #[serde(default)]
    pub actions: Vec<ActionManifest>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActionManifest {
    #[serde(rename = "UUID", default)]
    pub uuid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tooltip: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub property_inspector_path: String,
    #[serde(default)]
    pub states: Vec<StateManifest>,
    /// The state a new key starts in. Lower case in the manifest, unlike the rest.
    #[serde(rename = "state", default)]
    pub state: u32,
    #[serde(default = "keypad")]
    pub controllers: Vec<String>,
    /// Empty means every model; a leading `~` excludes one.
    #[serde(default)]
    pub devices: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StateManifest {
    #[serde(default)]
    pub image: String,
}

fn keypad() -> Vec<String> {
    vec!["Keypad".into()]
}

/// The texts of a `<language>.json` file that the host shows; the rest is for the plugin.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Translation {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    actions: Vec<ActionTranslation>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ActionTranslation {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    tooltip: Option<String>,
}

impl Manifest {
    pub fn read(dir: &Path) -> io::Result<Self> {
        let bytes = fs::read(dir.join("manifest.json"))?;
        // Some packages are saved with a byte order mark, which serde_json refuses.
        let text = String::from_utf8_lossy(&bytes);
        let text = text.trim_start_matches('\u{feff}');
        serde_json::from_str(text).map_err(io::Error::other)
    }

    pub fn runtime(&self, dir: &Path) -> Runtime {
        let path = self.code_path.trim().trim_start_matches("./");
        if path.is_empty() {
            return Runtime::None;
        }
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".html") || lower.ends_with(".htm") {
            Runtime::Html(path.to_owned())
        } else if lower.ends_with(".py") {
            Runtime::Python(dir.join(path))
        } else {
            Runtime::Node(dir.join(path))
        }
    }

    /// Names and descriptions in the user's language when the plugin has them. Ulanzi uses
    /// `es_ES.json`, but packages also ship `es.json`; both are tried, then English.
    pub fn localize(&mut self, dir: &Path, language: &str) {
        let Some(translation) = translation(dir, language) else {
            return;
        };
        if let Some(name) = translation.name.filter(|n| !n.trim().is_empty()) {
            self.name = name;
        }
        if let Some(description) = translation.description.filter(|d| !d.trim().is_empty()) {
            self.description = description;
        }
        if let Some(category) = translation.category.filter(|c| !c.trim().is_empty()) {
            self.category = category;
        }
        for (action, text) in self.actions.iter_mut().zip(translation.actions) {
            if let Some(name) = text.name.filter(|n| !n.trim().is_empty()) {
                action.name = name;
            }
            if let Some(tooltip) = text.tooltip.filter(|t| !t.trim().is_empty()) {
                action.tooltip = tooltip;
            }
        }
    }
}

impl ActionManifest {
    pub fn supports_keys(&self) -> bool {
        self.controllers
            .iter()
            .any(|c| c.eq_ignore_ascii_case("keypad"))
    }

    /// Whether the action can go on a dock of this model, as the `Devices` list says.
    pub fn supports_device(&self, model: &str) -> bool {
        let excluded = self
            .devices
            .iter()
            .filter_map(|d| d.strip_prefix('~'))
            .any(|d| d.eq_ignore_ascii_case(model));
        let included: Vec<&String> = self
            .devices
            .iter()
            .filter(|d| !d.starts_with('~'))
            .collect();
        !excluded && (included.is_empty() || included.iter().any(|d| d.eq_ignore_ascii_case(model)))
    }

    /// The image of a state, relative to the plugin folder.
    pub fn state_image(&self, state: u32) -> Option<&str> {
        self.states
            .get(state as usize)
            .or_else(|| self.states.first())
            .map(|s| s.image.as_str())
            .filter(|i| !i.is_empty())
            .or_else(|| Some(self.icon.as_str()).filter(|i| !i.is_empty()))
    }
}

/// `es-ES` → `es_ES.json`, `es.json`, then `en.json`.
fn translation(dir: &Path, language: &str) -> Option<Translation> {
    let full = language.replace('-', "_");
    let short = full.split('_').next().unwrap_or_default().to_owned();
    [full, short, "en".into()]
        .into_iter()
        .filter(|name| !name.is_empty())
        .find_map(|name| {
            let bytes = fs::read(dir.join(format!("{name}.json"))).ok()?;
            let text = String::from_utf8_lossy(&bytes);
            serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}'))
                .ok()
                .and_then(|value| serde_json::from_value(value).ok())
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn write(dir: &Path, name: &str, value: &Value) {
        fs::write(dir.join(name), serde_json::to_vec(value).unwrap()).unwrap();
    }

    fn manifest(dir: &Path) -> Manifest {
        write(
            dir,
            "manifest.json",
            &json!({
                "UUID": "com.example.test.plugin",
                "Name": "Plugin",
                "Description": "English",
                "Category": "Tools",
                "Actions": [
                    { "UUID": "com.example.test.plugin.one", "Name": "One", "Tooltip": "First" },
                    { "UUID": "com.example.test.plugin.two", "Name": "Two", "Tooltip": "Second" },
                ],
            }),
        );
        Manifest::read(dir).unwrap()
    }

    fn action(value: Value) -> ActionManifest {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn reads_pascal_case_with_a_byte_order_mark_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let text = r#"{
            "UUID": "com.example.test.plugin",
            "Name": "Plugin",
            "Author": "Someone",
            "Version": "1.2.0",
            "Icon": "images/icon.png",
            "CodePath": "app.js",
            "Actions": [
                {
                    "UUID": "com.example.test.plugin.act",
                    "Name": "Act",
                    "PropertyInspectorPath": "pi.html",
                    "States": [{ "Image": "a.png" }, { "Image": "b.png" }],
                    "state": 1
                },
                { "UUID": "com.example.test.plugin.dial", "Controllers": ["Encoder"] }
            ]
        }"#;
        fs::write(dir.path().join("manifest.json"), format!("\u{feff}{text}")).unwrap();
        let manifest = Manifest::read(dir.path()).unwrap();
        assert_eq!(manifest.uuid, "com.example.test.plugin");
        assert_eq!(manifest.author, "Someone");
        assert_eq!(manifest.version, "1.2.0");
        assert_eq!(manifest.code_path, "app.js");
        assert_eq!(manifest.description, "");
        let [act, dial] = &manifest.actions[..] else {
            panic!("two actions");
        };
        assert_eq!(act.state, 1);
        assert_eq!(act.property_inspector_path, "pi.html");
        assert_eq!(act.states.len(), 2);
        assert_eq!(act.controllers, vec!["Keypad".to_owned()]);
        assert!(act.supports_keys());
        assert!(act.devices.is_empty());
        assert_eq!(dial.state, 0);
        assert!(!dial.supports_keys());
    }

    #[test]
    fn a_manifest_that_is_missing_or_not_json_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Manifest::read(dir.path()).is_err());
        fs::write(dir.path().join("manifest.json"), "{ not json").unwrap();
        assert!(Manifest::read(dir.path()).is_err());
    }

    #[test]
    fn the_extension_of_the_code_path_decides_the_runtime() {
        let dir = Path::new("/plugins/x.ulanziPlugin");
        let runtime = |code: &str| {
            let manifest: Manifest = serde_json::from_value(json!({ "CodePath": code })).unwrap();
            manifest.runtime(dir)
        };
        assert_eq!(
            runtime("./plugin/app.js"),
            Runtime::Node(dir.join("plugin/app.js"))
        );
        assert_eq!(runtime("main.mjs"), Runtime::Node(dir.join("main.mjs")));
        assert_eq!(runtime("index.HTML"), Runtime::Html("index.HTML".into()));
        assert_eq!(
            runtime("./web/page.htm"),
            Runtime::Html("web/page.htm".into())
        );
        assert_eq!(runtime("main.py"), Runtime::Python(dir.join("main.py")));
        assert_eq!(runtime(""), Runtime::None);
        assert_eq!(runtime("   "), Runtime::None);
        let missing: Manifest = serde_json::from_value(json!({})).unwrap();
        assert_eq!(missing.runtime(dir), Runtime::None);
    }

    #[test]
    fn localizes_from_the_full_language_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut plugin = manifest(dir.path());
        write(
            dir.path(),
            "es_ES.json",
            &json!({ "Name": "Complemento", "Actions": [{ "Name": "Uno" }] }),
        );
        write(dir.path(), "es.json", &json!({ "Name": "Corto" }));
        plugin.localize(dir.path(), "es-ES");
        assert_eq!(plugin.name, "Complemento");
        assert_eq!(plugin.description, "English");
        assert_eq!(plugin.actions[0].name, "Uno");
        assert_eq!(plugin.actions[0].tooltip, "First");
        assert_eq!(plugin.actions[1].name, "Two");
    }

    #[test]
    fn localizes_from_the_short_language_then_english() {
        let dir = tempfile::tempdir().unwrap();
        let mut plugin = manifest(dir.path());
        write(
            dir.path(),
            "es.json",
            &json!({
                "Name": "Complemento",
                "Description": "En español",
                "Category": "Herramientas",
                "Actions": [{ "Name": " " }, { "Name": "Dos", "Tooltip": "Segunda" }],
            }),
        );
        write(dir.path(), "en.json", &json!({ "Name": "English name" }));
        plugin.localize(dir.path(), "es_ES");
        assert_eq!(plugin.name, "Complemento");
        assert_eq!(plugin.description, "En español");
        assert_eq!(plugin.category, "Herramientas");
        assert_eq!(
            plugin.actions[0].name, "One",
            "a blank text keeps the manifest's"
        );
        assert_eq!(plugin.actions[1].name, "Dos");
        assert_eq!(plugin.actions[1].tooltip, "Segunda");

        let dir = tempfile::tempdir().unwrap();
        let mut plugin = manifest(dir.path());
        fs::write(dir.path().join("de.json"), "{ broken").unwrap();
        write(dir.path(), "en.json", &json!({ "Name": "English name" }));
        plugin.localize(dir.path(), "de-DE");
        assert_eq!(plugin.name, "English name");
    }

    #[test]
    fn without_translations_nothing_changes() {
        let dir = tempfile::tempdir().unwrap();
        let mut plugin = manifest(dir.path());
        plugin.localize(dir.path(), "fr-FR");
        assert_eq!(plugin.name, "Plugin");
        assert_eq!(plugin.actions[1].name, "Two");
    }

    #[test]
    fn devices_include_or_exclude_models() {
        let any = action(json!({}));
        assert!(any.supports_device("D200"));
        let only = action(json!({ "Devices": ["D200"] }));
        assert!(only.supports_device("d200"));
        assert!(!only.supports_device("D100H"));
        let except = action(json!({ "Devices": ["~D200"] }));
        assert!(!except.supports_device("D200"));
        assert!(except.supports_device("D100H"));
        let mixed = action(json!({ "Devices": ["D200", "D100H", "~D100H"] }));
        assert!(mixed.supports_device("D200"));
        assert!(!mixed.supports_device("D100H"));
    }

    #[test]
    fn the_image_of_a_state_falls_back_to_the_first_then_the_icon() {
        let both = action(json!({
            "Icon": "icon.png",
            "States": [{ "Image": "off.png" }, { "Image": "on.png" }],
        }));
        assert_eq!(both.state_image(0), Some("off.png"));
        assert_eq!(both.state_image(1), Some("on.png"));
        assert_eq!(both.state_image(5), Some("off.png"));
        let blank = action(json!({ "Icon": "icon.png", "States": [{ "Image": "" }] }));
        assert_eq!(blank.state_image(0), Some("icon.png"));
        let icon = action(json!({ "Icon": "icon.png" }));
        assert_eq!(icon.state_image(0), Some("icon.png"));
        assert_eq!(action(json!({})).state_image(0), None);
    }
}
