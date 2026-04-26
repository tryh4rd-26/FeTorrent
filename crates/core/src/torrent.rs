//! `.torrent` file parsing.
//!
//! Supports single-file and multi-file torrents (v1).
//! Extracts info_hash by SHA-1 hashing the raw bencoded info dict.

use crate::bencode::{self, BValue, Decoder};
use crate::error::{CoreError, Result};
use crate::models::{FileInfo, FilePriority, TrackerInfo};
use sha1::{Digest, Sha1};

/// Parsed representation of a .torrent file.
#[derive(Debug, Clone)]
pub struct TorrentFile {
    pub info_hash: [u8; 20],
    pub name: String,
    pub piece_length: u32,
    /// Flat list of 20-byte SHA-1 hashes, one per piece.
    pub pieces: Vec<[u8; 20]>,
    pub total_size: u64,
    pub files: Vec<TorrentFileEntry>,
    pub trackers: Vec<String>,
    pub comment: Option<String>,
    pub created_by: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TorrentFileEntry {
    pub path: String, // relative path, joined with /
    pub size: u64,
    pub offset: u64, // byte offset within the entire torrent data
}

impl Default for TorrentFile {
    fn default() -> Self {
        Self {
            info_hash: [0u8; 20],
            name: "Unknown".into(),
            piece_length: 0,
            pieces: Vec::new(),
            total_size: 0,
            files: Vec::new(),
            trackers: Vec::new(),
            comment: None,
            created_by: None,
        }
    }
}

impl TorrentFile {
    /// Parse from raw .torrent bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let root = bencode::decode(data)?;
        let dict = root
            .as_dict()
            .ok_or(CoreError::TorrentMissingField("root dict"))?;

        // ── Info hash: SHA-1 of the raw bencoded info dict ──────────────────
        let info_hash = compute_info_hash(data)?;

        let info = dict
            .get(b"info".as_ref())
            .ok_or(CoreError::TorrentMissingField("info"))?;

        let mut torrent = Self::from_info_value(info)?;
        torrent.info_hash = info_hash;

        // ── Trackers ──────────────────────────────────────────────────────
        if let Some(announce) = dict.get(b"announce".as_ref()).and_then(|v| v.as_str()) {
            torrent.trackers.push(announce.to_string());
        }
        if let Some(announce_list) = dict
            .get(b"announce-list".as_ref())
            .and_then(|v| v.as_list())
        {
            for tier in announce_list {
                if let Some(tier_list) = tier.as_list() {
                    for url in tier_list {
                        if let Some(u) = url.as_str() {
                            let u = u.to_string();
                            if !torrent.trackers.contains(&u) {
                                torrent.trackers.push(u);
                            }
                        }
                    }
                }
            }
        }

        torrent.comment = dict
            .get(b"comment".as_ref())
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        torrent.created_by = dict
            .get(b"created by".as_ref())
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok(torrent)
    }

    /// Parse specifically from the bencoded 'info' dictionary (used for magnet links).
    pub fn from_info_dict(data: &[u8]) -> Result<Self> {
        let info_value = bencode::decode(data)?;
        let mut torrent = Self::from_info_value(&info_value)?;

        // Info hash is just SHA-1 of the entire info dict
        let mut hasher = Sha1::new();
        hasher.update(data);
        torrent.info_hash = hasher.finalize().into();

        Ok(torrent)
    }

    fn from_info_value(info: &BValue) -> Result<Self> {
        let info_dict = info
            .as_dict()
            .ok_or(CoreError::TorrentMissingField("info dict"))?;

        // ── Name ──────────────────────────────────────────────────────────
        let name = info_dict
            .get(b"name".as_ref())
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        // ── Piece length ──────────────────────────────────────────────────
        let piece_length = info_dict
            .get(b"piece length".as_ref())
            .and_then(|v| v.as_int())
            .ok_or(CoreError::TorrentMissingField("piece length"))?
            as u32;

        // ── Piece hashes ──────────────────────────────────────────────────
        let pieces_raw = info_dict
            .get(b"pieces".as_ref())
            .and_then(|v| v.as_bytes())
            .ok_or(CoreError::TorrentMissingField("pieces"))?;
        if pieces_raw.len() % 20 != 0 {
            return Err(CoreError::TorrentInvalidField(
                "pieces",
                "length not multiple of 20".into(),
            ));
        }
        let pieces: Vec<[u8; 20]> = pieces_raw
            .chunks_exact(20)
            .map(|c| c.try_into().unwrap())
            .collect();

        // ── Files ─────────────────────────────────────────────────────────
        let (files, total_size) = if let Some(files_list) = info_dict.get(b"files".as_ref()) {
            // Multi-file torrent
            parse_multi_file(&name, files_list)?
        } else {
            // Single-file torrent
            let size = info_dict
                .get(b"length".as_ref())
                .and_then(|v| v.as_int())
                .ok_or(CoreError::TorrentMissingField("length"))? as u64;
            let entry = TorrentFileEntry {
                path: name.clone(),
                size,
                offset: 0,
            };
            (vec![entry], size)
        };

        Ok(TorrentFile {
            info_hash: [0u8; 20],
            name,
            piece_length,
            pieces,
            total_size,
            files,
            trackers: Vec::new(),
            comment: None,
            created_by: None,
        })
    }

