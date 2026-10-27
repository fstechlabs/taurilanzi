//! Images plugins send: data URIs or bare base64, or files, in any format the renderer draws.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::render::{ImageData, ImageFormat};

/// Plugins send `data:image/png;base64,…`, or the base64 alone.
pub fn from_base64(data: &str) -> Option<ImageData> {
    let data = data.trim();
    let (mime, payload) = match data.strip_prefix("data:") {
        Some(rest) => {
            let (head, payload) = rest.split_once(',')?;
            (head.split(';').next().map(str::to_owned), payload)
        }
        None => (None, data),
    };
    let bytes = if data.starts_with("data:") && !data.contains(";base64,") {
        percent_encoding::percent_decode_str(payload).collect()
    } else {
        STANDARD
            .decode(payload.trim().replace(['\n', '\r', ' '], ""))
            .ok()?
    };
    let format = mime
        .as_deref()
        .and_then(ImageFormat::from_mime)
        .or_else(|| sniff(&bytes))?;
    Some(ImageData {
        format,
        bytes: Arc::from(bytes),
    })
}

/// Bytes from anywhere, told apart by their content.
pub fn from_bytes(bytes: Vec<u8>) -> Option<ImageData> {
    let format = sniff(&bytes)?;
    Some(ImageData {
        format,
        bytes: Arc::from(bytes),
    })
}

pub fn from_file(path: &Path) -> Option<ImageData> {
    let bytes = fs::read(path).ok()?;
    let by_name = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => Some(ImageFormat::Png),
        Some("jpg" | "jpeg") => Some(ImageFormat::Jpeg),
        Some("gif") => Some(ImageFormat::Gif),
        Some("webp") => Some(ImageFormat::Webp),
        Some("svg") => Some(ImageFormat::Svg),
        _ => None,
    };
    let format = sniff(&bytes).or(by_name)?;
    Some(ImageData {
        format,
        bytes: Arc::from(bytes),
    })
}

fn sniff(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(b"\x89PNG") {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Some(ImageFormat::Jpeg)
    } else if bytes.starts_with(b"GIF8") {
        Some(ImageFormat::Gif)
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(ImageFormat::Webp)
    } else {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
        (head.contains("<svg") || head.trim_start().starts_with("<?xml"))
            .then_some(ImageFormat::Svg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"/>"#;
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest";

    fn read(data: &str) -> Option<(ImageFormat, Vec<u8>)> {
        from_base64(data).map(|image| (image.format, image.bytes.to_vec()))
    }

    #[test]
    fn reads_data_uris() {
        let svg = format!("data:image/svg+xml;base64,{}", STANDARD.encode(SVG));
        assert_eq!(
            read(&svg),
            Some((ImageFormat::Svg, SVG.as_bytes().to_vec()))
        );
        let png = format!("data:image/png;base64,{}", STANDARD.encode(PNG));
        assert_eq!(read(&png), Some((ImageFormat::Png, PNG.to_vec())));
        let encoded = format!(
            "data:image/svg+xml,{}",
            percent_encoding::utf8_percent_encode(SVG, percent_encoding::NON_ALPHANUMERIC)
        );
        assert_eq!(
            read(&encoded),
            Some((ImageFormat::Svg, SVG.as_bytes().to_vec()))
        );
        let unknown_mime = format!(
            "data:application/octet-stream;base64,{}",
            STANDARD.encode(PNG)
        );
        assert_eq!(read(&unknown_mime).map(|(f, _)| f), Some(ImageFormat::Png));
    }

    #[test]
    fn sniffs_bare_base64() {
        assert_eq!(
            read(&STANDARD.encode(PNG)).map(|(f, _)| f),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            read(&STANDARD.encode(SVG)).map(|(f, _)| f),
            Some(ImageFormat::Svg)
        );
        let wrapped = STANDARD.encode(SVG);
        let (head, tail) = wrapped.split_at(10);
        assert_eq!(
            read(&format!("  {head}\n{tail}\r\n")).map(|(f, _)| f),
            Some(ImageFormat::Svg)
        );
        assert_eq!(
            read(&STANDARD.encode(b"\xFF\xD8\xFF")).map(|(f, _)| f),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(
            read(&STANDARD.encode(b"GIF89a")).map(|(f, _)| f),
            Some(ImageFormat::Gif)
        );
        assert_eq!(read(&STANDARD.encode(b"plain text")), None);
        assert_eq!(read("not base64 at all!"), None);
        assert_eq!(read("data:image/png;base64"), None);
    }

    #[test]
    fn reads_files_by_magic_bytes_then_extension() {
        let dir = tempfile::tempdir().unwrap();
        let file = |name: &str, bytes: &[u8]| {
            let path = dir.path().join(name);
            fs::write(&path, bytes).unwrap();
            from_file(&path).map(|image| image.format)
        };
        assert_eq!(file("a.png", PNG), Some(ImageFormat::Png));
        assert_eq!(file("a.svg", SVG.as_bytes()), Some(ImageFormat::Svg));
        assert_eq!(file("noextension", b"GIF89a..."), Some(ImageFormat::Gif));
        assert_eq!(
            file("lies.png", b"\xFF\xD8\xFF\xE0"),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(
            file("b.webp", b"RIFF\0\0\0\0WEBPVP8 "),
            Some(ImageFormat::Webp)
        );
        assert_eq!(file("c.JPG", b"????"), Some(ImageFormat::Jpeg));
        assert_eq!(file("d.txt", b"????"), None);
        assert_eq!(from_file(&dir.path().join("missing.png")), None);
    }
}
