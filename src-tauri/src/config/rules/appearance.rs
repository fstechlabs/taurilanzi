//! What the editor lets one look of a key hold: its background and its layers.

use super::asset_lookup::Assets;
use super::problem::{Problem, fail, finite, tag, within_range};
use super::widgets::{answers_presses, check_widget};
use crate::config::model::{Appearance, Crop, LayerConfig};
use crate::render::Background;

/// The weights the editor offers, and 800, which the wide key of the first run has.
pub(super) const FONT_WEIGHTS: [u16; 6] = [400, 500, 600, 700, 800, 900];
pub(super) const TEXT_SIZE: (f32, f32) = (10.0, 80.0);
const IMAGE_INSET: (f32, f32) = (0.0, 0.45);
const GRADIENT_ANGLE: (f32, f32) = (0.0, 360.0);
const CROP_ROTATION: (f32, f32) = (-180.0, 180.0);

pub(super) fn check_appearance(look: &Appearance, assets: &impl Assets) -> Result<(), Problem> {
    if let Background::LinearGradient { angle, .. } = &look.background {
        within_range("background.angle", *angle, GRADIENT_ANGLE)?;
    }
    check_order(&look.layers)?;
    for (i, layer) in look.layers.iter().enumerate() {
        let place = match layer {
            LayerConfig::Widget { widget } => {
                format!("layers[{i}].widget ({})", tag(widget, "kind"))
            }
            _ => format!("layers[{i}] ({})", tag(layer, "type")),
        };
        check_layer(layer, assets).map_err(|problem| problem.within(place))?;
    }
    let widgets: Vec<_> = look.widgets().collect();
    if widgets.len() > 1
        && let Some(pressed) = widgets.iter().find(|widget| answers_presses(widget))
    {
        return fail(format!(
            "a {} widget answers presses, so it can only be the one widget of a key; a press \
             of a key with several widgets shows the next one",
            tag(*pressed, "kind")
        ));
    }
    Ok(())
}

/// The editor writes an image, then the widgets, then a text, then whatever else the key had.
fn check_order(layers: &[LayerConfig]) -> Result<(), Problem> {
    let first_widget = layers
        .iter()
        .position(|layer| matches!(layer, LayerConfig::Widget { .. }));
    let Some(first) = first_widget else {
        return Ok(());
    };
    let after_image = matches!(layers.first(), Some(LayerConfig::Image { .. }));
    let contiguous = layers[first..]
        .iter()
        .skip_while(|layer| matches!(layer, LayerConfig::Widget { .. }))
        .all(|layer| !matches!(layer, LayerConfig::Widget { .. }));
    if !contiguous || !(first == 0 || first == 1 && after_image) {
        return fail(
            "widget layers go together, first or right after one image layer, before any text \
             or border layer",
        );
    }
    Ok(())
}

fn check_layer(layer: &LayerConfig, assets: &impl Assets) -> Result<(), Problem> {
    match layer {
        LayerConfig::Image {
            asset,
            inset,
            source,
            crop,
            ..
        } => {
            for (field, id) in [("asset", Some(asset)), ("source", source.as_ref())] {
                if let Some(id) = id
                    && !assets.has_image(id)
                {
                    return fail(format!(
                        "{field} {id:?} is not an image of the asset store; upload it first"
                    ));
                }
            }
            within_range("inset", *inset, IMAGE_INSET)?;
            crop.as_ref().map_or(Ok(()), check_crop)
        }
        LayerConfig::Text {
            font_family,
            font_weight,
            size,
            outline,
            ..
        } => {
            // The renderer falls back on its own family, and the families differ between
            // computers that share keys, so only a blank one is refused.
            if font_family.trim().is_empty() {
                return fail("font_family cannot be empty");
            }
            if !FONT_WEIGHTS.contains(font_weight) {
                return fail(format!(
                    "font_weight must be one of {FONT_WEIGHTS:?}, got {font_weight}"
                ));
            }
            within_range("size", *size, TEXT_SIZE)?;
            match outline {
                Some(outline) if !(outline.width.is_finite() && outline.width >= 0.0) => fail(
                    format!("outline.width must be 0 or more, got {}", outline.width),
                ),
                _ => Ok(()),
            }
        }
        LayerConfig::Border { width, radius, .. } => {
            for (field, value) in [("width", *width), ("radius", *radius)] {
                if !(value.is_finite() && value >= 0.0) {
                    return fail(format!("{field} must be 0 or more, got {value}"));
                }
            }
            Ok(())
        }
        LayerConfig::Widget { widget } => check_widget(widget, assets),
    }
}

fn check_crop(crop: &Crop) -> Result<(), Problem> {
    finite("crop.x", crop.x)?;
    finite("crop.y", crop.y)?;
    if !(crop.zoom.is_finite() && crop.zoom > 0.0) {
        return fail(format!("crop.zoom must be more than 0, got {}", crop.zoom));
    }
    within_range("crop.rotation", crop.rotation, CROP_ROTATION)
}

