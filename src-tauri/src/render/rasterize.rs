use std::sync::Arc;

use resvg::tiny_skia::{self, Pixmap};
use resvg::usvg::{self, fontdb::Database};

use super::fonts::DEFAULT_FAMILY;

#[derive(Debug, thiserror::Error)]
pub enum RasterizeError {
    #[error("invalid svg: {0}")]
    Svg(#[from] usvg::Error),
    #[error("invalid canvas size {0}x{1}")]
    Size(u32, u32),
    #[error("png encoding: {0}")]
    Png(String),
}

/// Renders an SVG document to an opaque PNG of the given size.
pub fn to_png(
    svg: &str,
    (width, height): (u32, u32),
    fonts: &Arc<Database>,
) -> Result<Vec<u8>, RasterizeError> {
    let options = usvg::Options {
        font_family: DEFAULT_FAMILY.to_owned(),
        fontdb: Arc::clone(fonts),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_str(svg, &options)?;

    let mut pixmap = Pixmap::new(width, height).ok_or(RasterizeError::Size(width, height))?;
    // The dock shows transparent pixels badly, so everything is painted over black.
    pixmap.fill(tiny_skia::Color::BLACK);
    let scale = tiny_skia::Transform::from_scale(
        width as f32 / tree.size().width(),
        height as f32 / tree.size().height(),
    );
    resvg::render(&tree, scale, &mut pixmap.as_mut());

    pixmap
        .encode_png()
        .map_err(|err| RasterizeError::Png(err.to_string()))
}
