mod community;
mod desktop;
mod hotkey;
mod hub;
mod images;
mod install;
#[cfg(target_os = "windows")]
mod job_object;
mod launcher;
mod manifest;
mod node;
mod protocol;
mod registry;
mod server;
mod service;

pub use community::CommunityPlugin;
pub use desktop::Desktop;
pub use hub::{PluginHost, PluginInput, VisualChange};
pub use images::from_file as image_file;
pub use protocol::{Instance, event, key_fields, key_name, settings_object, with_settings};
pub use registry::Registry;
pub use service::{PluginCatalog, Plugins, PluginsSetup};
