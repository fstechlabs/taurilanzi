use std::fmt::Write;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::design::{
    Background, Color, Fit, ImageData, ImageFormat, KeyDesign, Layer, VerticalPosition,
};
use super::widgets;

/// The dock bezel hides a few pixels of each edge.
pub const SAFE_MARGIN: f32 = 12.0;
const LINE_HEIGHT: f32 = 1.15;
/// Approximate ascent and cap height as a fraction of the font size, to place baselines.
const ASCENT: f32 = 0.8;
const CAP_HEIGHT: f32 = 0.72;

pub fn build(design: &KeyDesign, (width, height): (u32, u32)) -> String {
    let (w, h) = (width as f32, height as f32);
    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">"#
    );
    write_background(&mut svg, &design.background, w, h);
    for (index, layer) in design.layers.iter().enumerate() {
        match layer {
            Layer::Image {
                image,
                fit,
                inset,
                tint,
            } => {
                let inset = inset.clamp(0.0, 0.5);
                let (dx, dy) = (w * inset, h * inset);
                let aspect = match fit {
                    Fit::Contain => "xMidYMid meet",
                    Fit::Cover => "xMidYMid slice",
                };
                let filter = match tint.filter(|_| !paints_with_current_color(image)) {
                    Some(color) => {
                        let _ = write!(svg, "{}", silhouette(index, color));
                        format!(r#" filter="url(#tint{index})""#)
                    }
                    None => String::new(),
                };
                let _ = write!(
                    svg,
                    r#"<image x="{dx}" y="{dy}" width="{}" height="{}" preserveAspectRatio="{aspect}"{filter} href="data:{};base64,{}"/>"#,
                    w - 2.0 * dx,
                    h - 2.0 * dy,
                    image.format.mime(),
                    STANDARD.encode(tinted(image, *tint)),
                );
            }
            Layer::Text {
                content,
                font_family,
                font_weight,
                size,
                color,
                position,
                outline,
            } => {
                let lines: Vec<&str> = content.split('\n').collect();
                let first_baseline = first_baseline(*position, lines.len(), *size, h);
                let stroke = outline
                    .as_ref()
                    .map(|o| format!(r#" stroke="{}" stroke-width="{}" stroke-linejoin="round" paint-order="stroke""#, o.color.hex(), o.width))
                    .unwrap_or_default();
                let _ = write!(
                    svg,
                    r#"<text x="{}" font-family="{}" font-weight="{font_weight}" font-size="{size}" fill="{}" text-anchor="middle"{stroke}>"#,
                    w / 2.0,
                    escape(font_family),
                    color.hex(),
                );
                for (i, line) in lines.iter().enumerate() {
                    let y = first_baseline + i as f32 * size * LINE_HEIGHT;
                    let _ = write!(
                        svg,
                        r#"<tspan x="{}" y="{y}">{}</tspan>"#,
                        w / 2.0,
                        escape(line)
                    );
                }
                svg.push_str("</text>");
            }
            Layer::Border {
                color,
                width,
                radius,
            } => {
                let half = width / 2.0;
                let _ = write!(
                    svg,
                    r#"<rect x="{half}" y="{half}" width="{}" height="{}" rx="{radius}" fill="none" stroke="{}" stroke-width="{width}"/>"#,
                    w - width,
                    h - width,
                    color.hex(),
                );
            }
            Layer::Widget { view } => widgets::write(&mut svg, view, w, h),
            Layer::Status { ok } => write_status(&mut svg, *ok, w, h),
        }
    }
    svg.push_str("</svg>");
    svg
}

fn write_background(svg: &mut String, background: &Background, w: f32, h: f32) {
    match background {
        Background::Solid { color } => {
            let _ = write!(
                svg,
                r#"<rect width="{w}" height="{h}" fill="{}"/>"#,
                color.hex()
            );
        }
        Background::LinearGradient { from, to, angle } => {
            let (sin, cos) = angle.to_radians().sin_cos();
            let (x1, y1) = (0.5 - cos / 2.0, 0.5 - sin / 2.0);
            let (x2, y2) = (0.5 + cos / 2.0, 0.5 + sin / 2.0);
            let _ = write!(
                svg,
                r#"<defs><linearGradient id="bg" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}"><stop offset="0" stop-color="{}"/><stop offset="1" stop-color="{}"/></linearGradient></defs><rect width="{w}" height="{h}" fill="url(#bg)"/>"#,
                from.hex(),
                to.hex(),
            );
        }
    }
}

/// A round badge in the top right corner, inside the part the bezel leaves visible.
fn write_status(svg: &mut String, ok: bool, w: f32, h: f32) {
    let r = w.min(h) * 0.17;
    let (cx, cy) = (w - SAFE_MARGIN - r - 4.0, SAFE_MARGIN + r + 4.0);
    let fill = if ok { "#16a34a" } else { "#dc2626" };
    let mark = if ok {
        format!(
            "M{} {cy}L{} {}L{} {}",
            cx - 0.42 * r,
            cx - 0.1 * r,
            cy + 0.32 * r,
            cx + 0.44 * r,
            cy - 0.34 * r,
        )
    } else {
        let d = 0.34 * r;
        format!(
            "M{} {}L{} {}M{} {}L{} {}",
            cx - d,
            cy - d,
            cx + d,
            cy + d,
            cx + d,
            cy - d,
            cx - d,
            cy + d,
        )
    };
    let _ = write!(
        svg,
        r##"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{fill}" stroke="#000" stroke-opacity="0.35" stroke-width="{}"/><path d="{mark}" fill="none" stroke="#fff" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"##,
        r * 0.12,
        r * 0.26,
    );
}

fn first_baseline(position: VerticalPosition, lines: usize, size: f32, h: f32) -> f32 {
    let block = (lines.saturating_sub(1)) as f32 * size * LINE_HEIGHT;
    match position {
        VerticalPosition::Top => SAFE_MARGIN + size * ASCENT,
        VerticalPosition::Middle => (h - block + size * CAP_HEIGHT) / 2.0,
        VerticalPosition::Bottom => h - SAFE_MARGIN - block - size * (1.0 - ASCENT),
    }
}

/// Icons of the icon search paint with `currentColor`, which a tint replaces: they keep their
/// lines and see-through parts exactly.
fn paints_with_current_color(image: &ImageData) -> bool {
    image.format == ImageFormat::Svg
        && std::str::from_utf8(&image.bytes).is_ok_and(|text| text.contains("currentColor"))
}

/// Any other image, an uploaded icon or a picture, takes the tint as a single color over its
/// shape: whatever is not transparent.
fn silhouette(index: usize, color: Color) -> String {
    format!(
        r#"<filter id="tint{index}" color-interpolation-filters="sRGB"><feFlood flood-color="{}"/><feComposite in2="SourceAlpha" operator="in"/></filter>"#,
        color.hex()
    )
}

pub(super) fn tinted(image: &ImageData, tint: Option<Color>) -> Vec<u8> {
    match (image.format, tint, std::str::from_utf8(&image.bytes)) {
        (ImageFormat::Svg, Some(color), Ok(text)) => {
            text.replace("currentColor", &color.hex()).into_bytes()
        }
        _ => image.bytes.to_vec(),
    }
}

pub(super) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::design::{Color, KeyDesign};

    fn text_layer(content: &str) -> Layer {
        Layer::Text {
            content: content.into(),
            font_family: "Inter".into(),
            font_weight: 600,
            size: 30.0,
            color: Color::WHITE,
            position: VerticalPosition::Middle,
            outline: None,
        }
    }

    #[test]
    fn escapes_text_and_font_family() {
        let mut design = KeyDesign::solid(Color::BLACK);
        design.layers.push(text_layer(r#"<b> & "x""#));
        let svg = build(&design, (196, 196));
        assert!(svg.contains("&lt;b&gt; &amp; &quot;x&quot;"));
        assert!(!svg.contains("<b>"));
    }

    #[test]
    fn splits_lines_into_tspans() {
        let mut design = KeyDesign::solid(Color::BLACK);
        design.layers.push(text_layer("one\ntwo"));
        let svg = build(&design, (196, 196));
        assert_eq!(svg.matches("<tspan").count(), 2);
    }

    #[test]
    fn tint_replaces_current_color_in_svg_images() {
        let image = ImageData {
            format: ImageFormat::Svg,
            bytes: std::sync::Arc::from(&b"<svg fill=\"currentColor\"/>"[..]),
        };
        let out = String::from_utf8(tinted(&image, Some(Color::rgb(255, 0, 102)))).unwrap();
        assert_eq!(out, "<svg fill=\"#ff0066\"/>");
        assert_eq!(tinted(&image, None), image.bytes.to_vec());
    }

    #[test]
    fn canvas_matches_requested_size() {
        let svg = build(&KeyDesign::solid(Color::BLACK), (452, 196));
        assert!(svg.contains(r#"width="452" height="196" viewBox="0 0 452 196""#));
    }
}
