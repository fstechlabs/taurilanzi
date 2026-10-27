/// Release builds get their version when compiled (`scripts/build-app.ts`); the one in
/// `Cargo.toml` is only a placeholder for development builds.
pub const VERSION: &str = match option_env!("TAURILANZI_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

pub fn user_agent() -> String {
    format!("Taurilanzi/{VERSION}")
}
