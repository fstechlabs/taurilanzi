//! Installing a plugin from the ZIP the stores hand out, `<id>.ulanziPlugin/manifest.json` inside,
//! or from a package folder; and removing one installed in the app's folder.

use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use super::manifest::Manifest;
use super::registry::{PACKAGE_SUFFIX, valid_plugin_id};

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error(
        "{}",
        tr!("plugins.install.notAPlugin")
    )]
    NotAPlugin,
    #[error(
        "{}",
        tr!("plugins.install.unsafePaths")
    )]
    UnsafePath,
    #[error(
        "{}",
        trf!("plugins.install.badManifest", reason = .0)
    )]
    BadManifest(String),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{}", trf!("plugins.install.damagedZip", reason = .0))]
    Zip(#[from] zip::result::ZipError),
}

/// Copies the package into `plugins`, replacing an older version of the same folder. Returns the
/// id of the plugin.
pub fn install(source: &Path, plugins: &Path) -> Result<String, InstallError> {
    fs::create_dir_all(plugins)?;
    let staging = tempfile_dir(plugins)?;
    let result = (|| {
        let package = if source.is_dir() {
            copy_package(source, &staging)?
        } else {
            unzip(source, &staging)?
        };
        let manifest =
            Manifest::read(&package).map_err(|e| InstallError::BadManifest(e.to_string()))?;
        if !valid_plugin_id(&manifest.uuid) {
            return Err(InstallError::BadManifest(trf!(
                "plugins.install.invalidId",
                id = format!("{:?}", manifest.uuid)
            )));
        }
        let name = package
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(InstallError::NotAPlugin)?
            .to_owned();
        let target = plugins.join(&name);
        if target.exists() {
            fs::remove_dir_all(&target)?;
        }
        for other in copies_of(&manifest.uuid, plugins) {
            fs::remove_dir_all(other)?;
        }
        fs::rename(&package, &target)?;
        unquarantine(&target);
        Ok(manifest.uuid)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

pub fn uninstall(dir: &Path, plugins: &Path) -> io::Result<()> {
    let climbs = dir.components().any(|c| {
        !matches!(
            c,
            Component::Normal(_) | Component::RootDir | Component::Prefix(_)
        )
    });
    if climbs || !dir.starts_with(plugins) || dir == plugins {
        return Err(io::Error::other(tr!("plugins.install.notRemovable")));
    }
    fs::remove_dir_all(dir)
}

/// Packages of the same plugin under another folder name, which a new version replaces.
fn copies_of(id: &str, plugins: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(plugins) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|dir| {
            dir.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(PACKAGE_SUFFIX))
                && Manifest::read(dir).is_ok_and(|m| m.uuid == id)
        })
        .collect()
}

fn tempfile_dir(plugins: &Path) -> io::Result<PathBuf> {
    let dir = plugins.join(format!(".installing-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// A folder chosen by the user: the package itself, or a folder holding one.
fn copy_package(source: &Path, staging: &Path) -> Result<PathBuf, InstallError> {
    let package = if is_package(source) {
        source.to_path_buf()
    } else {
        fs::read_dir(source)?
            .flatten()
            .map(|e| e.path())
            .find(|p| is_package(p))
            .ok_or(InstallError::NotAPlugin)?
    };
    let target = staging.join(package.file_name().ok_or(InstallError::NotAPlugin)?);
    copy_dir(&package, &target)?;
    Ok(target)
}

fn is_package(path: &Path) -> bool {
    path.is_dir()
        && path.join("manifest.json").is_file()
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(PACKAGE_SUFFIX))
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&path, &target)?;
        } else if kind.is_file() {
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

/// Every entry must sit under one `<id>.ulanziPlugin/` folder, without absolute paths or `..`.
/// Leftovers of macOS archiving are skipped.
/// A ZIP holds the `.ulanziPlugin` folder, or (as some packages do) its contents with the
/// manifest at the top, which then go into a folder named after the plugin.
fn unzip(source: &Path, staging: &Path) -> Result<PathBuf, InstallError> {
    let mut archive = zip::ZipArchive::new(File::open(source)?)?;
    let flat = archive
        .file_names()
        .flatten()
        .any(|name| name.replace('\\', "/") == "manifest.json");
    let mut root: Option<String> = flat.then(|| FLAT_ROOT.to_owned());
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name()?.replace('\\', "/");
        if name.starts_with("__MACOSX/") || name.ends_with(".DS_Store") {
            continue;
        }
        let path = Path::new(&name);
        if path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(InstallError::UnsafePath);
        }
        let target = if flat {
            staging.join(FLAT_ROOT).join(path)
        } else {
            let first = name.split('/').next().unwrap_or_default().to_owned();
            if !first.ends_with(PACKAGE_SUFFIX) {
                return Err(InstallError::NotAPlugin);
            }
            match &root {
                Some(existing) if *existing != first => return Err(InstallError::UnsafePath),
                None => root = Some(first),
                _ => {}
            }
            staging.join(path)
        };
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = File::create(&target)?;
            io::copy(&mut entry, &mut file)?;
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o755 | 0o600))?;
            }
        }
    }
    let package = staging.join(root.ok_or(InstallError::NotAPlugin)?);
    if !package.join("manifest.json").is_file() {
        return Err(InstallError::NotAPlugin);
    }
    if !flat {
        return Ok(package);
    }
    let manifest =
        Manifest::read(&package).map_err(|e| InstallError::BadManifest(e.to_string()))?;
    if !valid_plugin_id(&manifest.uuid) {
        return Err(InstallError::BadManifest(format!(
            "el id {:?} no es válido",
            manifest.uuid
        )));
    }
    let named = staging.join(format!("{}{PACKAGE_SUFFIX}", manifest.uuid));
    fs::rename(&package, &named)?;
    Ok(named)
}

