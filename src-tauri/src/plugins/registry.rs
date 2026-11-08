//! The plugins installed in the app's own folder. Those of Ulanzi Studio are left alone: the user
//! installs here what the app should run.

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use log::warn;

use super::manifest::{ActionManifest, Manifest, Protocol, Runtime};

/// The folder of an Ulanzi package ends like this; the SDKs look for it to find the plugin root.
pub const PACKAGE_SUFFIX: &str = ".ulanziPlugin";
/// And that of a Stream Deck or Stream Dock one like this, in any case.
pub const STREAM_DECK_SUFFIX: &str = ".sdPlugin";
/// Images Stream Deck manifests name without their extension, tried in this order.
const IMAGE_EXTENSIONS: [&str; 5] = [".svg", "@2x.png", ".png", ".jpg", ".gif"];

#[derive(Debug, Clone)]
pub struct Plugin {
    pub manifest: Manifest,
    pub dir: PathBuf,
    /// The folder name, which ends in `.ulanziPlugin` or `.sdPlugin`.
    pub folder: String,
    pub protocol: Protocol,
    pub runtime: Runtime,
}

#[derive(Debug, Default)]
pub struct Registry {
    plugins: Vec<Plugin>,
}

impl Registry {
    /// Reads every package, skipping those without a valid manifest or with an id already seen.
    pub fn scan(root: &Path, language: &str) -> Self {
        let mut seen = HashSet::new();
        let mut plugins = Vec::new();
        for dir in packages(root) {
            match load(&dir, language) {
                Ok(plugin) if seen.insert(plugin.manifest.uuid.clone()) => plugins.push(plugin),
                Ok(_) => warn!(
                    "plugin in {} skipped: its id is already installed",
                    dir.display()
                ),
                Err(err) => warn!("plugin in {} skipped: {err}", dir.display()),
            }
        }
        plugins.sort_by_key(|p| p.manifest.name.to_lowercase());
        Self { plugins }
    }

    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins
    }

    pub fn plugin(&self, id: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.manifest.uuid == id)
    }

    /// The installed plugin an id belongs to: the plugin itself, the one whose manifest declares
    /// the action, or else the one whose id starts the id of the action, the longest if several
    /// do. Some published actions are not named after their plugin.
    pub fn owner(&self, uuid: &str) -> Option<&Plugin> {
        self.plugin(uuid)
            .or_else(|| {
                self.plugins
                    .iter()
                    .find(|p| p.manifest.actions.iter().any(|a| a.uuid == uuid))
            })
            .or_else(|| {
                self.plugins
                    .iter()
                    .filter(|p| {
                        uuid.strip_prefix(p.manifest.uuid.as_str())
                            .is_some_and(|rest| rest.starts_with('.'))
                    })
                    .max_by_key(|p| p.manifest.uuid.len())
            })
    }

    /// The plugin of an id as above, or by Ulanzi's convention when it is not installed.
    pub fn plugin_id(&self, uuid: &str) -> String {
        self.owner(uuid)
            .map_or_else(|| plugin_of(uuid), |p| p.manifest.uuid.clone())
    }

    pub fn action(&self, action: &str) -> Option<(&Plugin, &ActionManifest)> {
        let plugin = self.owner(action)?;
        let manifest = plugin.manifest.actions.iter().find(|a| a.uuid == action)?;
        Some((plugin, manifest))
    }
}

/// Ulanzi's convention: four segments for a plugin, more for an action, whose first four are its
/// plugin. Some published plugins have three, so the registry is asked first.
pub fn plugin_of(uuid: &str) -> String {
    uuid.split('.').take(4).collect::<Vec<_>>().join(".")
}

pub fn is_plugin_id(uuid: &str) -> bool {
    uuid.split('.').count() == 4
}

/// What a plugin may be called: reverse-DNS, two parts or more, letters, digits, `-` and `_`.
/// What an action may be called: a plugin id and one part more at least. Ulanzi's have five or
/// more; Stream Deck ones, three or four.
pub fn valid_action_id(uuid: &str) -> bool {
    uuid.split('.').count() >= 3 && valid_plugin_id(uuid)
}

