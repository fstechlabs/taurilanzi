//! Images, icons and sounds for keys: stored in the app's asset store, whose ids keys point to.

use std::time::Duration;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use ureq::Agent;

use super::context::{Answer, json, refused};
use crate::config::SoundFormat;
use crate::mcp::handler::Assistant;
use crate::render::ImageFormat;

const ICONIFY: &str = "https://api.iconify.design";
const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_SOUND_BYTES: u64 = 30 * 1024 * 1024;
const MAX_ICONS: usize = 64;
/// How the editor frames an icon: a margin around it, the whole of it shown.
const ICON_INSET: f32 = 0.22;

#[derive(Deserialize, JsonSchema)]
pub struct ImportParams {
    /// A file on this computer, by its full path.
    pub path: Option<String>,
    /// Or an http(s) address to download it from.
    pub url: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct IconParams {
    /// An Iconify icon, `prefix:name`, such as `mdi:microphone` (see search_icons).
    pub icon: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchParams {
    /// What the icon shows, in English: `microphone`, `play`, `lightbulb`…
    pub search: String,
    /// Only icons of this collection, such as `mdi`, `lucide`, `ph` or `logos`.
    pub collection: Option<String>,
    /// At most this many; 32 by default.
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
struct IconSet {
    #[serde(default)]
    icons: std::collections::BTreeMap<String, IconBody>,
    #[serde(default)]
    aliases: std::collections::BTreeMap<String, Alias>,
    width: Option<f32>,
    height: Option<f32>,
}

#[derive(Deserialize)]
struct IconBody {
    body: String,
    width: Option<f32>,
    height: Option<f32>,
    left: Option<f32>,
    top: Option<f32>,
}

#[derive(Deserialize)]
struct Alias {
    parent: String,
}

#[tool_router(router = assets, vis = "pub(in crate::mcp)")]
impl Assistant {
    #[tool(
        description = "Finds icons of Iconify (more than 200 000, free) by what they show. Answers their ids, for import_icon.",
        annotations(read_only_hint = true)
    )]
    async fn search_icons(&self, Parameters(params): Parameters<SearchParams>) -> Answer {
        let limit = params.limit.unwrap_or(32).clamp(1, MAX_ICONS);
        let mut query = vec![
            ("query", params.search),
            ("limit", MAX_ICONS.max(32).to_string()),
        ];
        if let Some(prefix) = params.collection {
            query.push(("prefix", prefix));
        }
        let found = tokio::task::spawn_blocking(move || -> Result<Value, String> {
            let mut request = agent().get(format!("{ICONIFY}/search"));
            for (name, value) in &query {
                request = request.query(*name, value);
            }
            let body = request
                .call()
                .map_err(|err| err.to_string())?
                .body_mut()
                .read_to_string()
                .map_err(|err| err.to_string())?;
            serde_json::from_str(&body).map_err(|err| err.to_string())
        })
        .await;
        match found {
            Ok(Ok(found)) => {
                let icons: Vec<Value> = found["icons"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(limit)
                    .cloned()
                    .collect();
                json(json!({ "icons": icons, "collections": found["collections"] }))
            }
            Ok(Err(err)) => refused(format!("Iconify did not answer: {err}")),
            Err(err) => refused(err.to_string()),
        }
    }

    #[tool(
        description = "Stores an Iconify icon for keys. Answers its asset id and the image layer the editor would make of it (sized and tinted like the editor's icons); put that layer first in the key's `layers`. Change `tint` to colour it; multicolour icons answer `tint: null`."
    )]
    async fn import_icon(&self, Parameters(params): Parameters<IconParams>) -> Answer {
        let Some((prefix, name)) = params
            .icon
            .trim()
            .split_once(':')
            .filter(|(p, n)| slug(p) && slug(n))
        else {
            return refused("icons are written prefix:name, such as mdi:microphone");
        };
        let (prefix, name) = (prefix.to_owned(), name.to_owned());
        let fetched = tokio::task::spawn_blocking({
            let (prefix, name) = (prefix.clone(), name.clone());
            move || -> Result<IconSet, String> {
                let body = agent()
                    .get(format!("{ICONIFY}/{prefix}.json"))
                    .query("icons", &name)
                    .call()
                    .map_err(|err| err.to_string())?
                    .body_mut()
                    .read_to_string()
                    .map_err(|err| err.to_string())?;
                serde_json::from_str(&body).map_err(|err| err.to_string())
            }
        })
        .await;
        let set = match fetched {
            Ok(Ok(set)) => set,
            Ok(Err(err)) => return refused(format!("Iconify did not answer: {err}")),
            Err(err) => return refused(err.to_string()),
        };
        let Some(svg) = svg_of(&set, &name) else {
            return refused(format!("Iconify has no icon {prefix}:{name}"));
        };
        let tintable = svg.contains("currentColor");
        let asset = match self.tools.assets.put(ImageFormat::Svg, svg.as_bytes()) {
            Ok(asset) => asset,
            Err(err) => return refused(format!("could not store the icon: {err}")),
        };
        let tint = tintable.then(|| json!({ "r": 255, "g": 255, "b": 255 }));
        json(json!({
            "asset": asset,
            "layer": {
                "type": "image",
                "asset": asset,
                "source": asset,
                "fit": "contain",
                "inset": ICON_INSET,
                "tint": tint,
                "icon": format!("{prefix}:{name}"),
                "crop": null,
            },
        }))
    }