/// Where a ZIP without its package folder is unpacked before the folder gets its name.
const FLAT_ROOT: &str = "unpacked";

/// Files downloaded from the internet carry a quarantine mark that stops their programs.
fn unquarantine(dir: &Path) {
    if cfg!(target_os = "macos") {
        // The system's: a Python package can put another `xattr` first in the PATH.
        let _ = Command::new("/usr/bin/xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(dir)
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use serde_json::json;
    use zip::write::{SimpleFileOptions, ZipWriter};

    use super::*;

    const FOLDER: &str = "com.a.b.c.ulanziPlugin";

    fn manifest(id: &str, version: &str) -> String {
        json!({ "UUID": id, "Name": "P", "Version": version }).to_string()
    }

    fn zip(dir: &Path, entries: &[(&str, &str)]) -> PathBuf {
        let path = dir.join("plugin.zip");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        for (name, content) in entries {
            if name.ends_with('/') {
                zip.add_directory(*name, SimpleFileOptions::default())
                    .unwrap();
            } else {
                zip.start_file(*name, SimpleFileOptions::default()).unwrap();
                zip.write_all(content.as_bytes()).unwrap();
            }
        }
        zip.finish().unwrap();
        path
    }

    fn package(parent: &Path, version: &str) -> PathBuf {
        let dir = parent.join(FOLDER);
        fs::create_dir_all(dir.join("images")).unwrap();
        fs::write(dir.join("manifest.json"), manifest("com.a.b.c", version)).unwrap();
        fs::write(dir.join("images/icon.svg"), "<svg/>").unwrap();
        dir
    }

    fn version(plugins: &Path) -> String {
        Manifest::read(&plugins.join(FOLDER)).unwrap().version
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_zip_without_its_folder_gets_one_named_after_the_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        let source = zip(
            dir.path(),
            &[
                ("manifest.json", &manifest("com.flat.plugin", "1.0.0")),
                ("images/", ""),
                ("images/icon.svg", "<svg/>"),
                ("__MACOSX/._manifest.json", "junk"),
            ],
        );
        assert_eq!(install(&source, &plugins).unwrap(), "com.flat.plugin");
        assert_eq!(
            entries(&plugins),
            vec!["com.flat.plugin.ulanziPlugin".to_owned()]
        );
        assert!(
            plugins
                .join("com.flat.plugin.ulanziPlugin/images/icon.svg")
                .is_file()
        );
        let escaping = zip(
            dir.path(),
            &[
                ("manifest.json", &manifest("com.flat.plugin", "1.0.0")),
                ("../evil", "x"),
            ],
        );
        assert!(matches!(
            install(&escaping, &plugins),
            Err(InstallError::UnsafePath)
        ));
    }

    #[test]
    fn a_new_version_under_another_folder_replaces_the_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        install(&package(&dir.path().join("old"), "1.0.0"), &plugins).unwrap();
        let renamed = dir.path().join("new/com.a.b.renamed.ulanziPlugin");
        fs::create_dir_all(&renamed).unwrap();
        fs::write(
            renamed.join("manifest.json"),
            manifest("com.a.b.c", "2.0.0"),
        )
        .unwrap();
        assert_eq!(install(&renamed, &plugins).unwrap(), "com.a.b.c");
        assert_eq!(
            entries(&plugins),
            vec!["com.a.b.renamed.ulanziPlugin".to_owned()]
        );
    }

    #[test]
    fn installs_a_package_folder_or_a_folder_holding_one() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        let source = package(&dir.path().join("download"), "1.0.0");
        assert_eq!(install(&source, &plugins).unwrap(), "com.a.b.c");
        assert_eq!(entries(&plugins), vec![FOLDER.to_owned()]);
        assert!(plugins.join(FOLDER).join("images/icon.svg").is_file());

        fs::remove_dir_all(&plugins).unwrap();
        assert_eq!(
            install(&dir.path().join("download"), &plugins).unwrap(),
            "com.a.b.c"
        );
        assert_eq!(entries(&plugins), vec![FOLDER.to_owned()]);
        assert!(
            source.join("manifest.json").is_file(),
            "the source is copied, not moved"
        );
    }

    #[test]
    fn installs_a_zip_skipping_macos_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        let manifest = manifest("com.a.b.c", "1.0.0");
        let source = zip(
            dir.path(),
            &[
                ("__MACOSX/", ""),
                ("__MACOSX/._manifest.json", "junk"),
                ("com.a.b.c.ulanziPlugin/", ""),
                ("com.a.b.c.ulanziPlugin/.DS_Store", "junk"),
                ("com.a.b.c.ulanziPlugin/manifest.json", &manifest),
                ("com.a.b.c.ulanziPlugin/plugin/app.js", "console.log(1)"),
            ],
        );
        assert_eq!(install(&source, &plugins).unwrap(), "com.a.b.c");
        assert_eq!(entries(&plugins), vec![FOLDER.to_owned()]);
        assert_eq!(
            entries(&plugins.join(FOLDER)),
            vec!["manifest.json".to_owned(), "plugin".to_owned()]
        );
        assert_eq!(
            fs::read_to_string(plugins.join(FOLDER).join("plugin/app.js")).unwrap(),
            "console.log(1)"
        );
    }

    #[test]
    fn a_new_version_replaces_the_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        let old = package(&dir.path().join("old"), "1.0.0");
        fs::write(old.join("old.txt"), "old").unwrap();
        install(&old, &plugins).unwrap();
        let manifest = manifest("com.a.b.c", "2.0.0");
        let new = zip(
            dir.path(),
            &[("com.a.b.c.ulanziPlugin/manifest.json", &manifest)],
        );
        install(&new, &plugins).unwrap();
        assert_eq!(version(&plugins), "2.0.0");
        assert!(!plugins.join(FOLDER).join("old.txt").exists());
        assert_eq!(entries(&plugins), vec![FOLDER.to_owned()]);
    }

    #[test]
    fn refuses_zips_that_leave_the_package() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        let manifest = manifest("com.a.b.c", "1.0.0");
        let cases: [&[(&str, &str)]; 3] = [
            &[
                ("com.a.b.c.ulanziPlugin/manifest.json", &manifest),
                ("com.a.b.c.ulanziPlugin/../evil.txt", "x"),
            ],
            &[
                ("com.a.b.c.ulanziPlugin/manifest.json", &manifest),
                ("/tmp/evil.txt", "x"),
            ],
            &[
                ("com.a.b.c.ulanziPlugin/manifest.json", &manifest),
                ("other.ulanziPlugin/manifest.json", &manifest),
            ],
        ];
        for entries_of_zip in cases {
            let source = zip(dir.path(), entries_of_zip);
            assert!(
                matches!(install(&source, &plugins), Err(InstallError::UnsafePath)),
                "{entries_of_zip:?}"
            );
        }
        assert!(!dir.path().join("evil.txt").exists());
        assert!(entries(&plugins).is_empty(), "nothing is left behind");
    }

    #[test]
    fn refuses_what_is_not_a_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        let manifest = manifest("com.a.b.c", "1.0.0");
        let no_manifest = zip(dir.path(), &[("com.a.b.c.ulanziPlugin/readme.txt", "x")]);
        assert!(matches!(
            install(&no_manifest, &plugins),
            Err(InstallError::NotAPlugin)
        ));
        let other_folder = zip(dir.path(), &[("folder/manifest.json", &manifest)]);
        assert!(matches!(
            install(&other_folder, &plugins),
            Err(InstallError::NotAPlugin)
        ));
        let empty = zip(dir.path(), &[]);
        assert!(matches!(
            install(&empty, &plugins),
            Err(InstallError::NotAPlugin)
        ));
        let not_zip = dir.path().join("plugin.zip");
        fs::write(&not_zip, "not a zip").unwrap();
        assert!(matches!(
            install(&not_zip, &plugins),
            Err(InstallError::Zip(_))
        ));
        let folder = dir.path().join("nothing");
        fs::create_dir_all(folder.join("x.ulanziPlugin")).unwrap();
        assert!(matches!(
            install(&folder, &plugins),
            Err(InstallError::NotAPlugin)
        ));
        assert!(entries(&plugins).is_empty());
    }

    #[test]
    fn refuses_bad_manifests_and_keeps_the_installed_version() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        install(&package(&dir.path().join("good"), "1.0.0"), &plugins).unwrap();
        let short = manifest("com a", "2.0.0");
        let bad_id = zip(
            dir.path(),
            &[("com.a.b.c.ulanziPlugin/manifest.json", &short)],
        );
        assert!(matches!(
            install(&bad_id, &plugins),
            Err(InstallError::BadManifest(_))
        ));
        let broken = zip(dir.path(), &[("com.a.b.c.ulanziPlugin/manifest.json", "{")]);
        assert!(matches!(
            install(&broken, &plugins),
            Err(InstallError::BadManifest(_))
        ));
        assert_eq!(version(&plugins), "1.0.0");
        assert_eq!(entries(&plugins), vec![FOLDER.to_owned()]);
    }

    #[test]
    fn uninstalls_only_inside_the_plugins_folder() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        install(&package(&dir.path().join("src"), "1.0.0"), &plugins).unwrap();
        let elsewhere = dir.path().join("src").join(FOLDER);
        assert!(uninstall(&elsewhere, &plugins).is_err());
        assert!(elsewhere.is_dir());
        assert!(uninstall(&plugins, &plugins).is_err());
        assert!(plugins.is_dir());
        uninstall(&plugins.join(FOLDER), &plugins).unwrap();
        assert!(entries(&plugins).is_empty());
    }

    #[test]
    fn uninstall_refuses_paths_that_climb_out() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let victim = dir.path().join("victim");
        fs::create_dir_all(&victim).unwrap();
        assert!(uninstall(&plugins.join("../victim"), &plugins).is_err());
        assert!(victim.is_dir());
    }
}
