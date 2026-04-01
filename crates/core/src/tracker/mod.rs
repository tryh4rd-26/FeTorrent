//! Tracker communication — HTTP and UDP tracker protocols.

pub mod http;
pub mod udp;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct AnnounceRequest {
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
    pub port: u16,
    pub uploaded: u64,
    pub downloaded: u64,
    pub left: u64,
    pub event: AnnounceEvent,
    pub num_want: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnnounceEvent {
    None,
    Started,
    Stopped,
    Completed,
}

impl AnnounceEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::Started => "started",
            Self::Stopped => "stopped",
            Self::Completed => "completed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnnounceResponse {
    pub interval: u32,
    pub seeders: u32,
    pub leechers: u32,
    pub peers: Vec<TrackerPeer>,
}

#[async_trait::async_trait]
pub trait Tracker: Send + Sync {
    async fn announce(&self, req: AnnounceRequest) -> Result<AnnounceResponse>;
    fn url(&self) -> &str;
}

#[derive(Debug, Clone)]
pub struct TrackerPeer {
    pub ip: std::net::IpAddr,
    pub port: u16,
}

impl TrackerPeer {
    pub fn addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::new(self.ip, self.port)
    }
}