#[cfg(test)]
mod tests {
    use crate::config::assets::AssetStore;
    use serde_json::{Value, json};

    use super::*;
    use crate::render::ImageFormat;

    fn assets() -> (tempfile::TempDir, AssetStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = AssetStore::open(dir.path().to_path_buf()).unwrap();
        (dir, store)
    }

    fn look(background: Value, layers: Value) -> Appearance {
        serde_json::from_value(json!({ "background": background, "layers": layers })).unwrap()
    }

    fn solid() -> Value {
        json!({ "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } })
    }

    fn text(size: f32, weight: u16) -> Value {
        json!({ "type": "text", "content": "Hola", "font_family": "Inter", "font_weight": weight, "size": size, "color": { "r": 255, "g": 255, "b": 255 }, "position": "bottom", "outline": null })
    }

    fn widget(kind: &str) -> Value {
        json!({ "type": "widget", "widget": { "kind": kind, "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 0, "g": 0, "b": 0 }, "font_family": "Inter" } } })
    }

    #[test]
    fn texts_take_the_sizes_and_weights_of_the_editor() {
        let (_dir, assets) = assets();
        let check = |layer: Value| check_appearance(&look(solid(), json!([layer])), &assets);
        for (size, weight) in [(10.0, 400), (80.0, 900), (30.0, 800)] {
            assert!(check(text(size, weight)).is_ok(), "{size} {weight}");
        }
        assert_eq!(
            check(text(9.0, 700)).unwrap_err().to_string(),
            "layers[0] (text): size must be 10 to 80, got 9"
        );
        assert_eq!(
            check(text(30.0, 300)).unwrap_err().to_string(),
            "layers[0] (text): font_weight must be one of [400, 500, 600, 700, 800, 900], got 300"
        );
        let mut blank = text(30.0, 700);
        blank["font_family"] = json!(" ");
        assert!(check(blank).is_err());
    }

    #[test]
    fn images_are_in_the_store_and_inset_up_to_45_percent() {
        let (_dir, assets) = assets();
        let id = assets.put(ImageFormat::Png, b"png").unwrap();
        let sound = assets
            .put_sound(crate::config::SoundFormat::Mp3, b"mp3")
            .unwrap();
        let image = |asset: &str, inset: f32, crop: Value| json!({ "type": "image", "asset": asset, "fit": "cover", "inset": inset, "tint": null, "icon": null, "source": id, "crop": crop });
        let check = |layer: Value| check_appearance(&look(solid(), json!([layer])), &assets);
        let crop = json!({ "zoom": 1.5, "x": 0.1, "y": -0.2, "rotation": -90, "flip_x": true, "flip_y": false });
        assert!(check(image(&id, 0.45, crop.clone())).is_ok());
        assert!(check(image(&id, 0.5, Value::Null)).is_err());
        assert!(check(image(&sound, 0.0, Value::Null)).is_err(), "a sound");
        let mut turned = crop.clone();
        turned["rotation"] = json!(270);
        assert!(check(image(&id, 0.0, turned)).is_err());
        let mut flat = crop;
        flat["zoom"] = json!(0);
        assert!(check(image(&id, 0.0, flat)).is_err());
    }

    #[test]
    fn gradients_turn_up_to_a_whole_circle() {
        let (_dir, assets) = assets();
        let gradient = |angle: f32| {
            look(
                json!({ "type": "linear_gradient", "from": { "r": 0, "g": 0, "b": 0 }, "to": { "r": 1, "g": 1, "b": 1 }, "angle": angle }),
                json!([]),
            )
        };
        assert!(check_appearance(&gradient(360.0), &assets).is_ok());
        assert_eq!(
            check_appearance(&gradient(-5.0), &assets)
                .unwrap_err()
                .to_string(),
            "background.angle must be 0 to 360, got -5"
        );
    }

    #[test]
    fn widgets_go_together_after_the_image_and_before_the_text() {
        let (_dir, assets) = assets();
        let id = assets.put(ImageFormat::Png, b"png").unwrap();
        let image = json!({ "type": "image", "asset": id, "fit": "cover", "inset": 0, "tint": null, "icon": null, "source": null, "crop": null });
        let border = json!({ "type": "border", "color": { "r": 0, "g": 0, "b": 0 }, "width": 2, "radius": 8 });
        let check = |layers: Value| check_appearance(&look(solid(), layers), &assets);
        assert!(
            check(json!([
                image,
                widget("moon"),
                widget("calendar"),
                text(30.0, 700),
                border,
                text(20.0, 400)
            ]))
            .is_ok()
        );
        assert!(check(json!([widget("moon"), text(30.0, 700)])).is_ok());
        assert!(check(json!([text(30.0, 700), widget("moon")])).is_err());
        assert!(check(json!([widget("moon"), border, widget("calendar")])).is_err());
        assert!(check(json!([image, image, widget("moon")])).is_err());
    }
}
