use jiff::Timestamp;
use log::warn;

use super::key_widgets::KeyNow;
use super::widgets::{self, Feeds};
use crate::config::{Appearance, LayerConfig};
use crate::render::{KeyDesign, Layer};

/// What the text of a key can show about the page it is on.
#[derive(Debug, Clone, Default)]
pub struct PageVars {
    pub name: String,
    /// From 1, in the order of the profile's pages.
    pub number: usize,
    pub count: usize,
}

impl PageVars {
    /// Fills in `{page}`, `{page_number}` and `{page_count}`.
    pub fn fill(&self, text: &str) -> String {
        text.replace("{page_number}", &self.number.to_string())
            .replace("{page_count}", &self.count.to_string())
            .replace("{page}", &self.name)
    }
}

/// Turns a stored appearance into something the renderer can draw, loading images from the
/// asset store and working out what widgets show now. Missing images are skipped so one broken
/// asset never blanks the whole key.
pub fn resolve(appearance: &Appearance, feeds: &Feeds, vars: &PageVars, key: KeyNow) -> KeyDesign {
    let assets = feeds.assets;
    let now = Timestamp::now();
    let layers = appearance
        .layers
        .iter()
        .filter_map(|layer| match layer {
            LayerConfig::Image {
                asset,
                fit,
                inset,
                tint,
                ..
            } => match assets.get(asset) {
                Ok(image) => Some(Layer::Image {
                    image,
                    fit: *fit,
                    inset: *inset,
                    tint: *tint,
                }),
                Err(err) => {
                    warn!("missing key image {asset}: {err}");
                    None
                }
            },
            LayerConfig::Text {
                content,
                font_family,
                font_weight,
                size,
                color,
                position,
                outline,
            } => Some(Layer::Text {
                content: vars.fill(content),
                font_family: font_family.clone(),
                font_weight: *font_weight,
                size: *size,
                color: *color,
                position: *position,
                outline: outline.clone(),
            }),
            LayerConfig::Border {
                color,
                width,
                radius,
            } => Some(Layer::Border {
                color: *color,
                width: *width,
                radius: *radius,
            }),
            LayerConfig::Widget { widget } => Some(Layer::Widget {
                view: widgets::view(widget, feeds, key, now),
            }),
        })
        .collect();
    KeyDesign {
        background: appearance.background.clone(),
        layers,
    }
}

/// Visual feedback while a key is held down.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AssetStore;
    use crate::monitor::Monitor;
    use crate::render::{Background, Color, Fit, ImageFormat};

    #[test]
    fn fills_page_variables() {
        let vars = PageVars {
            name: "OBS".into(),
            number: 2,
            count: 5,
        };
        assert_eq!(vars.fill("{page_number}/{page_count}"), "2/5");
        assert_eq!(vars.fill("En {page}"), "En OBS");
        assert_eq!(
            vars.fill("{pages}"),
            "{pages}",
            "unknown names stay as they are"
        );
    }

    #[test]
    fn skips_missing_images_but_keeps_other_layers() {
        let dir = tempfile::tempdir().unwrap();
        let assets = AssetStore::open(dir.path().to_path_buf()).unwrap();
        let present = assets.put(ImageFormat::Svg, b"<svg/>").unwrap();
        let missing = format!("{}.png", "0".repeat(64));
        let appearance = Appearance {
            background: Background::Solid {
                color: Color::WHITE,
            },
            layers: vec![
                LayerConfig::Image {
                    asset: present,
                    fit: Fit::Contain,
                    inset: 0.0,
                    tint: None,
                    icon: None,
                    source: None,
                    crop: None,
                },
                LayerConfig::Image {
                    asset: missing,
                    fit: Fit::Contain,
                    inset: 0.0,
                    tint: None,
                    icon: None,
                    source: None,
                    crop: None,
                },
                LayerConfig::Border {
                    color: Color::WHITE,
                    width: 2.0,
                    radius: 0.0,
                },
            ],
        };
        let feeds = Feeds {
            monitor: &Monitor::default(),
            hass: &Default::default(),
            assets: &assets,
        };
        let design = resolve(&appearance, &feeds, &PageVars::default(), KeyNow::default());
        assert_eq!(design.layers.len(), 2);
    }
}
