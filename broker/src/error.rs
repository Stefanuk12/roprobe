#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error(transparent)]
    Squash(#[from] squash::Error),

    #[error("lockfile not found")]
    LockfileNotFound,

    #[error(
        "a broker is already running on port {port} (pid {pid}); stop it first to bind elsewhere"
    )]
    BrokerAlreadyRunning { port: u16, pid: u32 },
}

pub type Result<T, E = Error> = core::result::Result<T, E>;
