//! UDP tracker protocol (BEP 15).
//!
//! Flow:
//!   1. Send connect request  → receive connect response (connection_id)
//!   2. Send announce request → receive announce response (peers)

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;
use rand::Rng;
use crate::error::{CoreError, Result};
use super::{AnnounceEvent, AnnounceRequest, AnnounceResponse, TrackerPeer};

const CONNECT_MAGIC: u64 = 0x41727101980;
const ACTION_CONNECT: u32 = 0;
const ACTION_ANNOUNCE: u32 = 1;
const TIMEOUT: Duration = Duration::from_secs(5);

pub struct UdpTracker {
    url: String,
}

impl UdpTracker {
    pub fn from_url(url: &str) -> Result<Self> {
        Ok(Self { url: url.to_string() })
    }
}

#[async_trait::async_trait]
impl super::Tracker for UdpTracker {
    async fn announce(&self, req: AnnounceRequest) -> Result<AnnounceResponse> {
        // Resolve host
        let host = self.url.trim_start_matches("udp://");
        let addrs = tokio::net::lookup_host(host).await
            .map_err(|e| CoreError::TrackerUdp(format!("lookup: {}", e)))?;
        let addr = addrs.into_iter().next()
            .ok_or_else(|| CoreError::TrackerUdp("no addresses found".into()))?;

        let socket = UdpSocket::bind("0.0.0.0:0").await
            .map_err(|e| CoreError::TrackerUdp(format!("bind: {}", e)))?;
        socket.connect(addr).await
            .map_err(|e| CoreError::TrackerUdp(format!("connect: {}", e)))?;

        // Step 1: connect
        let connection_id = self.do_connect(&socket).await?;

        // Step 2: announce
        self.do_announce(&socket, connection_id, &req).await
    }

    fn url(&self) -> &str {
        &self.url
    }
}

impl UdpTracker {

    async fn do_connect(&self, socket: &UdpSocket) -> Result<u64> {
        let transaction_id: u32 = rand::thread_rng().gen();
        let mut buf = [0u8; 16];
        buf[0..8].copy_from_slice(&CONNECT_MAGIC.to_be_bytes());
        buf[8..12].copy_from_slice(&ACTION_CONNECT.to_be_bytes());
        buf[12..16].copy_from_slice(&transaction_id.to_be_bytes());

        send_with_timeout(socket, &buf, TIMEOUT).await?;

        let mut resp = [0u8; 16];
        recv_with_timeout(socket, &mut resp, TIMEOUT).await?;

        let action = u32::from_be_bytes(resp[0..4].try_into().unwrap());
        let resp_tid = u32::from_be_bytes(resp[4..8].try_into().unwrap());
        if action != ACTION_CONNECT {
            return Err(CoreError::TrackerUdp(format!("expected action 0, got {}", action)));
        }
        if resp_tid != transaction_id {
            return Err(CoreError::TrackerUdp(format!("connect: transaction ID mismatch (sent {}, got {})", transaction_id, resp_tid)));
        }
        Ok(u64::from_be_bytes(resp[8..16].try_into().unwrap()))
    }

    async fn do_announce(&self, socket: &UdpSocket, connection_id: u64, req: &AnnounceRequest) -> Result<AnnounceResponse> {
        let transaction_id: u32 = rand::thread_rng().gen();
        let mut buf = Vec::with_capacity(98);
        buf.extend_from_slice(&connection_id.to_be_bytes());
        buf.extend_from_slice(&ACTION_ANNOUNCE.to_be_bytes());
        buf.extend_from_slice(&transaction_id.to_be_bytes());
        buf.extend_from_slice(&req.info_hash);
        buf.extend_from_slice(&req.peer_id);
        buf.extend_from_slice(&req.downloaded.to_be_bytes());
        buf.extend_from_slice(&req.left.to_be_bytes());
        buf.extend_from_slice(&req.uploaded.to_be_bytes());
        let event_code: u32 = match req.event {
            AnnounceEvent::None => 0,
            AnnounceEvent::Completed => 1,
            AnnounceEvent::Started => 2,
            AnnounceEvent::Stopped => 3,
        };
        buf.extend_from_slice(&event_code.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes()); // IP (default)
        buf.extend_from_slice(&rand::thread_rng().gen::<u32>().to_be_bytes()); // key
        buf.extend_from_slice(&(req.num_want as i32).to_be_bytes());
        buf.extend_from_slice(&req.port.to_be_bytes());

        send_with_timeout(socket, &buf, TIMEOUT).await?;

        let mut resp = vec![0u8; 4096];
        let n = recv_with_timeout(socket, &mut resp, TIMEOUT).await?;
        resp.truncate(n);

        if n < 20 {
            return Err(CoreError::TrackerUdp("announce response too short".into()));
        }

        let action = u32::from_be_bytes(resp[0..4].try_into().unwrap());
        if action != ACTION_ANNOUNCE {
            return Err(CoreError::TrackerUdp(format!("expected action 1, got {}", action)));
        }

        let interval = u32::from_be_bytes(resp[8..12].try_into().unwrap());
        let leechers = u32::from_be_bytes(resp[12..16].try_into().unwrap());
        let seeders = u32::from_be_bytes(resp[16..20].try_into().unwrap());

        let mut peers = Vec::new();
        for chunk in resp[20..].chunks_exact(6) {
            let ip = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
            let port = u16::from_be_bytes([chunk[4], chunk[5]]);
            peers.push(TrackerPeer { ip: IpAddr::V4(ip), port });
        }

        Ok(AnnounceResponse { interval, seeders, leechers, peers })
    }
}

async fn send_with_timeout(socket: &UdpSocket, buf: &[u8], timeout: Duration) -> Result<()> {
    tokio::time::timeout(timeout, socket.send(buf))
        .await
        .map_err(|_| CoreError::TrackerTimeout)?
        .map_err(|e| CoreError::TrackerUdp(e.to_string()))?;
    Ok(())
}

async fn recv_with_timeout(socket: &UdpSocket, buf: &mut [u8], timeout: Duration) -> Result<usize> {
    tokio::time::timeout(timeout, socket.recv(buf))
        .await
        .map_err(|_| CoreError::TrackerTimeout)?
        .map_err(|e| CoreError::TrackerUdp(e.to_string()))
}
