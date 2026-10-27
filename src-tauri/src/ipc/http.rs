//! Sends a request from the editor, to see what the server answers before pressing the key.

use super::error::{ErrorKind, IpcError};
use crate::actions::{self, HttpResponse};
use crate::config::HttpRequest;

#[tauri::command]
#[specta::specta]
pub async fn try_http(request: HttpRequest) -> Result<HttpResponse, IpcError> {
    tauri::async_runtime::spawn_blocking(move || actions::send_http(&request))
        .await
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))?
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))
}
