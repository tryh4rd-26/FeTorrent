//! Bencode parser and encoder — pure Rust, zero-copy where possible.
//!
//! Bencode format:
//!   Integer:    i<decimal>e         e.g. i42e
//!   Bytes/Str:  <len>:<data>        e.g. 4:spam
//!   List:       l<items>e           e.g. l4:spami42ee
//!   Dictionary: d<key><val>...e     e.g. d4:spami42ee  (keys must be sorted)
//!
//! SECURITY: Added depth limit and allocation bounds checks.

use crate::error::CoreError;
use std::collections::BTreeMap;

const MAX_DECODE_DEPTH: usize = 100; // Prevent DoS from nested structures
const MAX_BYTES_ALLOCATION: usize = 100 * 1024 * 1024; // 100MB limit per value

/// A decoded bencode value.
#[derive(Debug, Clone, PartialEq)]
pub enum BValue {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<BValue>),
    Dict(BTreeMap<Vec<u8>, BValue>),
}

impl BValue {
    /// Convenience: try to get as a UTF-8 string slice.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            BValue::Bytes(b) => std::str::from_utf8(b).ok(),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            BValue::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            BValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[BValue]> {
        match self {
            BValue::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_dict(&self) -> Option<&BTreeMap<Vec<u8>, BValue>> {
        match self {
            BValue::Dict(d) => Some(d),
            _ => None,
        }
    }

    /// Get a dict key as a string key.
    pub fn dict_get(&self, key: &str) -> Option<&BValue> {
        self.as_dict()?.get(key.as_bytes())
    }
}

// ─── Parser ──────────────────────────────────────────────────────────────────

pub struct Decoder<'a> {
    buf: &'a [u8],
    pos: usize,
    depth: usize, // Track recursion depth
}

impl<'a> Decoder<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self {
            buf,
            pos: 0,
            depth: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.buf.get(self.pos).copied()
    }

    fn consume(&mut self) -> Result<u8, CoreError> {
        let b = self
            .buf
            .get(self.pos)
            .copied()
            .ok_or(CoreError::BencodePrematureEnd)?;
        self.pos += 1;
        Ok(b)
    }

    fn expect(&mut self, expected: u8) -> Result<(), CoreError> {
        let b = self.consume()?;
        if b != expected {
            return Err(CoreError::BencodeInvalid(format!(
                "expected '{}' got '{}'",
                expected as char, b as char
            )));
        }
        Ok(())
    }

    pub fn decode(&mut self) -> Result<BValue, CoreError> {
        if self.depth > MAX_DECODE_DEPTH {
            return Err(CoreError::BencodeInvalid("Depth limit exceeded".into()));
        }
        self.depth += 1;
        let result = match self.peek().ok_or(CoreError::BencodePrematureEnd)? {
            b'i' => self.decode_int(),
            b'l' => self.decode_list(),
            b'd' => self.decode_dict(),
            b'0'..=b'9' => self.decode_bytes(),
            b => Err(CoreError::BencodeInvalid(format!(
                "unexpected byte: {}",
                b as char
            ))),
        };
        self.depth -= 1;
        result
    }

    fn decode_int(&mut self) -> Result<BValue, CoreError> {
        self.expect(b'i')?;
        let start = self.pos;
        let mut found_e = false;
        while self.pos < self.buf.len() {
            if self.buf[self.pos] == b'e' {
                found_e = true;
                break;
            }
            self.pos += 1;
        }
        if !found_e {
            return Err(CoreError::BencodePrematureEnd);
        }
        let s = std::str::from_utf8(&self.buf[start..self.pos])
            .map_err(|_| CoreError::BencodeInvalid("int not utf8".into()))?;
        let n: i64 = s
            .parse()
            .map_err(|_| CoreError::BencodeInvalid(format!("bad int: {}", s)))?;
        self.expect(b'e')?;
        Ok(BValue::Int(n))
    }

    fn decode_bytes(&mut self) -> Result<BValue, CoreError> {
        let len = self.decode_decimal()?;
        // SECURITY: Check allocation size
        if len > MAX_BYTES_ALLOCATION {
            return Err(CoreError::BencodeInvalid("allocation too large".into()));
        }
        self.expect(b':')?;
        let end = self
            .pos
            .checked_add(len)
            .ok_or(CoreError::BencodeInvalid("position overflow".into()))?;
        if end > self.buf.len() {
            return Err(CoreError::BencodePrematureEnd);
        }
        let data = self.buf[self.pos..end].to_vec();
        self.pos = end;
        Ok(BValue::Bytes(data))
    }

    fn decode_decimal(&mut self) -> Result<usize, CoreError> {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let s = std::str::from_utf8(&self.buf[start..self.pos])
            .map_err(|_| CoreError::BencodeInvalid("decimal not utf8".into()))?;
        s.parse()
            .map_err(|_| CoreError::BencodeInvalid(format!("bad decimal: {}", s)))
    }

    fn decode_list(&mut self) -> Result<BValue, CoreError> {
        self.expect(b'l')?;
        let mut list = Vec::new();
        while self.peek() != Some(b'e') {
            list.push(self.decode()?);
        }
        self.expect(b'e')?;
        Ok(BValue::List(list))
    }

    fn decode_dict(&mut self) -> Result<BValue, CoreError> {
        self.expect(b'd')?;
        let mut map = BTreeMap::new();
        while self.peek() != Some(b'e') {
            let key = match self.decode_bytes()? {
                BValue::Bytes(b) => b,
                _ => unreachable!(),
            };
            let val = self.decode()?;
            map.insert(key, val);
        }
        self.expect(b'e')?;
        Ok(BValue::Dict(map))
    }

    pub fn raw_value_at(&self, start: usize) -> &[u8] {
        &self.buf[start..self.pos]
    }

    pub fn position(&self) -> usize {
        self.pos
    }
}

