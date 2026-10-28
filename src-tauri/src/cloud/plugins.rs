//! After each sync, this computer tells the account which plugins it has, and installs from the
//! community catalog the ones the other computers have. Plugins installed from a file are only
//! named: the cloud never hands out code.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::documents::SyncExport;
use crate::config::KnownPlugin;

/// What sync needs of the plugins of this computer.
pub trait PluginInstaller: Send + Sync {
    /// The plugins installed here, by id, with their names.
    fn installed(&self) -> BTreeMap<String, String>;
    /// The package of the community catalog an installed plugin came from: `Some(None)` when it
    /// is not in the catalog, `None` when the catalog cannot be read now.
    fn catalog_of(&self, plugin: &str) -> Option<Option<String>>;
    /// Installs a package of the community catalog and answers the id of its plugin; `None`
    /// when the catalog has no such package that runs here.
    fn install(&self, catalog: &str) -> Result<Option<String>, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum MissingReason {
    /// Installed from a file on another computer: it has to be installed here by hand.
    FromFile,
    /// Its package in the catalog does not run on this system.
    NotHere,
    /// Installing it failed; sync tries again next time.
    Failed,
}

/// A plugin of another computer this one does not have.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct MissingPlugin {
    pub id: String,
    pub name: String,
    pub reason: MissingReason,
}

pub struct Reconciled {
    /// Plugins of this computer the account did not know, or not from where.
    pub learned: BTreeMap<String, KnownPlugin>,
    pub missing: Vec<MissingPlugin>,
}

