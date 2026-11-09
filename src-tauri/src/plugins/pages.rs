//! The page of each plugin installed from a store, as the store links it, for those whose
//! manifest names none: kept apart from the plugin's folder, which each update replaces.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use log::warn;

pub struct Pages {
    file: PathBuf,
    known: Mutex<BTreeMap<String, String>>,
}

impl Pages {
    pub fn open(file: PathBuf) -> Self {
        let known = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            file,
            known: Mutex::new(known),
        }
    }

    pub fn get(&self, plugin: &str) -> Option<String> {
        self.known
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(plugin)
            .cloned()
    }

    /// The page the store gives, or none, which forgets an older one.
    pub fn set(&self, plugin: &str, page: Option<String>) {
        let mut known = self.known.lock().unwrap_or_else(PoisonError::into_inner);
        let changed = match page.filter(|p| web_page(p)) {
            Some(page) => known.insert(plugin.to_owned(), page.clone()) != Some(page),
            None => known.remove(plugin).is_some(),
        };
        if changed {
            save(&self.file, &known);
        }
    }
}

/// Only what the browser may open.
pub fn web_page(url: &str) -> bool {
    url.starts_with("https://")
        && url.len() > "https://".len()
        && !url.contains(char::is_whitespace)
}

fn save(file: &Path, known: &BTreeMap<String, String>) {
    let written = serde_json::to_vec_pretty(known)
        .map_err(std::io::Error::other)
        .and_then(|bytes| fs::write(file, bytes));
    if let Err(err) = written {
        warn!("could not save the pages of the plugins: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_page_of_each_plugin_from_its_store() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("plugin-pages.json");
        let pages = Pages::open(file.clone());
        pages.set("com.a.b", Some("https://github.com/a/b".into()));
        pages.set("com.a.c", Some("http://a.example.com".into()));
        pages.set("com.a.d", Some("https://a.example.com/x y".into()));
        assert_eq!(
            pages.get("com.a.b").as_deref(),
            Some("https://github.com/a/b")
        );
        assert_eq!(
            pages.get("com.a.c"),
            None,
            "only pages the browser may open"
        );
        assert_eq!(pages.get("com.a.d"), None);

        let again = Pages::open(file.clone());
        assert_eq!(
            again.get("com.a.b").as_deref(),
            Some("https://github.com/a/b")
        );
        again.set("com.a.b", None);
        assert_eq!(Pages::open(file).get("com.a.b"), None, "forgotten for good");
    }
}