pub fn valid_plugin_id(uuid: &str) -> bool {
    uuid.split('.').count() >= 2
        && uuid.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

impl Plugin {
    /// A file of the plugin, refusing anything that would leave its folder.
    pub fn file(&self, relative: &str) -> Option<PathBuf> {
        inside(&self.dir, relative)
    }
}

/// `relative` resolved under `root`, unless it climbs out of it. A leading `/` is taken as the
/// root of the plugin, as browsers do with the pages it serves.
pub fn inside(root: &Path, relative: &str) -> Option<PathBuf> {
    let relative = relative
        .trim()
        .trim_start_matches("./")
        .trim_start_matches('/');
    let path = Path::new(relative);
    if relative.is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return None;
    }
    Some(root.join(path))
}

fn packages(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| protocol_of(n).is_some())
        })
        .collect();
    dirs.sort();
    dirs
}

/// What a package folder speaks, by how its name ends; `None` if it is not one.
pub fn protocol_of(folder: &str) -> Option<Protocol> {
    let lower = folder.to_ascii_lowercase();
    if lower.ends_with(&PACKAGE_SUFFIX.to_ascii_lowercase()) {
        Some(Protocol::Ulanzi)
    } else if lower.ends_with(&STREAM_DECK_SUFFIX.to_ascii_lowercase()) {
        Some(Protocol::StreamDeck)
    } else {
        None
    }
}

/// The name of a package folder without its ending.
pub fn package_stem(folder: &str) -> &str {
    let suffix = match protocol_of(folder) {
        Some(Protocol::Ulanzi) => PACKAGE_SUFFIX.len(),
        Some(Protocol::StreamDeck) => STREAM_DECK_SUFFIX.len(),
        None => 0,
    };
    &folder[..folder.len() - suffix]
}

/// The manifest of a package, with the id Stream Dock plugins leave to their folder.
pub fn read_manifest(dir: &Path) -> std::io::Result<Manifest> {
    let mut manifest = Manifest::read(dir)?;
    let folder = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    manifest.fill_id(folder);
    Ok(manifest)
}

fn load(dir: &Path, language: &str) -> Result<Plugin, String> {
    let folder = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    let protocol = protocol_of(&folder).ok_or("its folder is not a package")?;
    let mut manifest = read_manifest(dir).map_err(|err| err.to_string())?;
    if !valid_plugin_id(&manifest.uuid) {
        return Err(format!("its id {:?} is not a plugin id", manifest.uuid));
    }
    manifest.localize(dir, language);
    with_image_files(&mut manifest, dir);
    let runtime = manifest.runtime(dir, protocol);
    Ok(Plugin {
        folder,
        dir: dir.to_path_buf(),
        protocol,
        runtime,
        manifest,
    })
}

/// An image of a package, named as Stream Deck plugins name them: with or without its extension.
pub fn image_name(dir: &Path, given: &str) -> Option<String> {
    let given = given.trim();
    if given.is_empty() {
        return None;
    }
    if inside(dir, given).is_some_and(|p| p.is_file()) {
        return Some(given.to_owned());
    }
    IMAGE_EXTENSIONS
        .iter()
        .map(|ext| format!("{given}{ext}"))
        .find(|name| inside(dir, name).is_some_and(|p| p.is_file()))
}

