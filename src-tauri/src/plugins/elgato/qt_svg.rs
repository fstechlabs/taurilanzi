//! Stream Dock and the Stream Deck draw the SVG images of plugins with Qt, which strokes text
//! `font-size / 100` as wide as the SVG says (measured on Qt 6.4, the one Stream Dock carries):
//! plugins are made to look right that way, so a thin outline in their images stays thin.

use std::sync::Arc;

use roxmltree::{Document, Node, ParsingOptions};

use crate::render::{ImageData, ImageFormat};

/// Qt's font size for text that sets none: Stream Dock's application font.
const DEFAULT_FONT_SIZE: f64 = 12.0;

pub fn as_qt_draws(image: ImageData) -> ImageData {
    if image.format != ImageFormat::Svg {
        return image;
    }
    match std::str::from_utf8(&image.bytes)
        .ok()
        .and_then(text_strokes_as_qt)
    {
        Some(svg) => ImageData {
            format: image.format,
            bytes: Arc::from(svg.into_bytes()),
        },
        None => image,
    }
}

#[derive(Clone, Copy)]
struct Inherited {
    font_size: f64,
    stroked: bool,
    stroke_width: f64,
    /// The font size of the `<text>` the element is in, which sets Qt's scale for all of it.
    text_font_size: Option<f64>,
}

/// The SVG with the strokes of its text as Qt draws them, or `None` when nothing changes.
fn text_strokes_as_qt(svg: &str) -> Option<String> {
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let document = Document::parse_with_options(svg, options).ok()?;
    let mut edits = Vec::new();
    let root = Inherited {
        font_size: DEFAULT_FONT_SIZE,
        stroked: false,
        stroke_width: 1.0,
        text_font_size: None,
    };
    collect(svg, document.root_element(), root, &mut edits);
    if edits.is_empty() {
        return None;
    }
    let mut out = svg.to_owned();
    edits.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    for (at, text) in edits {
        out.insert_str(at, &text);
    }
    Some(out)
}

fn collect(svg: &str, node: Node, parent: Inherited, edits: &mut Vec<(usize, String)>) {
    let mut own = parent;
    if let Some(size) = property(node, "font-size").and_then(|v| font_size(&v, parent.font_size)) {
        own.font_size = size;
    }
    if let Some(stroke) = property(node, "stroke") {
        own.stroked = !matches!(stroke.trim(), "none" | "transparent" | "");
    }
    if let Some(width) = property(node, "stroke-width").and_then(|v| length(&v)) {
        own.stroke_width = width;
    }
    let name = node.tag_name().name();
    if name == "text" {
        own.text_font_size = Some(own.font_size);
    }
    if let Some(text_size) = own.text_font_size
        && own.stroked
        && matches!(name, "text" | "tspan" | "textPath")
    {
        let width = format!("stroke-width:{}", own.stroke_width * text_size / 100.0);
        edits.push(match node.attribute_node("style") {
            Some(style) => (style.range_value().end, format!(";{width}")),
            None => (tag_name_end(svg, node), format!(r#" style="{width}""#)),
        });
    }
    for child in node.children().filter(Node::is_element) {
        collect(svg, child, own, edits);
    }
}

/// A presentation attribute or the same property in the `style` attribute, which wins.
fn property(node: Node, name: &str) -> Option<String> {
    let styled = node.attribute("style").and_then(|style| {
        style
            .split(';')
            .filter_map(|d| d.split_once(':'))
            .rfind(|(key, _)| key.trim() == name)
            .map(|(_, value)| value.trim().to_owned())
    });
    styled.or_else(|| node.attribute(name).map(str::to_owned))
}

fn length(value: &str) -> Option<f64> {
    let value = value.trim();
    let number = value.strip_suffix("px").unwrap_or(value).trim();
    number
        .parse()
        .ok()
        .filter(|n: &f64| n.is_finite() && *n >= 0.0)
}

fn font_size(value: &str, parent: f64) -> Option<f64> {
    let value = value.trim();
    if let Some(em) = value.strip_suffix("em") {
        return em.trim().parse::<f64>().ok().map(|n| n * parent);
    }
    if let Some(percent) = value.strip_suffix('%') {
        return percent
            .trim()
            .parse::<f64>()
            .ok()
            .map(|n| n * parent / 100.0);
    }
    if let Some(points) = value.strip_suffix("pt") {
        return points.trim().parse::<f64>().ok().map(|n| n * 4.0 / 3.0);
    }
    length(value)
}

/// Where the element's name ends in its start tag, written with its prefix if it has one.
fn tag_name_end(svg: &str, node: Node) -> usize {
    let start = node.range().start + 1;
    start
        + svg[start..]
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thins_text_strokes_by_the_font_size() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg"><text stroke-width="1" stroke="#000000" font-size="22">99%</text></svg>"##;
        assert_eq!(
            text_strokes_as_qt(svg).unwrap(),
            r##"<svg xmlns="http://www.w3.org/2000/svg"><text style="stroke-width:0.22" stroke-width="1" stroke="#000000" font-size="22">99%</text></svg>"##
        );
    }

    #[test]
    fn follows_what_text_inherits_and_its_style() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><g stroke="red" style="stroke-width:4" font-size="50"><text>a<tspan font-size="10">b</tspan></text><text style="font-size:2em;fill:white">c</text><rect width="9" height="9"/></g></svg>"#;
        let out = text_strokes_as_qt(svg).unwrap();
        assert!(out.contains(
            r#"<text style="stroke-width:2">a<tspan style="stroke-width:2" font-size="10">"#
        ));
        assert!(out.contains(r#"<text style="font-size:2em;fill:white;stroke-width:4">c"#));
        assert!(out.contains(r#"<rect width="9" height="9"/>"#));
    }

    #[test]
    fn leaves_images_without_stroked_text_alone() {
        for svg in [
            r#"<svg xmlns="http://www.w3.org/2000/svg"><text fill="white">a</text><rect stroke="red"/></svg>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg" stroke="red"><text stroke="none">a</text></svg>"#,
            "not an svg",
        ] {
            assert_eq!(text_strokes_as_qt(svg), None, "{svg}");
        }
    }
}
