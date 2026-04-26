//! Magnet link parser (BEP 9).
//!
//! Handles: `magnet:?xt=urn:btih:<hex|base32>&dn=<name>&tr=<tracker>&...`

use crate::error::{CoreError, Result};

#[derive(Debug, Clone)]
pub struct MagnetLink {
    /// Raw 20-byte info hash.
    pub info_hash: [u8; 20],
    /// Display name (dn parameter).
    pub display_name: Option<String>,
    /// Tracker URLs (tr parameters).
    pub trackers: Vec<String>,
    /// Exact length hint (xl parameter).
    pub exact_length: Option<u64>,
}

impl MagnetLink {
    pub fn parse(uri: &str) -> Result<Self> {
        let uri = uri.trim();
        if !uri.starts_with("magnet:?") {
            return Err(CoreError::MagnetInvalid(
                "must start with 'magnet:?'".into(),
            ));
        }

        let query = &uri["magnet:?".len()..];
        let params = parse_query_string(query);

        // xt=urn:btih:<hash>
        let xt = params
            .iter()
            .find(|(k, _)| k == "xt")
            .map(|(_, v)| v.as_str())
            .ok_or_else(|| CoreError::MagnetInvalid("missing xt parameter".into()))?;

        let hash_str = if let Some(hash) = xt.strip_prefix("urn:btih:") {
            hash
        } else {
            return Err(CoreError::MagnetInvalid(format!("unsupported xt: {}", xt)));
        };

        let info_hash = decode_info_hash(hash_str)?;

        let display_name = params
            .iter()
            .find(|(k, _)| k == "dn")
            .map(|(_, v)| url_decode(v));

        let trackers = params
            .iter()
            .filter(|(k, _)| k == "tr")
            .map(|(_, v)| url_decode(v))
            .collect();

        let exact_length = params
            .iter()
            .find(|(k, _)| k == "xl")
            .and_then(|(_, v)| v.parse::<u64>().ok());

        Ok(MagnetLink {
            info_hash,
            display_name,
            trackers,
            exact_length,
        })
    }

    pub fn info_hash_hex(&self) -> String {
        hex::encode(self.info_hash)
    }

    pub fn name(&self) -> &str {
        self.display_name.as_deref().unwrap_or("Unknown")
    }

    /// Build a magnet URI from this struct.
    pub fn to_uri(&self) -> String {
        let mut uri = format!("magnet:?xt=urn:btih:{}", self.info_hash_hex());
        if let Some(dn) = &self.display_name {
            uri.push_str(&format!("&dn={}", urlencoding::encode(dn)));
        }
        for tr in &self.trackers {
            uri.push_str(&format!("&tr={}", urlencoding::encode(tr)));
        }
        uri
    }
}

/// Parse `key=value&key=value` — values may be URL-encoded.
fn parse_query_string(s: &str) -> Vec<(String, String)> {
    s.split('&')
        .filter_map(|part| {
            let mut it = part.splitn(2, '=');
            let k = it.next()?.to_string();
            let v = it.next().unwrap_or("").to_string();
            Some((k, v))
        })
        .collect()
}

fn url_decode(s: &str) -> String {
    urlencoding::decode(s)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| s.to_string())
}

/// Decode a hex (40 chars) or base32 (32 chars) info hash.
fn decode_info_hash(s: &str) -> Result<[u8; 20]> {
    let s = s.trim();
    match s.len() {
        40 => {
            let bytes = hex::decode(s)
                .map_err(|_| CoreError::MagnetInvalid(format!("invalid hex hash: {}", s)))?;
            bytes
                .try_into()
                .map_err(|_| CoreError::MagnetInvalid("hash wrong length".into()))
        }
        32 => decode_base32(s),
        _ => Err(CoreError::MagnetInvalid(format!(
            "hash length {} invalid (expected 40 or 32)",
            s.len()
        ))),
    }
}

/// RFC 4648 base32 decoding (uppercase, no padding in magnets).
fn decode_base32(s: &str) -> Result<[u8; 20]> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let s = s.to_uppercase();
    let mut bits: u64 = 0;
    let mut bit_count: u32 = 0;
    let mut out = Vec::with_capacity(20);

    for c in s.bytes() {
        let val = ALPHABET.iter().position(|&x| x == c).ok_or_else(|| {
            CoreError::MagnetInvalid(format!("invalid base32 char: {}", c as char))
        })? as u64;
        bits = (bits << 5) | val;
        bit_count += 5;
        if bit_count >= 8 {
            bit_count -= 8;
            out.push((bits >> bit_count) as u8);
            bits &= (1 << bit_count) - 1;
        }
    }

    if out.len() != 20 {
        return Err(CoreError::MagnetInvalid(format!(
            "base32 decoded to {} bytes, expected 20",
            out.len()
        )));
    }
    Ok(out.try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MAGNET: &str = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Big+Buck+Bunny&tr=udp%3A%2F%2Fexplodie.org%3A6969";

    #[test]
    fn test_parse_magnet() {
        let m = MagnetLink::parse(SAMPLE_MAGNET).unwrap();
        assert_eq!(
            m.info_hash_hex(),
            "dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c"
        );
        assert_eq!(m.display_name.as_deref(), Some("Big+Buck+Bunny"));
        assert_eq!(m.trackers.len(), 1);
    }

    #[test]
    fn test_roundtrip() {
        let m = MagnetLink::parse(SAMPLE_MAGNET).unwrap();
        let uri = m.to_uri();
        let m2 = MagnetLink::parse(&uri).unwrap();
        assert_eq!(m.info_hash, m2.info_hash);
    }

    #[test]
    fn test_bad_magnet() {
        assert!(MagnetLink::parse("http://not-a-magnet").is_err());
        assert!(MagnetLink::parse("magnet:?dn=noxt").is_err());
    }
}
