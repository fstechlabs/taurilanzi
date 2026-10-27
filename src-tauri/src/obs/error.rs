#[derive(Debug, thiserror::Error)]
pub enum ObsError {
    #[error("{}", tr!("obs.notConnected"))]
    NotConnected,
    #[error("{}", tr!("obs.timeout"))]
    Timeout,
    #[error("{}", tr!("obs.lost"))]
    Lost,
    /// OBS answered, but could not do it; OBS's own explanation, if it gave one.
    #[error(
        "{}",
        trf!("obs.rejected", message = .message)
    )]
    Rejected { code: u16, message: String },
    #[error(
        "{}",
        trf!("obs.protocol", details = .0)
    )]
    Protocol(String),
    #[error(
        "{}",
        trf!("obs.secrets", reason = .0)
    )]
    Secrets(#[from] std::io::Error),
    #[error(
        "{}",
        tr!("obs.stopped")
    )]
    Stopped,
}