    pub fn info_hash_hex(&self) -> String {
        hex::encode(self.info_hash)
    }

    pub fn num_pieces(&self) -> u32 {
        self.pieces.len() as u32
    }

    pub fn into_file_infos(&self) -> Vec<FileInfo> {
        self.files
            .iter()
            .enumerate()
            .map(|(i, f)| FileInfo {
                index: i,
                path: f.path.clone(),
                size: f.size,
                downloaded: 0,
                priority: FilePriority::Normal,
            })
            .collect()
    }

    pub fn into_tracker_infos(&self) -> Vec<TrackerInfo> {
        self.trackers
            .iter()
            .map(|url| TrackerInfo {
                url: url.clone(),
                status: "pending".into(),
                seeders: None,
                leechers: None,
                last_announce: None,
                next_announce: None,
            })
            .collect()
    }
}

fn parse_multi_file(name: &str, files_val: &BValue) -> Result<(Vec<TorrentFileEntry>, u64)> {
    let list = files_val
        .as_list()
        .ok_or(CoreError::TorrentInvalidField("files", "not a list".into()))?;
    let mut entries = Vec::new();
    let mut offset = 0u64;
    for item in list {
        let d = item.as_dict().ok_or(CoreError::TorrentInvalidField(
            "file entry",
            "not a dict".into(),
        ))?;
        let size = d
            .get(b"length".as_ref())
            .and_then(|v| v.as_int())
            .ok_or(CoreError::TorrentMissingField("file length"))? as u64;
        let path_parts = d
            .get(b"path".as_ref())
            .and_then(|v| v.as_list())
            .ok_or(CoreError::TorrentMissingField("file path"))?;
        let mut parts = vec![name.to_string()];
        for part in path_parts {
            parts.push(
                part.as_str()
                    .ok_or(CoreError::TorrentInvalidField(
                        "file path part",
                        "not utf8".into(),
                    ))?
                    .to_string(),
            );
        }
        let path = parts.join("/");
        entries.push(TorrentFileEntry { path, size, offset });
        offset += size;
    }
    Ok((entries, offset))
}

/// SHA-1 hash of the raw bencoded info dict.
fn compute_info_hash(data: &[u8]) -> Result<[u8; 20]> {
    // We need to find and hash just the "info" value's raw bytes.
    // Strategy: parse the outer dict manually to find the "info" key range.
    let mut decoder = Decoder::new(data);

    // Skip 'd'
    let _root_start = decoder.position();
    let _root = decoder
        .decode()
        .map_err(|e| CoreError::BencodeInvalid(format!("root: {}", e)))?;

    // Re-scan raw bytes to find info dict span
    // Simple approach: scan for the info key bytes
    find_and_hash_info_dict(data)
}

fn find_and_hash_info_dict(data: &[u8]) -> Result<[u8; 20]> {
    // Find "4:info" in the byte stream and hash the following bencoded value
    let key = b"4:info";
    let pos = data
        .windows(key.len())
        .position(|w| w == key)
        .ok_or(CoreError::TorrentMissingField("info key"))?;
    let info_start = pos + key.len();
    let mut d = Decoder::new(&data[info_start..]);
    let _start = d.position();
    d.decode()?;
    let end = d.position();
    let raw_info = &data[info_start..info_start + end];
    let mut hasher = Sha1::new();
    hasher.update(raw_info);
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_info_hash_computation() {
        // Basic sanity: encode a fake info dict and hash it
        let info_bytes = b"d6:lengthi1024e4:name4:teste";
        let torrent = format!(
            "d8:announce18:http://tracker.test4:info{}e",
            std::str::from_utf8(info_bytes).unwrap()
        );
        let mut hasher = Sha1::new();
        hasher.update(info_bytes);
        let expected: [u8; 20] = hasher.finalize().into();
        let result = find_and_hash_info_dict(torrent.as_bytes());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }
}
