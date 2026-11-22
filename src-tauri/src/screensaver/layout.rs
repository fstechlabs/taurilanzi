//! Where each key sits on a dock's panel. The screensaver covers the whole panel, and each key
//! shows its own window of it.

use resvg::tiny_skia::IntRect;

use crate::driver::DeviceModel;

/// Every key of the model that has a screen, with its place on the panel.
pub fn keys(model: &DeviceModel) -> impl Iterator<Item = (u8, IntRect)> + '_ {
    model.controls.iter().filter_map(|control| {
        let b = control.bounds;
        let rect = IntRect::from_xywh(b.x as i32, b.y as i32, b.width, b.height)?;
        Some((control.index, rect))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_fill_the_panel_of_the_d200_with_their_gaps() {
        let model = crate::driver::model("ulanzi.d200").unwrap();
        let rects: std::collections::BTreeMap<u8, IntRect> = keys(model).collect();
        assert_eq!(rects.len(), 14);
        assert_eq!(rects[&4].right() as u32, model.panel.width);
        assert_eq!(rects[&10].bottom() as u32, model.panel.height);
        let wide = rects[&13];
        assert_eq!((wide.x(), wide.y(), wide.width()), (768, 512, 452));
        assert_eq!(wide.right() as u32, model.panel.width);
    }
}
