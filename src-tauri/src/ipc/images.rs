//! `deck://` serves images to the UI, so it can use them in a plain `<img>`:
//!
//! - `/key/{page}/{index}`: a saved key as the dock shows it.
//! - `/preview/{index}?appearance={json}&page={id}`: unsaved edits, drawn at the size of that key,
//!   with the values of that page in its text.
//! - `/asset/{id}`: an image from the store.
//! - `/screensaver/{scene}?width={pixels}`: how the dock looks with that screensaver.
//! - `/app-icon/{app id}`: the icon of an installed app, for the apps linked to profiles.
//! - `/plugin/{plugin id}/{path}`: an image of a plugin's package, for its icons.
//! - `/community?url={url}`: an icon or screenshot of the community catalog.
//!
//! Windows reaches the same paths at `http://deck.localhost/`.

use std::borrow::Cow;
use std::sync::{Arc, PoisonError};

use percent_encoding::percent_decode_str;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager, UriSchemeContext, UriSchemeResponder};
use uuid::Uuid;

use super::error::{ErrorKind, IpcError};
use crate::config::{Appearance, AssetStore, Scene};
use crate::engine::EngineHandle;
use crate::plugins::{Plugins, image_file};
use crate::{foreground, screensaver};

const PREVIEW_WIDTH: u32 = 360;

pub const SCHEME: &str = "deck";

pub fn serve(
    context: UriSchemeContext<'_, tauri::Wry>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = context.app_handle().clone();
    let path = request.uri().path().to_owned();
    let query = request.uri().query().map(str::to_owned);
    tauri::async_runtime::spawn(async move {
        let response = respond(&app, &path, query.as_deref())
            .await
            .unwrap_or_else(|err| failure(&err));
        responder.respond(response);
    });
}

async fn respond(
    app: &AppHandle,
    path: &str,
    query: Option<&str>,
) -> Result<Response<Vec<u8>>, IpcError> {
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    match parts.as_slice() {
        ["key", page, index] => {
            let page = page.parse::<Uuid>().map_err(|_| not_found())?;
            let index = index.parse().map_err(|_| not_found())?;
            Ok(png(engine(app)?.key_image(page, index).await?))
        }
        ["preview", index] => {
            let index = index.parse().map_err(|_| not_found())?;
            let appearance = query_value(query, "appearance")
                .and_then(|json| serde_json::from_str::<Appearance>(&json).ok())
                .ok_or_else(|| IpcError::new(ErrorKind::Invalid, "missing or bad appearance"))?;
            let page = query_value(query, "page").and_then(|id| id.parse::<Uuid>().ok());
            Ok(png(engine(app)?.preview(page, index, appearance).await?))
        }
        ["screensaver", scene] => {
            let scene = Scene::from_id(scene).ok_or_else(not_found)?;
            let width = query_value(query, "width")
                .and_then(|w| w.parse().ok())
                .unwrap_or(PREVIEW_WIDTH);
            let image =
                tauri::async_runtime::spawn_blocking(move || screensaver::preview(scene, width))
                    .await
                    .ok()
                    .flatten()
                    .ok_or_else(|| {
                        IpcError::new(ErrorKind::Failed, "could not draw the preview")
                    })?;
            Ok(png(image))
        }
        ["asset", id] => {
            let assets = app
                .try_state::<AssetStore>()
                .ok_or_else(|| IpcError::new(ErrorKind::Unavailable, "starting"))?;
            let image = assets.get(id).map_err(|_| not_found())?;
            Ok(
                response(image.format.mime(), "public, max-age=31536000, immutable")
                    .body(image.bytes.to_vec())
                    .expect("asset response"),
            )
        }
        ["app-icon", id] => {
            let id = percent_decode_str(id).decode_utf8_lossy().into_owned();
            let icon = tauri::async_runtime::spawn_blocking(move || foreground::app_icon(&id))
                .await
                .ok()
                .flatten()
                .ok_or_else(not_found)?;
            Ok(response("image/png", "max-age=3600")
                .body(icon)
                .expect("icon response"))
        }
        ["plugin", id, rest @ ..] if !rest.is_empty() => {
            let id = percent_decode_str(id).decode_utf8_lossy().into_owned();
            let relative = rest
                .iter()
                .map(|part| percent_decode_str(part).decode_utf8_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let plugins = app
                .try_state::<Option<Arc<Plugins>>>()
                .and_then(|state| state.inner().clone())
                .ok_or_else(not_found)?;
            let file = {
                let registry = plugins.registry();
                let registry = registry.read().unwrap_or_else(PoisonError::into_inner);
                registry.plugin(&id).and_then(|p| p.file(&relative))
            };
            let image = file.and_then(|f| image_file(&f)).ok_or_else(not_found)?;
            Ok(response(image.format.mime(), "max-age=3600")
                .body(image.bytes.to_vec())
                .expect("plugin image response"))
        }
        ["community"] => {
            let url = query_value(query, "url")
                .ok_or_else(not_found)?
                .into_owned();
            let plugins = app
                .try_state::<Option<Arc<Plugins>>>()
                .and_then(|state| state.inner().clone())
                .ok_or_else(not_found)?;
            let image = tauri::async_runtime::spawn_blocking(move || plugins.community_image(&url))
                .await
                .ok()
                .flatten()
                .ok_or_else(not_found)?;
            Ok(response(image.format.mime(), "max-age=86400")
                .body(image.bytes.to_vec())
                .expect("community image response"))
        }
        _ => Err(not_found()),
    }
}

fn engine(app: &AppHandle) -> Result<EngineHandle, IpcError> {
    app.try_state::<EngineHandle>()
        .map(|engine| engine.inner().clone())
        .ok_or_else(|| IpcError::new(ErrorKind::Unavailable, "starting"))
}

fn query_value<'a>(query: Option<&'a str>, name: &str) -> Option<Cow<'a, str>> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .and_then(|(_, value)| percent_decode_str(value).decode_utf8().ok())
}

fn png(bytes: Vec<u8>) -> Response<Vec<u8>> {
    response("image/png", "no-store")
        .body(bytes)
        .expect("png response")
}

/// Images are fetched from the app's own page, which is another origin than `deck://`.
fn response(mime: &str, cache: &str) -> tauri::http::response::Builder {
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, cache)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
}

fn not_found() -> IpcError {
    IpcError::new(ErrorKind::NotFound, "not found")
}

fn failure(err: &IpcError) -> Response<Vec<u8>> {
    let status = match err.kind {
        ErrorKind::NotFound => StatusCode::NOT_FOUND,
        ErrorKind::Invalid => StatusCode::BAD_REQUEST,
        ErrorKind::Conflict => StatusCode::CONFLICT,
        ErrorKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        ErrorKind::Failed => StatusCode::INTERNAL_SERVER_ERROR,
    };
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(err.message.clone().into_bytes())
        .expect("error response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_encoded_query_values() {
        let query = Some("x=1&appearance=%7B%22a%22%3A%22%C3%B1%20b%22%7D");
        assert_eq!(query_value(query, "appearance").unwrap(), r#"{"a":"ñ b"}"#);
        assert!(query_value(query, "missing").is_none());
    }
}
