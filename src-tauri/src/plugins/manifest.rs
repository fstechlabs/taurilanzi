//! A plugin package: `manifest.json` at the root of a folder ending in `.ulanziPlugin`, as Ulanzi
//! Studio reads it, or in `.sdPlugin`, as the Stream Deck and Mirabox's Stream Dock read theirs,
//! with optional `<language>.json` translations next to it.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

/// How a plugin talks to its host, which the ending of its folder tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// Ulanzi Studio's: commands named `cmd`, keys named `column_row`.
    Ulanzi,
    /// Elgato's Stream Deck one, which Mirabox's Stream Dock speaks too: `event`s about a
    /// `context`, the plugin started with `-port`, `-pluginUUID`, `-registerEvent` and `-info`.
    StreamDeck,
}

/// Main service of the plugin. As in Ulanzi Studio, the extension of `CodePath` decides how it
/// runs, and `Type` tells Python apart when the extension does not. Stream Deck plugins may also
/// be a program of their own for each system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Runtime {
    Node(PathBuf),
    Html(String),
    Python(PathBuf),
    Native(PathBuf),
    /// Nothing runs: a plugin made only of images, or one of Ulanzi's built in ones.
    None,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Manifest {
    /// Stream Dock plugins leave it out: their folder names them.
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
    /// Only in Stream Deck manifests, which tells them apart in a package without its folder.
    #[serde(rename = "SDKVersion", default)]
    pub sdk_version: Value,
    /// Set by Mirabox's own SDK, whose pages connect through `connectMiraBoxSDSocket`.
    #[serde(rename = "APIVersion", default)]
    pub api_version: Value,
    #[serde(default)]
    pub code_path: String,
    /// What runs on each system, over `CodePath`, in Stream Deck plugins.
    #[serde(default)]
    pub code_path_mac: String,
    #[serde(default)]
    pub code_path_win: String,
    /// `JavaScript` or `Python`.
    #[serde(rename = "Type", default)]
    pub kind: String,
    /// The inspector of the actions that name none, in Stream Deck plugins.
    #[serde(default)]
    pub property_inspector_path: String,
    #[serde(default)]
    pub actions: Vec<ActionManifest>,
    /// A script that sets up the app the plugin works with, such as a remote script for a music
    /// app. Ulanzi Studio runs it each time the plugin starts.
    #[serde(default)]
    pub install_to_deps_app: Option<DepsInstaller>,
    /// The Stream Deck or Stream Dock the plugin was made for.
    #[serde(default)]
    pub software: Software,
    /// Apps whose launch and exit the plugin hears of: bundle ids on macOS, programs on Windows.
    #[serde(default)]
    pub applications_to_monitor: ApplicationsToMonitor,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Software {
    #[serde(default)]
    pub minimum_version: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ApplicationsToMonitor {
    #[serde(default)]
    pub mac: Vec<String>,
    #[serde(default)]
    pub windows: Vec<String>,
}

impl ApplicationsToMonitor {
    /// Those of this system; Linux has none, as no Stream Deck runs there.
    pub fn here(&self) -> &[String] {
        if cfg!(target_os = "macos") {
            &self.mac
        } else if cfg!(target_os = "windows") {
            &self.windows
        } else {
            &[]
        }
    }
}

/// The name manifests give the system the app runs on.
pub fn this_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DepsInstaller {
    #[serde(rename = "Type", default)]
    pub kind: String,
    #[serde(default)]
    pub code_path: String,
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
    /// Stream Deck plugins: the host does not switch between two states on each press.
    #[serde(default)]
    pub disable_automatic_states: bool,
    /// The settings a new key starts with, in Stream Dock plugins.
    #[serde(default)]
    pub settings: Value,
    /// Whether the user may give the key a title of their own over the plugin's.
    #[serde(default = "yes")]
    pub user_title_enabled: bool,
    #[serde(default = "yes")]
    pub visible_in_actions_list: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StateManifest {
    #[serde(default)]
    pub image: String,
    /// The title the host draws over the image, in Stream Deck plugins, until the plugin sets one.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub show_title: Option<bool>,
    /// `top`, `middle` or `bottom`.
    #[serde(default)]
    pub title_alignment: String,
    #[serde(default)]
    pub title_color: String,
    /// In points of a 72-point key; a number or a string of one.
    #[serde(default)]
    pub font_size: Value,
    #[serde(default)]
    pub font_family: String,
    /// `Regular`, `Bold`, `Italic` or `Bold Italic`.
    #[serde(default)]
    pub font_style: String,
}

fn keypad() -> Vec<String> {
    vec!["Keypad".into()]
}

/// The texts of a `<language>.json` file that the host shows; the rest is for the plugin.
/// Ulanzi's list the actions in order; Stream Deck's key them by their id.
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
    #[serde(flatten)]
    by_id: HashMap<String, Value>,
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

    pub fn runtime(&self, dir: &Path, protocol: Protocol) -> Runtime {
        let code = match protocol {
            Protocol::Ulanzi => self.code_path.as_str(),
            Protocol::StreamDeck => self.stream_deck_code(),
        };
        let path = code.trim().trim_start_matches("./");
        if path.is_empty() {
            return Runtime::None;
        }
        let lower = path.to_ascii_lowercase();
        let script = [".js", ".mjs", ".cjs"].iter().any(|e| lower.ends_with(e));
        if lower.ends_with(".html") || lower.ends_with(".htm") {
            Runtime::Html(path.to_owned())
        } else if lower.ends_with(".py")
            || lower.ends_with(".pyw")
            || (self.kind.trim().eq_ignore_ascii_case("python") && !script)
        {
            Runtime::Python(dir.join(path))
        } else if protocol == Protocol::StreamDeck && !script {
            // Windows programs are named without their `.exe` too, as that system finds them.
            let program = dir.join(path);
            let exe = dir.join(format!("{path}.exe"));
            Runtime::Native(if !program.is_file() && exe.is_file() {
                exe
            } else {
                program
            })
        } else {
            Runtime::Node(dir.join(path))
        }
    }

    /// The code for this system, as the Stream Deck picks it; Linux, which neither knows, takes
    /// what is shared.
    fn stream_deck_code(&self) -> &str {
        let own = if cfg!(target_os = "windows") {
            &self.code_path_win
        } else if cfg!(target_os = "macos") {
            &self.code_path_mac
        } else {
            &self.code_path
        };
        [
            own,
            &self.code_path,
            &self.code_path_mac,
            &self.code_path_win,
        ]
        .into_iter()
        .map(|c| c.trim())
        .find(|c| !c.is_empty())
        .unwrap_or_default()
    }

    /// Whether it was made for Mirabox's Stream Dock rather than Elgato's Stream Deck, which
    /// decides the version of the host it is told and how its titles are drawn. Mirabox's SDK sets
    /// `APIVersion`; its older plugins ask for a Stream Dock of version 2 or 3, or for none,
    /// while Elgato's ask for a Stream Deck of version 4 or later.
    pub fn made_for_stream_dock(&self) -> bool {
        if !self.api_version.is_null() {
            return true;
        }
        let major = self
            .software
            .minimum_version
            .trim()
            .split('.')
            .next()
            .and_then(|m| m.parse::<u32>().ok());
        match major {
            Some(major) => major < 4,
            None => self.sdk_version.as_u64().is_none_or(|v| v <= 1),
        }
    }

    /// The script of `InstallToDepsApp`, which Ulanzi Studio only runs when it is JavaScript.
    pub fn deps_installer(&self, dir: &Path) -> Option<PathBuf> {
        let installer = self.install_to_deps_app.as_ref()?;
        let path = installer.code_path.trim();
        let kind = installer.kind.trim();
        let javascript = kind.is_empty() || kind.eq_ignore_ascii_case("javascript");
        if !javascript || !path.to_ascii_lowercase().ends_with(".js") {
            return None;
        }
        super::registry::inside(dir, path)
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
        let mut listed = translation.actions.into_iter();
        for action in &mut self.actions {
            let text = listed.next().unwrap_or_default();
            let keyed = translation
                .by_id
                .get(&action.uuid)
                .and_then(|value| serde_json::from_value::<ActionTranslation>(value.clone()).ok())
                .unwrap_or_default();
            if let Some(name) = text.name.or(keyed.name).filter(|n| !n.trim().is_empty()) {
                action.name = name;
            }
            if let Some(tooltip) = text
                .tooltip
                .or(keyed.tooltip)
                .filter(|t| !t.trim().is_empty())
            {
                action.tooltip = tooltip;
            }
        }
    }

    /// Stream Deck plugins with no id of their own are named by their folder or, failing that,
    /// by what their actions start with.
    pub fn fill_id(&mut self, folder: &str) {
        if !self.uuid.trim().is_empty() {
            return;
        }
        let stem = super::registry::package_stem(folder);
        self.uuid = if super::registry::valid_plugin_id(stem) {
            stem.to_owned()
        } else {
            self.actions
                .first()
                .and_then(|a| a.uuid.rsplit_once('.'))
                .map(|(prefix, _)| prefix.to_owned())
                .unwrap_or_default()
        };
    }
}

impl ActionManifest {
    /// An empty list means keys too: Stream Dock's own examples leave it so.
    pub fn supports_keys(&self) -> bool {
        self.controllers.is_empty()
            || self
                .controllers
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
            manifest.runtime(dir, Protocol::Ulanzi)
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
        assert_eq!(missing.runtime(dir, Protocol::Ulanzi), Runtime::None);
    }

    #[test]
    fn the_type_tells_python_apart_when_the_extension_does_not() {
        let dir = Path::new("/plugins/x.ulanziPlugin");
        let runtime = |code: &str, kind: &str| {
            let manifest: Manifest =
                serde_json::from_value(json!({ "CodePath": code, "Type": kind })).unwrap();
            manifest.runtime(dir, Protocol::Ulanzi)
        };
        assert_eq!(
            runtime("plugin/app.py", "Python"),
            Runtime::Python(dir.join("plugin/app.py"))
        );
        assert_eq!(
            runtime("plugin/app.pyw", "JavaScript"),
            Runtime::Python(dir.join("plugin/app.pyw"))
        );
        assert_eq!(
            runtime("plugin/main", " python "),
            Runtime::Python(dir.join("plugin/main"))
        );
        assert_eq!(
            runtime("plugin/app.js", "Python"),
            Runtime::Node(dir.join("plugin/app.js")),
            "a script of Node is run by Node whatever the type says"
        );
        assert_eq!(
            runtime("plugin/app.html", "Python"),
            Runtime::Html("plugin/app.html".into())
        );
    }

    #[test]
    fn the_setup_script_runs_only_when_it_is_javascript_inside_the_plugin() {
        let dir = Path::new("/plugins/x.ulanziPlugin");
        let installer = |value: Value| {
            let manifest: Manifest =
                serde_json::from_value(json!({ "InstallToDepsApp": value })).unwrap();
            manifest.deps_installer(dir)
        };
        assert_eq!(
            installer(json!({ "Type": "JavaScript", "CodePath": "./dist/install.js" })),
            Some(dir.join("dist/install.js"))
        );
        assert_eq!(
            installer(json!({ "CodePath": "install.js" })),
            Some(dir.join("install.js"))
        );
        assert_eq!(
            installer(json!({ "Type": "Python", "CodePath": "install.py" })),
            None
        );
        assert_eq!(
            installer(json!({ "Type": "JavaScript", "CodePath": "install.py" })),
            None
        );
        assert_eq!(
            installer(json!({ "Type": "JavaScript", "CodePath": "../../evil.js" })),
            None
        );
        let none: Manifest = serde_json::from_value(json!({})).unwrap();
        assert_eq!(none.deps_installer(dir), None);
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