/// `checked` keeps the plugins already looked up in the catalog without finding them, so a
/// plugin installed from a file does not read the catalog on every sync.
pub fn reconcile(
    installer: &dyn PluginInstaller,
    export: &SyncExport,
    checked: &mut BTreeSet<String>,
) -> Reconciled {
    let installed = installer.installed();
    let known = &export.settings.plugins;
    let mut learned = BTreeMap::new();
    for (id, name) in &installed {
        let known_catalog = known.get(id).map(|k| k.catalog.is_some());
        if known_catalog == Some(true) || (known_catalog == Some(false) && checked.contains(id)) {
            continue;
        }
        // Out of reach, the catalog is asked again next time.
        let Some(catalog) = installer.catalog_of(id) else {
            continue;
        };
        if catalog.is_none() {
            checked.insert(id.clone());
            if known_catalog.is_some() {
                continue;
            }
        }
        learned.insert(
            id.clone(),
            KnownPlugin {
                name: name.clone(),
                catalog,
            },
        );
    }

    let mut missing = Vec::new();
    for (id, plugin) in known {
        if installed.contains_key(id) || export.declined_plugins.contains(id) {
            continue;
        }
        let reason = match &plugin.catalog {
            None => Some(MissingReason::FromFile),
            Some(catalog) => match installer.install(catalog) {
                Ok(Some(got)) if got == *id => {
                    log::info!("plugin {id} installed, as another computer has it");
                    None
                }
                Ok(Some(got)) => {
                    log::warn!("installing {catalog} gave plugin {got} instead of {id}");
                    Some(MissingReason::Failed)
                }
                Ok(None) => Some(MissingReason::NotHere),
                Err(err) => {
                    log::warn!("could not install plugin {id} from the catalog: {err}");
                    Some(MissingReason::Failed)
                }
            },
        };
        if let Some(reason) = reason {
            missing.push(MissingPlugin {
                id: id.clone(),
                name: plugin.name.clone(),
                reason,
            });
        }
    }
    Reconciled { learned, missing }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::cloud::SharedSettings;
    use crate::config::{AssetStore, first_run};

    /// Plugins of this computer, a catalog that names some of them, and what got installed.
    #[derive(Default)]
    struct Shelf {
        installed: Mutex<BTreeMap<String, String>>,
        catalog: BTreeMap<String, String>,
        reachable: bool,
        lookups: Mutex<Vec<String>>,
        fails: Vec<String>,
    }

    impl PluginInstaller for Shelf {
        fn installed(&self) -> BTreeMap<String, String> {
            self.installed.lock().unwrap().clone()
        }

        fn catalog_of(&self, plugin: &str) -> Option<Option<String>> {
            self.lookups.lock().unwrap().push(plugin.to_owned());
            self.reachable.then(|| {
                self.catalog
                    .iter()
                    .find(|(_, id)| *id == plugin)
                    .map(|(package, _)| package.clone())
            })
        }

        fn install(&self, catalog: &str) -> Result<Option<String>, String> {
            if self.fails.iter().any(|f| f == catalog) {
                return Err("no network".into());
            }
            let Some(id) = self.catalog.get(catalog) else {
                return Ok(None);
            };
            self.installed
                .lock()
                .unwrap()
                .insert(id.clone(), format!("{id} name"));
            Ok(Some(id.clone()))
        }
    }

    fn export(known: &[(&str, Option<&str>)], declined: &[&str]) -> SyncExport {
        let dir = tempfile::tempdir().unwrap();
        let config = first_run(&AssetStore::open(dir.path().to_path_buf()).unwrap()).unwrap();
        let mut settings = SharedSettings::of(&config);
        settings.plugins = known
            .iter()
            .map(|(id, catalog)| {
                (
                    id.to_string(),
                    KnownPlugin {
                        name: format!("{id} name"),
                        catalog: catalog.map(str::to_owned),
                    },
                )
            })
            .collect();
        SyncExport {
            profiles: Vec::new(),
            local: BTreeSet::new(),
            settings,
            edited: BTreeMap::new(),
            declined_plugins: declined.iter().map(|d| d.to_string()).collect(),
        }
    }

    fn shelf(installed: &[&str], catalog: &[(&str, &str)]) -> Shelf {
        Shelf {
            installed: Mutex::new(
                installed
                    .iter()
                    .map(|id| (id.to_string(), format!("{id} name")))
                    .collect(),
            ),
            catalog: catalog
                .iter()
                .map(|(package, id)| (package.to_string(), id.to_string()))
                .collect(),
            reachable: true,
            ..Shelf::default()
        }
    }

    #[test]
    fn tells_the_account_what_this_computer_has_and_where_it_came_from() {
        let shelf = shelf(
            &["a.b.c.d", "from.a.file.x"],
            &[("a.b.c.d.ulanziPlugin", "a.b.c.d")],
        );
        let mut checked = BTreeSet::new();
        let done = reconcile(&shelf, &export(&[], &[]), &mut checked);
        assert_eq!(
            done.learned["a.b.c.d"].catalog.as_deref(),
            Some("a.b.c.d.ulanziPlugin")
        );
        assert_eq!(done.learned["from.a.file.x"].catalog, None);
        assert!(done.missing.is_empty());
    }

    #[test]
    fn installs_from_the_catalog_what_the_other_computers_have() {
        let shelf = shelf(&[], &[("a.b.c.d.ulanziPlugin", "a.b.c.d")]);
        let known = [
            ("a.b.c.d", Some("a.b.c.d.ulanziPlugin")),
            ("from.a.file.x", None),
            ("other.os.only.x", Some("other.os.only.x.ulanziPlugin")),
        ];
        let done = reconcile(&shelf, &export(&known, &[]), &mut BTreeSet::new());
        assert!(shelf.installed().contains_key("a.b.c.d"));
        let missing: Vec<(&str, MissingReason)> = done
            .missing
            .iter()
            .map(|m| (m.id.as_str(), m.reason))
            .collect();
        assert_eq!(
            missing,
            [
                ("from.a.file.x", MissingReason::FromFile),
                ("other.os.only.x", MissingReason::NotHere),
            ]
        );
    }

    #[test]
    fn leaves_out_what_was_uninstalled_here() {
        let shelf = shelf(&[], &[("a.b.c.d.ulanziPlugin", "a.b.c.d")]);
        let known = [("a.b.c.d", Some("a.b.c.d.ulanziPlugin"))];
        let done = reconcile(&shelf, &export(&known, &["a.b.c.d"]), &mut BTreeSet::new());
        assert!(shelf.installed().is_empty());
        assert!(done.missing.is_empty());
    }

    #[test]
    fn a_failed_install_is_tried_again_later() {
        let mut shelf = shelf(&[], &[("a.b.c.d.ulanziPlugin", "a.b.c.d")]);
        shelf.fails = vec!["a.b.c.d.ulanziPlugin".into()];
        let known = [("a.b.c.d", Some("a.b.c.d.ulanziPlugin"))];
        let done = reconcile(&shelf, &export(&known, &[]), &mut BTreeSet::new());
        assert_eq!(done.missing[0].reason, MissingReason::Failed);
    }

    #[test]
    fn asks_the_catalog_once_for_plugins_it_does_not_have() {
        let shelf = shelf(&["from.a.file.x"], &[]);
        let known = [("from.a.file.x", None)];
        let mut checked = BTreeSet::new();
        for _ in 0..3 {
            let done = reconcile(&shelf, &export(&known, &[]), &mut checked);
            assert!(done.learned.is_empty());
        }
        assert_eq!(shelf.lookups.lock().unwrap().len(), 1);
    }

    #[test]
    fn waits_for_the_catalog_when_it_cannot_be_read() {
        let mut shelf = shelf(&["a.b.c.d"], &[("a.b.c.d.ulanziPlugin", "a.b.c.d")]);
        shelf.reachable = false;
        let done = reconcile(&shelf, &export(&[], &[]), &mut BTreeSet::new());
        assert!(
            done.learned.is_empty(),
            "not noted as from a file while the catalog is away"
        );
    }
}