    #[tool(
        description = "Stores an image for keys (PNG, JPEG, WebP, GIF or SVG, up to 10 MB) from a file or an address. Answers its asset id, for an image layer `{\"type\": \"image\", \"asset\": id, \"fit\": \"cover\", \"inset\": 0}`."
    )]
    async fn import_image(&self, Parameters(params): Parameters<ImportParams>) -> Answer {
        let bytes = match read(params, MAX_IMAGE_BYTES).await {
            Ok(bytes) => bytes,
            Err(err) => return refused(err),
        };
        let Some(format) = image_format(&bytes) else {
            return refused("that is not a PNG, JPEG, WebP, GIF or SVG image");
        };
        match self.tools.assets.put(format, &bytes) {
            Ok(asset) => json(json!({ "asset": asset })),
            Err(err) => refused(format!("could not store the image: {err}")),
        }
    }

    #[tool(
        description = "Stores a sound for play_sound actions (MP3, WAV, M4A, FLAC or OGG, up to 30 MB) from a file or an address. Answers its asset id, for `sound`, and the name to show, for `name`."
    )]
    async fn import_sound(&self, Parameters(params): Parameters<ImportParams>) -> Answer {
        let name = params
            .path
            .as_deref()
            .or(params.url.as_deref())
            .and_then(|from| from.rsplit(['/', '\\']).next())
            .map(|name| name.split(['?', '#']).next().unwrap_or(name).to_owned())
            .unwrap_or_default();
        let Some(format) = sound_format(&name) else {
            return refused("the file must end in .mp3, .wav, .m4a, .flac or .ogg");
        };
        let bytes = match read(params, MAX_SOUND_BYTES).await {
            Ok(bytes) => bytes,
            Err(err) => return refused(err),
        };
        match self.tools.assets.put_sound(format, &bytes) {
            Ok(asset) => json(json!({ "sound": asset, "name": name })),
            Err(err) => refused(format!("could not store the sound: {err}")),
        }
    }
}

async fn read(params: ImportParams, limit: u64) -> Result<Vec<u8>, String> {
    match (params.path, params.url) {
        (Some(path), None) => {
            let size = std::fs::metadata(&path)
                .map_err(|err| format!("{path}: {err}"))?
                .len();
            if size > limit {
                return Err(format!(
                    "the file is larger than {} MB",
                    limit / 1024 / 1024
                ));
            }
            std::fs::read(&path).map_err(|err| format!("{path}: {err}"))
        }
        (None, Some(url)) => {
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err("only http and https addresses".into());
            }
            tokio::task::spawn_blocking(move || {
                agent()
                    .get(&url)
                    .call()
                    .map_err(|err| err.to_string())?
                    .body_mut()
                    .with_config()
                    .limit(limit)
                    .read_to_vec()
                    .map_err(|err| err.to_string())
            })
            .await
            .map_err(|err| err.to_string())?
        }
        _ => Err("give either path or url".into()),
    }
}

/// By content, not by name: what an address answers may be named anything.
fn image_format(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(b"\x89PNG") {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(ImageFormat::Jpeg)
    } else if bytes.starts_with(b"GIF8") {
        Some(ImageFormat::Gif)
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(ImageFormat::Webp)
    } else if std::str::from_utf8(&bytes[..bytes.len().min(4096)])
        .is_ok_and(|head| head.contains("<svg"))
    {
        Some(ImageFormat::Svg)
    } else {
        None
    }
}

fn sound_format(name: &str) -> Option<SoundFormat> {
    let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
    let mime = match extension.as_str() {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        _ => return None,
    };
    SoundFormat::from_mime(mime)
}

/// The SVG the editor makes of an Iconify icon.
fn svg_of(set: &IconSet, name: &str) -> Option<String> {
    let icon = set
        .icons
        .get(name)
        .or_else(|| set.icons.get(&set.aliases.get(name)?.parent))?;
    let width = icon.width.or(set.width).unwrap_or(16.0);
    let height = icon.height.or(set.height).unwrap_or(16.0);
    Some(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="{} {} {width} {height}">{}</svg>"#,
        icon.left.unwrap_or(0.0),
        icon.top.unwrap_or(0.0),
        icon.body
    ))
}

fn slug(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text.split('-').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn agent() -> Agent {
    Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .user_agent(crate::version::user_agent())
        .build()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_are_drawn_as_the_editor_draws_them() {
        let set: IconSet = serde_json::from_value(json!({
            "icons": { "mic": { "body": "<path fill=\"currentColor\"/>" } },
            "aliases": { "microphone": { "parent": "mic" } },
            "width": 24, "height": 24
        }))
        .unwrap();
        assert_eq!(
            svg_of(&set, "microphone").unwrap(),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24"><path fill="currentColor"/></svg>"#
        );
        assert!(slug("mdi") && slug("arrow-left") && !slug("Arrow") && !slug("a--b"));
    }

    #[test]
    fn images_are_told_by_their_content() {
        assert!(matches!(
            image_format(b"\x89PNG\r\n"),
            Some(ImageFormat::Png)
        ));
        assert!(matches!(
            image_format(b"<?xml?><svg/>"),
            Some(ImageFormat::Svg)
        ));
        assert!(image_format(b"hello").is_none());
        assert!(matches!(sound_format("a/b.MP3"), Some(SoundFormat::Mp3)));
    }
}
