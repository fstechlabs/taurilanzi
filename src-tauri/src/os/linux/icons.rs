//! App icons from the icon themes, as the desktop draws them in its launcher.

use std::fs;
use std::path::{Path, PathBuf};

use resvg::{tiny_skia, usvg};

/// Pixels; the UI shows icons at a third of this, so they stay sharp on dense screens.
const SIZE: u32 = 64;
const SIZES: &[&str] = &[
    "64x64", "72x72", "96x96", "128x128", "48x48", "256x256", "512x512", "32x32", "scalable",
];
/// Where apps install their icons, then the themes desktops ship with.
const THEMES: &[&str] = &["hicolor", "Adwaita", "Yaru", "breeze", "Papirus"];

/// The icon an app's desktop file names, as a PNG.
pub fn icon_png(icon: &str) -> Option<Vec<u8>> {
    let path = find(icon)?;
    let bytes = fs::read(&path).ok()?;
    if path.extension().is_some_and(|ext| ext == "svg") {
        svg_to_png(&bytes)
    } else {
        bytes.starts_with(b"\x89PNG").then_some(bytes)
    }
}

fn find(icon: &str) -> Option<PathBuf> {
    let named = Path::new(icon);
    if named.is_absolute() {
        return named.is_file().then(|| named.to_owned());
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".icons"));
    }
    roots.extend(super::data_dirs().into_iter().map(|dir| dir.join("icons")));
    for theme in THEMES {
        for root in &roots {
            for size in SIZES {
                for ext in ["png", "svg"] {
                    let path = root
                        .join(theme)
                        .join(size)
                        .join("apps")
                        .join(format!("{icon}.{ext}"));
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
    }
    super::data_dirs()
        .into_iter()
        .map(|dir| dir.join("pixmaps"))
        .chain(Some(PathBuf::from("/usr/share/pixmaps")))
        .flat_map(|dir| ["png", "svg"].map(|ext| dir.join(format!("{icon}.{ext}"))))
        .find(|path| path.is_file())
}

fn svg_to_png(svg: &[u8]) -> Option<Vec<u8>> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = SIZE as f32 / size.width().max(size.height());
    let mut pixmap = tiny_skia::Pixmap::new(SIZE, SIZE)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().ok()
}
