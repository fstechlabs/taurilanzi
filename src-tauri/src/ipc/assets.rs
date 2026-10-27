use tauri::State;
use tauri::ipc::{InvokeBody, Request};

use super::error::{ErrorKind, IpcError};
use crate::config::{AssetStore, SoundFormat};
use crate::render::ImageFormat;

const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_SOUND_BYTES: usize = 30 * 1024 * 1024;
const TYPE_HEADER: &str = "asset-type";

/// Stores an image or a sound and returns its id. Takes the raw bytes, so files never travel as
/// JSON; that is also why it is left out of the generated bindings. The file type goes in
/// `Asset-Type`: Tauri's IPC uses `Content-Type` for itself.
#[tauri::command]
pub async fn put_asset(
    request: Request<'_>,
    assets: State<'_, AssetStore>,
) -> Result<String, IpcError> {
    let invalid = |message: &str| IpcError::new(ErrorKind::Invalid, message);
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(invalid(tr!("assets.rawBytes")));
    };
    let mime = request
        .headers()
        .get(TYPE_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let stored = if let Some(format) = ImageFormat::from_mime(mime) {
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(invalid(tr!("assets.imageTooBig")));
        }
        assets.put(format, bytes)
    } else if let Some(format) = SoundFormat::from_mime(mime) {
        if bytes.len() > MAX_SOUND_BYTES {
            return Err(invalid(tr!("assets.soundTooBig")));
        }
        assets.put_sound(format, bytes)
    } else {
        return Err(invalid(tr!("assets.unsupportedType")));
    };
    stored.map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))
}
