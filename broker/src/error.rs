#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("lockfile not found")]
    LockfileNotFound,
}

pub type Result<T, E = Error> = core::result::Result<T, E>;