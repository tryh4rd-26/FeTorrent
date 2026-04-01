use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    // Bencode
    #[error("bencode: premature end of data")]
    BencodePrematureEnd,
    #[error("bencode: invalid data — {0}")]
    BencodeInvalid(String),

    // Torrent file parsing
    #[error("torrent: missing field '{0}'")]
    TorrentMissingField(&'static str),
    #[error("torrent: invalid field '{0}' — {1}")]
    TorrentInvalidField(&'static str, String),

    // Magnet
    #[error("magnet: invalid URI — {0}")]
    MagnetInvalid(String),

    // Tracker
    #[error("tracker: HTTP error — {0}")]
    TrackerHttp(String),
    #[error("tracker: UDP error — {0}")]
    TrackerUdp(String),
    #[error("tracker: failure response — {0}")]
    TrackerFailure(String),
    #[error("tracker: timeout")]
    TrackerTimeout,

    // Peer
    #[error("peer: handshake failed — {0}")]
    PeerHandshake(String),
    #[error("peer: connection closed")]
    PeerDisconnected,
    #[error("peer: protocol error — {0}")]
    PeerProtocol(String),
    #[error("peer: IO error — {0}")]
    PeerIo(#[from] std::io::Error),

    // Pieces / storage
    #[error("storage: piece {0} hash mismatch")]
    PieceHashMismatch(u32),
    #[error("storage: IO error — {0}")]
    StorageIo(String),

    // Engine
    #[error("engine: torrent {0} not found")]
    TorrentNotFound(usize),
    #[error("engine: torrent already exists")]
    TorrentAlreadyExists,

    // Config
    #[error("config: {0}")]
    Config(String),

    // Generic
    #[error("{0}")]
    Other(String),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
