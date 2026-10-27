//! What plugins ask of the desktop app around the engine: windows, dialogs and messages.

pub trait Desktop: Send + Sync {
    /// A short message for the user, from a plugin.
    fn toast(&self, plugin: &str, message: &str);
    /// A small window with a page of the plugin.
    fn open_view(&self, url: &str, width: f64, height: f64);
    /// A file or folder picker; blocks until the user chooses.
    fn select(&self, kind: &str, filter: &str) -> Option<String>;
    /// Runs the main service of a plugin made of HTML in a page nobody sees.
    fn run_page(&self, plugin: &str, url: &str);
    fn stop_page(&self, plugin: &str);
}

/// For tests: nothing is shown.
#[cfg(test)]
pub struct Headless;

#[cfg(test)]
impl Desktop for Headless {
    fn toast(&self, plugin: &str, message: &str) {
        log::info!("toast from {plugin}: {message}");
    }
    fn open_view(&self, _url: &str, _width: f64, _height: f64) {}
    fn select(&self, _kind: &str, _filter: &str) -> Option<String> {
        None
    }
    fn run_page(&self, _plugin: &str, _url: &str) {}
    fn stop_page(&self, _plugin: &str) {}
}
