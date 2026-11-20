//! Keys and pages drawn as the dock shows them, for the assistant to see what it made.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use resvg::tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_router};

use super::context::{Answer, engine_error, refused};
use super::ids::parse_id;
use super::reading::{KeyParams, PageParams};
use crate::driver::d200::{KEY_COUNT, WIDE_KEY};
use crate::mcp::handler::Assistant;

const SCREEN: (u32, u32) = (1220, 708);
/// A key and the gap after it.
const PITCH: i32 = 256;
const COLUMNS: u8 = 5;
/// Half the screen is enough to judge a page, at a quarter of the size.
const PAGE_SCALE: f32 = 0.5;

#[tool_router(router = rendering, vis = "pub(in crate::mcp)")]
impl Assistant {
    #[tool(
        description = "Draws one key exactly as the dock shows it, as a PNG (196x196, or 452x196 for the wide key 13).",
        annotations(read_only_hint = true)
    )]
    async fn render_key(&self, Parameters(params): Parameters<KeyParams>) -> Answer {
        let page = match parse_id(&params.page, "page") {
            Ok(page) => page,
            Err(err) => return refused(err),
        };
        match self.tools.engine.key_image(page, params.index).await {
            Ok(png) => image(&png),
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "Draws a whole page as the dock's screen shows it, every key in its place, as a PNG at half size (610x354). Use it to check a page after changing it.",
        annotations(read_only_hint = true)
    )]
    async fn render_page(&self, Parameters(params): Parameters<PageParams>) -> Answer {
        let page = match parse_id(&params.page, "page") {
            Ok(page) => page,
            Err(err) => return refused(err),
        };
        let mut screen = Pixmap::new(SCREEN.0, SCREEN.1).expect("a screen");
        screen.fill(Color::BLACK);
        for index in (0..KEY_COUNT).chain([WIDE_KEY]) {
            let png = match self.tools.engine.key_image(page, index).await {
                Ok(png) => png,
                Err(err) => return engine_error(err),
            };
            let Ok(key) = Pixmap::decode_png(&png) else {
                continue;
            };
            let (x, y) = slot(index);
            screen.draw_pixmap(
                x,
                y,
                key.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
        let (width, height) = (
            (SCREEN.0 as f32 * PAGE_SCALE) as u32,
            (SCREEN.1 as f32 * PAGE_SCALE) as u32,
        );
        let mut small = Pixmap::new(width, height).expect("a smaller screen");
        let paint = PixmapPaint {
            quality: resvg::tiny_skia::FilterQuality::Bicubic,
            ..PixmapPaint::default()
        };
        small.draw_pixmap(
            0,
            0,
            screen.as_ref(),
            &paint,
            Transform::from_scale(PAGE_SCALE, PAGE_SCALE),
            None,
        );
        match small.encode_png() {
            Ok(png) => image(&png),
            Err(err) => refused(format!("could not draw the page: {err}")),
        }
    }
}

/// Where a key sits on the screen: rows of five, the wide key over the last two slots.
fn slot(index: u8) -> (i32, i32) {
    let at = if index == WIDE_KEY { 13 } else { index };
    (
        i32::from(at % COLUMNS) * PITCH,
        i32::from(at / COLUMNS) * PITCH,
    )
}

fn image(png: &[u8]) -> Answer {
    Ok(CallToolResult::success(vec![ContentBlock::image(
        STANDARD.encode(png),
        "image/png",
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_sit_where_the_dock_has_them() {
        assert_eq!(slot(0), (0, 0));
        assert_eq!(slot(4), (1024, 0));
        assert_eq!(slot(5), (0, 256));
        assert_eq!(slot(12), (512, 512));
        assert_eq!(slot(WIDE_KEY), (768, 512));
    }
}
