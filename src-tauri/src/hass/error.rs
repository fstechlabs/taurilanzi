#[derive(Debug, thiserror::Error)]
pub enum HassError {
    #[error("{}", tr!("hass.notConnected"))]
    NotConnected,
    #[error("{}", tr!("hass.timeout"))]
    Timeout,
    #[error("{}", tr!("hass.lost"))]
    Lost,
    /// Home Assistant answered, but could not do it; its own explanation.
    #[error("{}", trf!("hass.rejected", message = .message))]
    Rejected { code: String, message: String },
    #[error("{}", trf!("hass.protocol", details = .0))]
    Protocol(String),
    #[error("{}", trf!("hass.unknownEntity", entity = .0))]
    UnknownEntity(String),
    /// The entity has no such thing: a light without effects, a lock asked for a volume…
    #[error("{}", trf!("hass.unsupported", entity = .0))]
    Unsupported(String),
    #[error("{}", trf!("hass.secrets", reason = .0))]
    Secrets(#[from] std::io::Error),
    #[error("{}", tr!("hass.stopped"))]
    Stopped,
}
