//! The MCP server's face: its name, what it tells the assistant first, and its tools.

use std::sync::Arc;

use rmcp::ServerHandler;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::tool_handler;

use super::tools::Tools;

const INSTRUCTIONS: &str = include_str!("instructions.md");

#[derive(Clone)]
pub struct Assistant {
    pub(super) tools: Arc<Tools>,
    router: ToolRouter<Self>,
}

impl Assistant {
    pub fn new(tools: Arc<Tools>) -> Self {
        Self {
            tools,
            router: Self::reading()
                + Self::rendering()
                + Self::editing()
                + Self::settings()
                + Self::integrations()
                + Self::plugin_tools()
                + Self::assets(),
        }
    }

    /// Every tool, for the tests that check nothing configurable is left out.
    #[cfg(test)]
    pub fn tool_names(&self) -> Vec<String> {
        self.router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect()
    }
}

#[tool_handler(router = self.router)]
impl ServerHandler for Assistant {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("taurilanzi", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}
