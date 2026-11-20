//! The MCP server as the window shows it: on or off, where assistants reach it, and its token.

use std::sync::Arc;

use tauri::State;

use crate::mcp::{McpService, McpStatus};

#[tauri::command]
#[specta::specta]
pub async fn mcp_status(mcp: State<'_, Arc<McpService>>) -> Result<McpStatus, ()> {
    Ok(mcp.status().await)
}

#[tauri::command]
#[specta::specta]
pub async fn set_mcp_enabled(
    mcp: State<'_, Arc<McpService>>,
    enabled: bool,
) -> Result<McpStatus, ()> {
    Ok(mcp.inner().set_enabled(enabled).await)
}

/// Assistants given the old token are let in no more.
#[tauri::command]
#[specta::specta]
pub async fn regenerate_mcp_token(mcp: State<'_, Arc<McpService>>) -> Result<McpStatus, ()> {
    Ok(mcp.inner().regenerate_token().await)
}
