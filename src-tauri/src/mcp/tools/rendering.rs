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
use crate::driver::DeviceModel;
use crate::mcp::handler::Assistant;

/// Half the screen is enough to judge a page, at a quarter of the size.
const PAGE_SCALE: f32 = 0.5;

#[tool_router(router = rendering, vis = "pub(in crate::mcp)")]
impl Assistant {
    #[tool(
        description = "Draws one key exactly as its dock shows it, as a PNG at the size of its control (see the layout of the dock in get_overview).",
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
        description = "Draws a whole page as its dock's screen shows it, every key in its place, as a PNG at half size. Use it to check a page after changing it.",
        annotations(read_only_hint = true)
    )]
    async fn render_page(&self, Parameters(params): Parameters<PageParams>) -> Answer {
        let page = match parse_id(&params.page, "page") {
            Ok(page) => page,
            Err(err) => return refused(err),
        };
        let config = match self.tools.engine.config().await {
            Ok(config) => config,
            Err(err) => return engine_error(err),
        };
        let Some(model) = config
            .device_of_page(page)
            .and_then(|d| config.device(d))
            .and_then(|d| crate::driver::model(&d.model))
        else {
            return refused(format!(
                "page {page} does not exist, or its dock is unknown"
            ));
        };
        let mut screen = Pixmap::new(model.panel.width, model.panel.height).expect("a screen");
        screen.fill(Color::BLACK);
        for control in &model.controls {
            let png = match self.tools.engine.key_image(page, control.index).await {
                Ok(png) => png,
                Err(err) => return engine_error(err),
            };
            let Ok(key) = Pixmap::decode_png(&png) else {
                continue;
            };
            let (x, y) = slot(model, control.index);
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
            (model.panel.width as f32 * PAGE_SCALE) as u32,
            (model.panel.height as f32 * PAGE_SCALE) as u32,
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

/// Where a key sits on its dock's screen.
fn slot(model: &DeviceModel, index: u8) -> (i32, i32) {
    model
        .control(index)
        .map_or((0, 0), |c| (c.bounds.x as i32, c.bounds.y as i32))
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
        let d200 = crate::driver::model("ulanzi.d200").unwrap();
        assert_eq!(slot(d200, 0), (0, 0));
        assert_eq!(slot(d200, 4), (1024, 0));
        assert_eq!(slot(d200, 5), (0, 256));
        assert_eq!(slot(d200, 12), (512, 512));
        assert_eq!(slot(d200, 13), (768, 512));
    }
}
