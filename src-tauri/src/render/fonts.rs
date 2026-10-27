use std::sync::Arc;

use resvg::usvg::fontdb::Database;

pub const DEFAULT_FAMILY: &str = "Inter";

const BUNDLED: &[&[u8]] = &[
    include_bytes!("../../assets/fonts/inter/Inter-Regular.ttf"),
    include_bytes!("../../assets/fonts/inter/Inter-Medium.ttf"),
    include_bytes!("../../assets/fonts/inter/Inter-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/inter/Inter-Bold.ttf"),
    include_bytes!("../../assets/fonts/inter/Inter-Black.ttf"),
];

/// Bundled fonts always available, plus the fonts installed on the system.
pub fn load(include_system: bool) -> Arc<Database> {
    let mut db = Database::new();
    for font in BUNDLED {
        db.load_font_data(font.to_vec());
    }
    if include_system {
        db.load_system_fonts();
    }
    Arc::new(db)
}

pub fn families(db: &Database) -> Vec<String> {
    let mut names: Vec<String> = db
        .faces()
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .collect();
    names.sort();
    names.dedup();
    names
}