/// Stream Deck manifests name images without an extension: the host finds the file.
fn with_image_files(manifest: &mut Manifest, dir: &Path) {
    let found = |path: &mut String| {
        if let Some(file) = image_name(dir, path) {
            *path = file;
        }
    };
    found(&mut manifest.icon);
    for action in &mut manifest.actions {
        found(&mut action.icon);
        for state in &mut action.states {
            found(&mut state.image);
        }
        if action.property_inspector_path.trim().is_empty() {
            action
                .property_inspector_path
                .clone_from(&manifest.property_inspector_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn package(root: &Path, folder: &str, manifest: &Value) -> PathBuf {
        let dir = root.join(folder);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        dir
    }

    fn plugin(id: &str, name: &str) -> Value {
        json!({
            "UUID": id,
            "Name": name,
            "Actions": [{ "UUID": format!("{id}.act"), "Name": "Act" }],
        })
    }

    #[test]
    fn scans_the_packages_of_its_folder() {
        let root = tempfile::tempdir().unwrap();
        let own = root.path().join("own");
        package(
            &own,
            "com.a.b.zeta.ulanziPlugin",
            &plugin("com.a.b.zeta", "Zeta"),
        );
        package(
            &own,
            "com.x.y.alpha.ulanziPlugin",
            &plugin("com.x.y.alpha", "alpha"),
        );
        package(
            &own,
            "copy.ulanziPlugin",
            &plugin("com.a.b.zeta", "Copy of Zeta"),
        );
        package(&own, "plain", &plugin("com.a.b.plain", "Not a package"));
        package(&own, "bad.ulanziPlugin", &plugin("not an id", "Bad id"));
        package(
            &own,
            "three.ulanziPlugin",
            &plugin("com.three.parts", "Three"),
        );
        let broken = own.join("broken.ulanziPlugin");
        fs::create_dir_all(&broken).unwrap();
        fs::write(broken.join("manifest.json"), "{").unwrap();
        fs::create_dir_all(own.join("empty.ulanziPlugin")).unwrap();
        fs::write(own.join("file.ulanziPlugin"), "not a folder").unwrap();

        let registry = Registry::scan(&own, "en");
        let found: Vec<(&str, &str)> = registry
            .plugins()
            .iter()
            .map(|p| (p.manifest.uuid.as_str(), p.folder.as_str()))
            .collect();
        assert_eq!(
            found,
            vec![
                ("com.x.y.alpha", "com.x.y.alpha.ulanziPlugin"),
                ("com.three.parts", "three.ulanziPlugin"),
                ("com.a.b.zeta", "com.a.b.zeta.ulanziPlugin"),
            ]
        );
        assert_eq!(
            registry.plugin_id("com.three.parts.speedtest"),
            "com.three.parts"
        );
        assert_eq!(registry.plugin_id("com.three.parts"), "com.three.parts");
        assert_eq!(registry.plugin_id("com.a.b.zeta.act"), "com.a.b.zeta");
        assert_eq!(
            registry.plugin_id("com.three.partsx.act"),
            "com.three.partsx.act",
            "not by text alone"
        );
        assert_eq!(
            registry.plugin_id("com.q.q.q.act"),
            "com.q.q.q",
            "Ulanzi's rule when not installed"
        );
        let zeta = registry.plugin("com.a.b.zeta").unwrap();
        assert_eq!(zeta.manifest.name, "Zeta");
        assert_eq!(zeta.dir, own.join("com.a.b.zeta.ulanziPlugin"));
        assert_eq!(zeta.runtime, Runtime::None);
        assert!(registry.plugin("com.a.b.plain").is_none());
    }

    #[test]
    fn missing_folders_give_an_empty_registry() {
        let root = tempfile::tempdir().unwrap();
        let registry = Registry::scan(&root.path().join("nothing"), "en");
        assert!(registry.plugins().is_empty());
    }

    #[test]
    fn finds_actions_by_their_plugin() {
        let root = tempfile::tempdir().unwrap();
        package(root.path(), "p.ulanziPlugin", &plugin("com.a.b.c", "P"));
        let registry = Registry::scan(root.path(), "en");
        let (found, action) = registry.action("com.a.b.c.act").unwrap();
        assert_eq!(found.manifest.uuid, "com.a.b.c");
        assert_eq!(action.name, "Act");
        assert!(registry.action("com.a.b.c.other").is_none());
        assert!(registry.action("com.q.q.q.act").is_none());
    }

    #[test]
    fn plugin_ids_are_the_first_four_segments() {
        assert_eq!(plugin_of("com.a.b.c.act.sub"), "com.a.b.c");
        assert_eq!(plugin_of("com.a.b.c"), "com.a.b.c");
        assert_eq!(plugin_of("com.a"), "com.a");
        assert!(is_plugin_id("com.a.b.c"));
        assert!(!is_plugin_id("com.a.b"));
        assert!(valid_plugin_id("com.narlei.internetspeedtest"));
        assert!(valid_plugin_id("com.a.b.c"));
        for bad in ["com", "com..a", "com a.b", ".com.a", "com/a.b"] {
            assert!(!valid_plugin_id(bad), "{bad}");
        }
        assert!(!is_plugin_id("com.a.b.c.act"));
    }

    #[test]
    fn files_stay_inside_the_folder() {
        let root = Path::new("/plugins/p.ulanziPlugin");
        assert_eq!(
            inside(root, "images/a.png"),
            Some(root.join("images/a.png"))
        );
        assert_eq!(inside(root, "./a.png"), Some(root.join("a.png")));
        assert_eq!(
            inside(root, "/etc/passwd"),
            Some(root.join("etc/passwd")),
            "an absolute path is read inside the folder"
        );
        assert_eq!(inside(root, ".."), None);
        assert_eq!(inside(root, "../other/a.png"), None);
        assert_eq!(inside(root, "images/../../a.png"), None);
        assert_eq!(inside(root, ""), None);
        assert_eq!(inside(root, "  "), None);
    }

    #[test]
    fn stream_deck_packages_are_named_by_their_folder_and_find_their_images() {
        let root = tempfile::tempdir().unwrap();
        let dir = package(
            root.path(),
            "com.mirabox.demo.SDPlugin",
            &json!({
                "Name": "Demo",
                "Icon": "images/icon",
                "PropertyInspectorPath": "pi/index.html",
                "Actions": [
                    { "UUID": "com.mirabox.demo.one", "Name": "One", "States": [{ "Image": "images/one" }] },
                    { "UUID": "com.mirabox.demo.two", "Name": "Two", "PropertyInspectorPath": "pi/two.html" },
                ],
            }),
        );
        fs::create_dir_all(dir.join("images")).unwrap();
        fs::write(dir.join("images/icon@2x.png"), "png").unwrap();
        fs::write(dir.join("images/one.svg"), "<svg/>").unwrap();
        fs::write(
            dir.join("es.json"),
            json!({ "Name": "Demostración", "com.mirabox.demo.two": { "Name": "Dos" } })
                .to_string(),
        )
        .unwrap();
        let numbered = package(
            root.path(),
            "20250118000451.sdPlugin",
            &json!({ "Actions": [{ "UUID": "com.hotspot.discord.mute" }] }),
        );

        let registry = Registry::scan(root.path(), "es");
        let demo = registry.plugin("com.mirabox.demo").unwrap();
        assert_eq!(demo.protocol, Protocol::StreamDeck);
        assert_eq!(demo.manifest.name, "Demostración");
        assert_eq!(demo.manifest.icon, "images/icon@2x.png");
        let [one, two] = &demo.manifest.actions[..] else {
            panic!("two actions");
        };
        assert_eq!(one.states[0].image, "images/one.svg");
        assert_eq!(one.property_inspector_path, "pi/index.html", "the plugin's");
        assert_eq!(two.property_inspector_path, "pi/two.html");
        assert_eq!(two.name, "Dos");
        assert!(one.supports_keys(), "no controllers named is keys");
        let discord = registry.plugin("com.hotspot.discord").unwrap();
        assert_eq!(discord.dir, numbered, "by what its actions start with");
        assert_eq!(package_stem("a.b.sdPlugin"), "a.b");
        assert_eq!(protocol_of("a.b.ulanziPlugin"), Some(Protocol::Ulanzi));
        assert_eq!(protocol_of("a.b.zip"), None);
    }

    #[test]
    fn an_action_belongs_to_the_plugin_that_declares_it() {
        let root = tempfile::tempdir().unwrap();
        package(
            root.path(),
            "com.mirabox.weather.sdPlugin",
            &json!({
                "Name": "Weather",
                "Actions": [{ "UUID": "com.hotspot.weather.action1", "Name": "Now" }],
            }),
        );
        package(
            root.path(),
            "com.hotspot.ulanziPlugin",
            &plugin("com.hotspot", "Other"),
        );
        let registry = Registry::scan(root.path(), "en");
        assert_eq!(
            registry.plugin_id("com.hotspot.weather.action1"),
            "com.mirabox.weather",
            "declared, though another plugin's id starts it"
        );
        assert_eq!(registry.plugin_id("com.hotspot.other"), "com.hotspot");
    }
}
