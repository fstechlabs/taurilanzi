//! What the tools reach in the app, and how they answer.

use std::sync::Arc;

use rmcp::ErrorData;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;

use crate::config::AssetStore;
use crate::engine::{CommandError, EngineHandle};
use crate::plugins::Plugins;

/// Something the window should know the assistant did.
#[derive(Debug, Clone)]
pub enum Notice {
    /// The configuration changed; undo takes it back when `undoable`.
    Edited { undoable: bool },
    /// The assistant suggests installing a plugin of a store.
    Recommended { id: String, name: String },
}

pub struct Tools {
    pub engine: EngineHandle,
    pub plugins: Option<Arc<Plugins>>,
    pub assets: AssetStore,
    pub notify: Arc<dyn Fn(Notice) + Send + Sync>,
}

pub type Answer = Result<CallToolResult, ErrorData>;

pub fn json(value: impl Serialize) -> Answer {
    Ok(CallToolResult::success(vec![ContentBlock::json(value)?]))
}

pub fn text(message: impl Into<String>) -> Answer {
    Ok(CallToolResult::success(vec![ContentBlock::text(message)]))
}

/// A refusal the assistant reads and can correct, rather than a protocol error.
pub fn refused(message: impl Into<String>) -> Answer {
    Ok(CallToolResult::error(vec![ContentBlock::text(message)]))
}

pub fn engine_error(err: CommandError) -> Answer {
    refused(err.to_string())
}

impl Tools {
    pub fn edited(&self, undoable: bool) {
        (self.notify)(Notice::Edited { undoable });
    }

    pub fn plugins(&self) -> Result<&Arc<Plugins>, String> {
        self.plugins
            .as_ref()
            .ok_or_else(|| "plugins are not running in this app".to_owned())
    }
}