/// Decode a full bencoded buffer.
pub fn decode(buf: &[u8]) -> Result<BValue, CoreError> {
    let mut d = Decoder::new(buf);
    d.decode()
}

/// Decode and also return the byte range of the top-level value.
pub fn decode_with_range(buf: &[u8]) -> Result<(BValue, usize, usize), CoreError> {
    let mut d = Decoder::new(buf);
    let start = d.pos;
    let v = d.decode()?;
    Ok((v, start, d.pos))
}

// ─── Encoder ─────────────────────────────────────────────────────────────────

pub fn encode(v: &BValue) -> Vec<u8> {
    let mut out = Vec::new();
    encode_into(v, &mut out);
    out
}

fn encode_into(v: &BValue, out: &mut Vec<u8>) {
    match v {
        BValue::Int(n) => {
            out.push(b'i');
            out.extend_from_slice(n.to_string().as_bytes());
            out.push(b'e');
        }
        BValue::Bytes(b) => {
            out.extend_from_slice(b.len().to_string().as_bytes());
            out.push(b':');
            out.extend_from_slice(b);
        }
        BValue::List(l) => {
            out.push(b'l');
            for item in l {
                encode_into(item, out);
            }
            out.push(b'e');
        }
        BValue::Dict(d) => {
            out.push(b'd');
            for (k, v) in d {
                out.extend_from_slice(k.len().to_string().as_bytes());
                out.push(b':');
                out.extend_from_slice(k);
                encode_into(v, out);
            }
            out.push(b'e');
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_int() {
        assert_eq!(decode(b"i42e").unwrap(), BValue::Int(42));
        assert_eq!(decode(b"i-7e").unwrap(), BValue::Int(-7));
    }

    #[test]
    fn test_bytes() {
        assert_eq!(decode(b"4:spam").unwrap(), BValue::Bytes(b"spam".to_vec()));
    }

    #[test]
    fn test_list() {
        let v = decode(b"l4:spami42ee").unwrap();
        assert_eq!(
            v,
            BValue::List(vec![BValue::Bytes(b"spam".to_vec()), BValue::Int(42)])
        );
    }

    #[test]
    fn test_dict() {
        let v = decode(b"d3:cow3:moo4:spam4:eggse").unwrap();
        let d = v.as_dict().unwrap();
        assert_eq!(d[b"cow".as_ref()], BValue::Bytes(b"moo".to_vec()));
        assert_eq!(d[b"spam".as_ref()], BValue::Bytes(b"eggs".to_vec()));
    }

    #[test]
    fn test_roundtrip() {
        let original = b"d3:bari42e3:fool4:spame4:name4:teste";
        let v = decode(original).unwrap();
        let re = encode(&v);
        assert_eq!(decode(&re).unwrap(), v);
    }

    #[test]
    fn test_recursion_limit() {
        let mut deep = String::new();
        for _ in 0..110 {
            deep.push('l');
        }
        for _ in 0..110 {
            deep.push('e');
        }
        let result = decode(deep.as_bytes());
        assert!(result.is_err());
        assert!(format!("{:?}", result.err()).contains("Depth limit exceeded"));
    }

    #[test]
    fn test_allocation_limit() {
        // 100MB + 1 byte
        let huge = format!("{}:", 100 * 1024 * 1024 + 1);
        let result = decode(huge.as_bytes());
        assert!(result.is_err());
        assert!(format!("{:?}", result.err()).contains("allocation too large"));
    }
}
