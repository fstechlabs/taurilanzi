use std::collections::BTreeMap;
use std::io;

use uuid::Uuid;

use super::actions::Action;
use super::assets::AssetStore;
use super::devices::{FIRST_DOCK, first_device};
use super::model::{
    Appearance, Config, DeviceSettings, Key, KeyKind, LayerConfig, Page, Position, Profile,
    SCHEMA_VERSION,
};
use super::preferences::{NewKeyStyle, Preferences};
use crate::driver::{DeviceModel, FIRST_MODEL};
use crate::render::{Background, Color, DEFAULT_FAMILY, Fit, ImageFormat, VerticalPosition};

/// A key at least this much wider than tall gets the banner of a starter profile.
const BANNER_RATIO: f32 = 1.6;

const GLOBE_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c3 3.2 3 14.8 0 18M12 3c-3 3.2-3 14.8 0 18"/></svg>"#;

const BACK_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 6l-6 6 6 6M4 12h16"/></svg>"#;

const SAMPLE_LINKS: [(&str, &str, Color); 4] = [
    ("GitHub", "https://github.com", Color::rgb(36, 41, 47)),
    ("Rust", "https://doc.rust-lang.org", Color::rgb(183, 65, 14)),
    ("Tauri", "https://v2.tauri.app", Color::rgb(36, 200, 219)),
    ("Nuxt", "https://nuxt.com", Color::rgb(0, 140, 90)),
];

/// Config created the first time the app runs: the first dock, with a few keys that open
/// websites.
pub fn first_run(assets: &AssetStore) -> io::Result<Config> {
    let model = crate::driver::model(FIRST_MODEL).expect("the first model is in the catalog");
    let profile = starter_profile(model, FIRST_DOCK, (Uuid::new_v4(), Uuid::new_v4()), assets)?;
    Ok(Config {
        schema: SCHEMA_VERSION,
        revision: 0,
        devices: vec![first_device(DeviceSettings {
            brightness: 70,
            sleep_minutes: 0,
        })],
        preferences: Preferences::default(),
        active_profiles: BTreeMap::from([(FIRST_DOCK, profile.id)]),
        profiles: vec![profile],
        plugin_settings: Default::default(),
        integrations: Vec::new(),
        local_profiles: Default::default(),
        plugins: Default::default(),
        declined_plugins: Default::default(),
    })
}

/// What a dock starts with: keys that open websites on its first controls and, on a wide key if
/// it has one, the app's name. `ids` are those of the profile and of its page.
pub fn starter_profile(
    model: &DeviceModel,
    device: Uuid,
    (id, page_id): (Uuid, Uuid),
    assets: &AssetStore,
) -> io::Result<Profile> {
    let globe = assets.put(ImageFormat::Svg, GLOBE_ICON.as_bytes())?;
    let banner = model.controls.iter().find(|control| {
        model
            .image_size(control.index)
            .is_some_and(|size| size.width as f32 >= BANNER_RATIO * size.height as f32)
    });
    let mut keys = BTreeMap::new();
    let squares = model
        .controls
        .iter()
        .filter(|control| Some(control.index) != banner.map(|b| b.index));
    for (control, (label, url, color)) in squares.zip(SAMPLE_LINKS) {
        keys.insert(
            control.index,
            Key {
                kind: Some(KeyKind::Action),
                appearance: Appearance {
                    background: Background::Solid { color },
                    layers: vec![
                        LayerConfig::Image {
                            asset: globe.clone(),
                            fit: Fit::Contain,
                            inset: 0.26,
                            tint: None,
                            icon: None,
                            source: None,
                            crop: None,
                        },
                        text(label, 28.0, 700, VerticalPosition::Bottom),
                    ],
                },
                actions: vec![Action::OpenUrl { url: url.into() }],
                double_press: Vec::new(),
                long_press: Vec::new(),
                second_state: None,
            },
        );
    }
    if let Some(banner) = banner {
        keys.insert(
            banner.index,
            Key {
                kind: Some(KeyKind::Basic),
                appearance: Appearance {
                    background: Background::LinearGradient {
                        from: Color::rgb(20, 24, 40),
                        to: Color::rgb(60, 20, 80),
                        angle: 0.0,
                    },
                    layers: vec![text("Taurilanzi", 34.0, 800, VerticalPosition::Middle)],
                },
                actions: Vec::new(),
                double_press: Vec::new(),
                long_press: Vec::new(),
                second_state: None,
            },
        );
    }
    let page = Page {
        id: page_id,
        name: tr!("defaults.homePage").into(),
        keys,
        position: Position::default(),
    };
    Ok(Profile {
        id,
        device,
        name: tr!("defaults.profile").into(),
        home_page: page.id,
        pages: vec![page],
        apps: Vec::new(),
    })
}

/// The key every new page starts with, so there is always a way out. It takes the colors and
/// font of new keys.
pub fn back_key(assets: &AssetStore, style: &NewKeyStyle) -> io::Result<Key> {
    let arrow = assets.put(ImageFormat::Svg, BACK_ICON.as_bytes())?;
    Ok(Key {
        kind: Some(KeyKind::Action),
        appearance: Appearance {
            background: Background::Solid {
                color: style.background,
            },
            layers: vec![
                LayerConfig::Image {
                    asset: arrow,
                    fit: Fit::Contain,
                    inset: 0.28,
                    tint: Some(style.text_color),
                    icon: None,
                    source: None,
                    crop: None,
                },
                LayerConfig::Text {
                    content: tr!("defaults.back").into(),
                    font_family: style.font_family.clone(),
                    font_weight: style.font_weight,
                    size: 26.0,
                    color: style.text_color,
                    position: VerticalPosition::Bottom,
                    outline: None,
                },
            ],
        },
        actions: vec![Action::Back],
        double_press: Vec::new(),
        long_press: Vec::new(),
        second_state: None,
    })
}

fn text(content: &str, size: f32, weight: u16, position: VerticalPosition) -> LayerConfig {
    LayerConfig::Text {
        content: content.into(),
        font_family: DEFAULT_FAMILY.into(),
        font_weight: weight,
        size,
        color: Color::WHITE,
        position,
        outline: None,
    }
}
