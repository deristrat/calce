use thiserror::Error;

#[derive(Debug, Error)]
pub enum CdcError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("protocol: {0}")]
    Protocol(String),

    #[error("connection lost")]
    ConnectionLost,

    #[error("config: {0}")]
    Config(String),

    #[error("event channel closed")]
    ChannelClosed,
}
