mod assets;
mod bindings;
mod cloud;
mod commands;
mod error;
mod events;
mod hass;
mod http;
mod images;
mod integrations;
mod mcp;
mod obs;
mod plugins;
mod profiles;
mod system;
mod updates;

pub use bindings::{builder, invoke_handler};
#[cfg(target_os = "macos")]
pub use events::HistoryShortcut;
pub use events::{
    AssistantEdited, CloudChanged, McpChanged, PluginRecommended, PluginToast, UpdateOffered,
    forward,
};
pub use images::{SCHEME, serve as serve_images};
