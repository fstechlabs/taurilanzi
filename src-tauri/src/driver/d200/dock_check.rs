//! Manual check on the real dock of what the preview cannot show: color emoji, images with
//! transparency and the wide key without the firmware's clock, as the D200 displays them.
//!
//! Talks to the dock directly, without the app or its configuration. Connect the dock, close
//! Ulanzi Studio and the app (only one process can use it), and from `src-tauri`:
//!
//! ```text
//! cargo test --lib dock_check -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::io::{self, BufRead};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{D200, Event, KEY_SIZE, WIDE_KEY, WIDE_KEY_SIZE};
use crate::render::{
    Background, Color, Fit, ImageData, ImageFormat, KeyDesign, Layer, Renderer, VerticalPosition,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[test]
#[ignore = "needs the real dock; see the module docs"]
fn dock_check() {
    let (dock, events) = D200::start();
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    let device = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(Event::Connected(device)) => break device,
            Ok(_) => {}
            Err(_) => panic!(
                "no dock after {CONNECT_TIMEOUT:?}: connect it and close Ulanzi Studio and the app"
            ),
        }
    };
    println!("{} connected, firmware {}", device.model, device.firmware);
    dock.replace_page(test_keys());

    println!("Key 1: two color emoji over a gradient, with Ñandú below.");
    println!("Key 2: a white ring fading to transparent from left to right over the gradient.");
    println!("       It should blend into the gradient, with no black or white halo.");
    println!("Wide key: the gradient with its text and no clock or stats from the firmware.");
    println!("Look at the dock, then press Enter to blank it…");
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).unwrap_or(0) == 0 {
        // No terminal to wait on: leave the keys up for a while instead.
        std::thread::sleep(Duration::from_secs(30));
    }
    dock.replace_page(BTreeMap::new());
    // Gives the worker time to send the blank page before the dock is dropped.
    std::thread::sleep(Duration::from_secs(1));
    println!("Dock blanked. Start Ulanzi Studio or the app to get your keys back.");
}

#[test]
fn test_keys_render() {
    let keys = test_keys();
    assert_eq!(keys.keys().copied().collect::<Vec<_>>(), [0, 1, WIDE_KEY]);
    assert!(keys.values().all(|png| png.starts_with(b"\x89PNG")));
}

fn test_keys() -> BTreeMap<u8, Vec<u8>> {
    let ring = Layer::Image {
        image: ImageData {
            format: ImageFormat::Png,
            bytes: Arc::from(fading_ring(160)),
        },
        fit: Fit::Contain,
        inset: 0.1,
        tint: None,
    };
    let designs = [
        (0, vec![text("🎉👍\nÑandú", 30.0)]),
        (1, vec![ring, text("Alpha", 26.0)]),
        (WIDE_KEY, vec![text("Tecla ancha, sin reloj", 30.0)]),
    ];
    let mut renderer = Renderer::new();
    designs
        .into_iter()
        .map(|(index, layers)| {
            let size = if index == WIDE_KEY {
                WIDE_KEY_SIZE
            } else {
                KEY_SIZE
            };
            let design = KeyDesign {
                background: Background::LinearGradient {
                    from: Color::rgb(255, 0, 102),
                    to: Color::rgb(51, 0, 204),
                    angle: 90.0,
                },
                layers,
            };
            let png = renderer.render(&design, size).expect("draw the test key");
            (index, png.as_ref().clone())
        })
        .collect()
}

fn text(content: &str, size: f32) -> Layer {
    Layer::Text {
        content: content.into(),
        font_family: "Inter".into(),
        font_weight: 700,
        size,
        color: Color::WHITE,
        position: VerticalPosition::Bottom,
        outline: None,
    }
}

/// A white ring whose alpha fades from left to right, on a fully transparent background.
fn fading_ring(size: u32) -> Vec<u8> {
    let center = size as f32 / 2.0;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let distance = ((x as f32 - center).powi(2) + (y as f32 - center).powi(2)).sqrt();
            let ring = 255.0 - ((distance - size as f32 * 0.32).abs() * 12.0);
            let alpha = if distance < center {
                ring.max(0.0)
            } else {
                0.0
            };
            let fade = alpha * x as f32 / size as f32;
            pixels.extend_from_slice(&[255, 255, 255, fade as u8]);
        }
    }
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&pixels).expect("png data");
    writer.finish().expect("png end");
    png
}
