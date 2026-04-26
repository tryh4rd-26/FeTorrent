use crate::error::{CoreError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInfo {
    pub id: usize,
    pub name: String,
    pub info_hash: String, // Hex string
    pub magnet: Option<String>,
    pub save_path: String,

    pub status: TorrentStatus,
    pub progress: f32, // 0.0 to 1.0
    pub downloaded: u64,
    pub uploaded: u64,
    pub total_size: u64,

    pub dl_speed: u64, // bytes/s
    pub ul_speed: u64, // bytes/s
    pub eta_secs: Option<u64>,
    pub ratio: f32,

    pub num_peers: usize,
    pub num_seeds: usize,
    pub num_leechers: usize,
    pub num_pieces: u32,
    pub piece_length: u32,

    pub files: Vec<FileInfo>,
    pub trackers: Vec<TrackerInfo>,
    #[serde(default)]
    pub logs: Vec<ActivityLog>,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLog {
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub level: String,
}

impl TorrentInfo {
    pub fn is_complete(&self) -> bool {
        self.downloaded >= self.total_size && self.total_size > 0
    }

    pub fn get_info_hash_bytes(&self) -> Result<[u8; 20]> {
        if self.info_hash.len() != 40 {
            return Err(CoreError::TorrentInvalidField(
                "info_hash",
                "invalid hex length".into(),
            ));
        }
        let bytes = hex::decode(&self.info_hash)
            .map_err(|e| CoreError::TorrentInvalidField("info_hash", e.to_string()))?;
        let arr: [u8; 20] = bytes
            .try_into()
            .map_err(|_| CoreError::TorrentInvalidField("info_hash", "not 20 bytes".into()))?;
        Ok(arr)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TorrentStatus {
    Queued,
    Checking,
    Downloading,
    Seeding,
    Paused,
    Finished,
    DownloadingMetadata,
    Error(String),
}

impl std::fmt::Display for TorrentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queued => write!(f, "Queued"),
            Self::Checking => write!(f, "Checking"),
            Self::Downloading => write!(f, "Downloading"),
            Self::Seeding => write!(f, "Seeding"),
            Self::Paused => write!(f, "Paused"),
            Self::Finished => write!(f, "Finished"),
            Self::DownloadingMetadata => write!(f, "Downloading Metadata"),
            Self::Error(e) => write!(f, "Error: {}", e),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub index: usize,
    pub path: String,
    pub size: u64,
    pub downloaded: u64,
    pub priority: FilePriority,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FilePriority {
    Skip,
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerInfo {
    pub url: String,
    pub status: String,
    pub seeders: Option<u32>,
    pub leechers: Option<u32>,
    pub last_announce: Option<DateTime<Utc>>,
    pub next_announce: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub addr: String,
    pub client: String,
    pub progress: f32,
    pub dl_speed: u64,
    pub ul_speed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalStats {
    pub dl_speed: u64,
    pub ul_speed: u64,
    pub total_downloaded: u64,
    pub total_uploaded: u64,
    pub active_torrents: usize,
    pub seeding_torrents: usize,
    pub paused_torrents: usize,
    pub ratio: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TorrentEvent {
    TorrentAdded {
        torrent: TorrentInfo,
    },
    TorrentUpdated {
        torrent: TorrentInfo,
    },
    TorrentRemoved {
        id: usize,
    },
    StatsUpdate {
        torrents: Vec<TorrentInfo>,
        global: GlobalStats,
    },
    Log {
        level: String,
        message: String,
    },
}
