//! Where each key sits on the D200 panel. The screensaver covers the whole panel, and each key
//! shows its own window of it.

use resvg::tiny_skia::IntRect;

use crate::driver::d200::{COLUMNS, KEY_COUNT, KEY_SIZE, WIDE_KEY, WIDE_KEY_SIZE};

pub const PANEL: (u32, u32) = (1220, 708);
const PITCH: u32 = 256;

/// Every key with its place on the panel, the wide key last.
pub fn keys() -> impl Iterator<Item = (u8, IntRect)> {
    (0..KEY_COUNT)
        .chain([WIDE_KEY])
        .map(|index| (index, rect(index)))
}

pub fn rect(index: u8) -> IntRect {
    let (col, row) = (u32::from(index % COLUMNS), u32::from(index / COLUMNS));
    let (width, height) = if index == WIDE_KEY {
        WIDE_KEY_SIZE
    } else {
        KEY_SIZE
    };
    IntRect::from_xywh((col * PITCH) as i32, (row * PITCH) as i32, width, height)
        .expect("keys have a size")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_fill_the_panel_with_their_gaps() {
        let rects: Vec<_> = keys().map(|(_, rect)| rect).collect();
        assert_eq!(rects.len(), 14);
        assert_eq!(rect(4).right() as u32, PANEL.0);
        assert_eq!(rect(10).bottom() as u32, PANEL.1);
        let wide = rect(WIDE_KEY);
        assert_eq!((wide.x(), wide.y(), wide.width()), (768, 512, 452));
        assert_eq!(wide.right() as u32, PANEL.0);
    }
}
