#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error(
        "{}",
        tr!("actions.unsupportedUrl")
    )]
    UnsupportedUrl,
    #[error("{}", trf!("actions.notFound", path = .0))]
    NotFound(String),
    #[error("{}", trf!("actions.notAnApp", path = .0))]
    NotAnApp(String),
    #[error("{}", trf!("actions.unknownKey", key = .0))]
    UnknownKey(String),
    #[cfg(not(target_os = "windows"))]
    #[cfg_attr(
        target_os = "linux",
        error("{}", tr!("actions.noKeyboardPermission"))
    )]
    #[cfg_attr(
        not(target_os = "linux"),
        error("{}", tr!("actions.noAccessibility"))
    )]
    NoAccessibility,
    #[error("{}", trf!("actions.launch", reason = .0))]
    Launch(#[from] std::io::Error),
    #[error(
        "{}",
        trf!("actions.sound", reason = .0)
    )]
    Sound(String),
    #[error("{}", trf!("actions.http", reason = .0))]
    Http(String),
    #[error("{}", trf!("actions.httpStatus", status = .0))]
    HttpStatus(u16),
    /// The server of the action was removed from the integrations.
    #[error("{}", tr!("actions.noServer"))]
    NoServer,
    #[error("OBS: {0}")]
    Obs(#[from] crate::obs::ObsError),
    #[error("Home Assistant: {0}")]
    Hass(#[from] crate::hass::HassError),
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[error("{}", trf!("actions.clipboard", reason = .0))]
    Clipboard(String),
    #[cfg(target_os = "windows")]
    #[error("{}", tr!("actions.keysRefused"))]
    KeysRefused,
    #[cfg(target_os = "windows")]
    #[error("{}", tr!("actions.noBrightness"))]
    NoBrightness,
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    #[error(
        "{}",
        tr!("actions.unsupported")
    )]
    Unsupported,
}
