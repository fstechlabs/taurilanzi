//! The screensaver as a still picture over the whole panel: one of the app's own, drawn from the
//! SVG files in `presets/`, or an image of the user's. From it come the key images the dock gets
//! and the previews for the settings.

use std::collections::BTreeMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use resvg::tiny_skia::{Color, FillRule, Mask, Path, PathBuilder, Pixmap, PixmapPaint, Transform};
use resvg::usvg;

use super::layout::{self, PANEL};
use crate::config::Scene;
use crate::render::ImageData;

/// The corners of the keys on the dock, as a fraction of their height.
const KEY_RADIUS: f32 = 0.12;
const GAP_COLOR: (u8, u8, u8) = (10, 10, 12);

pub enum Source {
    Scene(Scene),
    /// Covers the panel, cropped to fit.
    Image(ImageData),
}

fn document(source: &Source) -> String {
    match source {
        Source::Scene(scene) => match scene {
            Scene::Sunset => include_str!("presets/sunset.svg"),
            Scene::Aurora => include_str!("presets/aurora.svg"),
            Scene::Ocean => include_str!("presets/ocean.svg"),
            Scene::Neon => include_str!("presets/neon.svg"),
            Scene::Mountains => include_str!("presets/mountains.svg"),
            Scene::Graphite => include_str!("presets/graphite.svg"),
        }
        .to_owned(),
        Source::Image(image) => format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><image width="{w}" height="{h}" preserveAspectRatio="xMidYMid slice" href="data:{};base64,{}"/></svg>"#,
            image.format.mime(),
            STANDARD.encode(&image.bytes),
            w = PANEL.0,
            h = PANEL.1,
        ),
    }
}

/// The whole panel at `scale`, where 1 is the size of the dock, over black.
pub fn draw(source: &Source, scale: f32) -> Option<Pixmap> {
    let tree = usvg::Tree::from_str(&document(source), &usvg::Options::default()).ok()?;
    let width = (PANEL.0 as f32 * scale).round() as u32;
    let height = (PANEL.1 as f32 * scale).round() as u32;
    let mut pixmap = Pixmap::new(width, height)?;
    pixmap.fill(Color::BLACK);
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(pixmap)
}

/// One PNG per key, cut out of a panel at the size of the dock.
pub fn keys(panel: &Pixmap) -> BTreeMap<u8, Vec<u8>> {
    layout::keys()
        .filter_map(|(index, rect)| {
            let png = panel.clone_rect(rect)?.encode_png().ok()?;
            Some((index, png))
        })
        .collect()
}

/// How the dock shows a scene while the app puts it up: seen through its rounded keys, with the
/// gaps dark. `width` in pixels, the height follows.
pub fn preview(scene: Scene, width: u32) -> Option<Vec<u8>> {
    let scale = width.clamp(64, PANEL.0) as f32 / PANEL.0 as f32;
    let art = draw(&Source::Scene(scene), scale)?;
    let mut mask = Mask::new(art.width(), art.height())?;
    let ts = Transform::from_scale(scale, scale);
    for (_, key) in layout::keys() {
        let area = (
            key.x() as f32,
            key.y() as f32,
            key.width() as f32,
            key.height() as f32,
        );
        if let Some(path) = rounded_rect(area, key.height() as f32 * KEY_RADIUS) {
            mask.fill_path(&path, FillRule::Winding, true, ts);
        }
    }
    let mut out = Pixmap::new(art.width(), art.height())?;
    out.fill(Color::from_rgba8(
        GAP_COLOR.0,
        GAP_COLOR.1,
        GAP_COLOR.2,
        255,
    ));
    out.draw_pixmap(
        0,
        0,
        art.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        Some(&mask),
    );
    out.encode_png().ok()
}

fn rounded_rect((x, y, w, h): (f32, f32, f32, f32), radius: f32) -> Option<Path> {
    let r = radius.min(w / 2.0).min(h / 2.0);
    // Control point distance that makes a cubic curve look like a quarter circle.
    let k = r * 0.552_284_8;
    let mut b = PathBuilder::new();
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    b.close();
    b.finish()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::driver::d200::{KEY_SIZE, WIDE_KEY, WIDE_KEY_SIZE};
    use crate::render::ImageFormat;

    fn size(png: &[u8]) -> (u32, u32) {
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let info = decoder.read_info().unwrap();
        (info.info().width, info.info().height)
    }

    #[test]
    fn every_scene_parses() {
        for scene in Scene::ALL {
            let doc = document(&Source::Scene(scene));
            assert!(
                usvg::Tree::from_str(&doc, &usvg::Options::default()).is_ok(),
                "{scene:?}"
            );
        }
    }

    #[test]
    fn a_picture_gives_every_key() {
        let panel = draw(&Source::Scene(Scene::Mountains), 1.0).unwrap();
        assert_eq!((panel.width(), panel.height()), PANEL);
        let images = keys(&panel);
        assert_eq!(images.len(), 14);
        assert_eq!(size(&images[&0]), KEY_SIZE);
        assert_eq!(size(&images[&WIDE_KEY]), WIDE_KEY_SIZE);
    }

    #[test]
    fn an_image_covers_the_panel() {
        let mut tall = Pixmap::new(40, 200).unwrap();
        tall.fill(Color::from_rgba8(255, 0, 0, 255));
        let image = ImageData {
            format: ImageFormat::Png,
            bytes: Arc::from(tall.encode_png().unwrap()),
        };
        let panel = draw(&Source::Image(image), 0.1).unwrap();
        let corner = panel.pixel(0, 0).unwrap();
        assert_eq!((corner.red(), corner.green(), corner.blue()), (255, 0, 0));
    }

    #[test]
    fn previews_keep_the_panel_shape() {
        let png = preview(Scene::Graphite, 610).unwrap();
        assert_eq!(size(&png), (610, 354));
    }
}
