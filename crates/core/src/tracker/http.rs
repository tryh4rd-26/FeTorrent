//! HTTP tracker client (BEP 3).
//!
//! GET http://tracker/announce?info_hash=...&peer_id=...&...
//! Response is a bencoded dictionary.

use super::{AnnounceEvent, AnnounceRequest, AnnounceResponse, TrackerPeer};
use crate::bencode;
use crate::error::{CoreError, Result};
use reqwest::Client;
use std::net::{IpAddr, Ipv4Addr};

pub struct HttpTracker {
    client: Client,
    url: String,
}

#[async_trait::async_trait]
impl super::Tracker for HttpTracker {
    async fn announce(&self, req: AnnounceRequest) -> Result<AnnounceResponse> {
        let url = self.build_url(&req);
        tracing::debug!("HTTP tracker announce: {}", url);

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "FeTorrent/0.1")
            .send()
            .await
            .map_err(|e| CoreError::TrackerHttp(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(CoreError::TrackerHttp(format!("HTTP {}", resp.status())));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CoreError::TrackerHttp(e.to_string()))?;

        self.parse_response(&bytes)
    }

    fn url(&self) -> &str {
        &self.url
    }
}

impl HttpTracker {
    pub fn new(url: &str) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to build HTTP client");
        Self {
            client,
            url: url.to_string(),
        }
    }

    fn build_url(&self, req: &AnnounceRequest) -> String {
        // info_hash and peer_id must be URL-encoded as raw bytes
        let info_hash = url_encode_bytes(&req.info_hash);
        let peer_id = url_encode_bytes(&req.peer_id);
        let event = if req.event != AnnounceEvent::None {
            format!("&event={}", req.event.as_str())
        } else {
            String::new()
        };
        let sep = if self.url.contains('?') { '&' } else { '?' };
        format!(
            "{}{}info_hash={}&peer_id={}&port={}&uploaded={}&downloaded={}&left={}&compact=1&numwant={}{}",
            self.url, sep,
            info_hash, peer_id,
            req.port, req.uploaded, req.downloaded, req.left,
            req.num_want,
            event,
        )
    }

    fn parse_response(&self, data: &[u8]) -> Result<AnnounceResponse> {
        let root = bencode::decode(data)?;
        let dict = root
            .as_dict()
            .ok_or(CoreError::TrackerHttp("response not a dict".into()))?;

        // Check for tracker failure
        if let Some(reason) = dict.get(b"failure reason".as_ref()) {
            let msg = reason.as_str().unwrap_or("unknown").to_string();
            return Err(CoreError::TrackerFailure(msg));
        }

        let interval = dict
            .get(b"interval".as_ref())
            .and_then(|v| v.as_int())
            .unwrap_or(1800) as u32;

        let seeders = dict
            .get(b"complete".as_ref())
            .and_then(|v| v.as_int())
            .unwrap_or(0) as u32;

        let leechers = dict
            .get(b"incomplete".as_ref())
            .and_then(|v| v.as_int())
            .unwrap_or(0) as u32;

        let mut peers = Vec::new();

        if let Some(peers_val) = dict.get(b"peers".as_ref()) {
            match peers_val {
                // Compact format: 6 bytes per peer (4 IP + 2 port)
                bencode::BValue::Bytes(compact) => {
                    for chunk in compact.chunks_exact(6) {
                        let ip = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
                        let port = u16::from_be_bytes([chunk[4], chunk[5]]);
                        peers.push(TrackerPeer {
                            ip: IpAddr::V4(ip),
                            port,
                        });
                    }
                }
                // Dictionary format (non-compact)
                bencode::BValue::List(list) => {
                    for entry in list {
                        if let Some(d) = entry.as_dict() {
                            let ip_str = d.get(b"ip".as_ref()).and_then(|v| v.as_str());
                            let port = d.get(b"port".as_ref()).and_then(|v| v.as_int());
                            if let (Some(ip), Some(p)) = (ip_str, port) {
                                if let Ok(ip) = ip.parse::<IpAddr>() {
                                    peers.push(TrackerPeer { ip, port: p as u16 });
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(AnnounceResponse {
            interval,
            seeders,
            leechers,
            peers,
        })
    }
}

/// URL-encode raw bytes as %XX sequences.
fn url_encode_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
                format!("{}", b as char)
            } else {
                format!("%{:02X}", b)
            }
        })
        .collect()
}
