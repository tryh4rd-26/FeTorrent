//! BitTorrent protocol handshake.
//!
//! Format: <pstrlen><pstr><reserved><info_hash><peer_id>
//! pstrlen = 19
//! pstr = "BitTorrent protocol"
//! reserved = 8 zero bytes (mostly)
//! info_hash = 20 bytes
//! peer_id = 20 bytes

use crate::error::{CoreError, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const HANDSHAKE_LEN: usize = 68;
pub const PROTOCOL_STRING: &[u8; 19] = b"BitTorrent protocol";

#[derive(Debug, Clone)]
pub struct Handshake {
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
    pub reserved: [u8; 8],
}

impl Handshake {
    pub fn new(info_hash: [u8; 20], peer_id: [u8; 20]) -> Self {
        Self {
            info_hash,
            peer_id,
            // Bit 43 (0x10) is extension protocol (BEP 10), Bit 0 (0x01) is used for DHT sometimes, etc.
            reserved: [0, 0, 0, 0, 0, 0x10, 0, 0], // Advertise extension support
        }
    }

    pub fn to_bytes(&self) -> [u8; HANDSHAKE_LEN] {
        let mut buf = [0u8; HANDSHAKE_LEN];
        buf[0] = 19;
        buf[1..20].copy_from_slice(PROTOCOL_STRING);
        buf[20..28].copy_from_slice(&self.reserved);
        buf[28..48].copy_from_slice(&self.info_hash);
        buf[48..68].copy_from_slice(&self.peer_id);
        buf
    }

    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < HANDSHAKE_LEN {
            return Err(CoreError::PeerHandshake("buffer too small".into()));
        }

        let pstrlen = buf[0];
        if pstrlen != 19 {
            return Err(CoreError::PeerHandshake(format!(
                "invalid pstrlen: {}",
                pstrlen
            )));
        }

        if &buf[1..20] != PROTOCOL_STRING {
            return Err(CoreError::PeerHandshake("invalid protocol string".into()));
        }

        let mut reserved = [0u8; 8];
        reserved.copy_from_slice(&buf[20..28]);

        let mut info_hash = [0u8; 20];
        info_hash.copy_from_slice(&buf[28..48]);

        let mut peer_id = [0u8; 20];
        peer_id.copy_from_slice(&buf[48..68]);

        Ok(Self {
            info_hash,
            peer_id,
            reserved,
        })
    }

    /// Perform a full handshake transaction with a peer.
    pub async fn exchange(stream: &mut TcpStream, own_handshake: &Handshake) -> Result<Handshake> {
        // Send our handshake
        let bytes = own_handshake.to_bytes();
        stream
            .write_all(&bytes)
            .await
            .map_err(Into::<CoreError>::into)?;

        // Read peer's handshake
        let mut resp = [0u8; HANDSHAKE_LEN];
        stream
            .read_exact(&mut resp)
            .await
            .map_err(Into::<CoreError>::into)?;

        let peer_handshake = Handshake::from_bytes(&resp)?;

        // Verify info hash matches
        if peer_handshake.info_hash != own_handshake.info_hash {
            return Err(CoreError::PeerHandshake(format!(
                "info_hash mismatch! expected {}, got {}",
                hex::encode(own_handshake.info_hash),
                hex::encode(peer_handshake.info_hash)
            )));
        }

        Ok(peer_handshake)
    }
}
