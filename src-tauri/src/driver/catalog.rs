//! Every dock model the app drives.

use std::sync::LazyLock;

use super::model::DeviceModel;
use super::ulanzi;

static MODELS: LazyLock<Vec<DeviceModel>> = LazyLock::new(|| vec![ulanzi::models::d200()]);

/// The model every configuration had before the app knew of others.
pub const FIRST_MODEL: &str = "ulanzi.d200";

pub fn models() -> &'static [DeviceModel] {
    &MODELS
}

pub fn model(id: &str) -> Option<&'static DeviceModel> {
    MODELS.iter().find(|m| m.id == id)
}

/// The model a USB device is, by the exact identity of one of its variants.
pub fn matching(vendor_id: u16, product_id: u16, interface: i32) -> Option<&'static DeviceModel> {
    MODELS
        .iter()
        .find(|m| m.matches(vendor_id, product_id, interface))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn ids_and_usb_identities_are_unique() {
        let mut ids = HashSet::new();
        let mut usb = HashSet::new();
        for model in models() {
            assert!(ids.insert(model.id), "{} twice", model.id);
            for variant in &model.variants {
                let at = (
                    variant.usb.vendor_id,
                    variant.usb.product_id,
                    variant.usb.interface,
                );
                assert!(usb.insert(at), "{at:?} matches two models");
            }
        }
        assert!(model(FIRST_MODEL).is_some());
    }

    #[test]
    fn controls_fit_their_panel_and_grid() {
        for model in models() {
            let mut indexes = HashSet::new();
            for control in &model.controls {
                assert!(indexes.insert(control.index), "{}: index twice", model.id);
                let b = control.bounds;
                assert!(b.x + b.width <= model.panel.width, "{}: {b:?}", model.id);
                assert!(b.y + b.height <= model.panel.height, "{}: {b:?}", model.id);
                let c = control.cell;
                assert!(
                    c.column + c.column_span <= model.columns,
                    "{}: {c:?}",
                    model.id
                );
                assert!(c.row + c.row_span <= model.rows, "{}: {c:?}", model.id);
            }
        }
    }
}
