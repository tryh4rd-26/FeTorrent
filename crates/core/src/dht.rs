//! Distributed Hash Table (DHT) for trackerless peer discovery (BEP 5).
//!
//! Note: Full Kademlia routing table implementation is extensive.
//! This module provides the framework and basic KRPC bindings.

use crate::bencode::{self};
use crate::error::{CoreError, Result};

pub struct DhtNode {
    pub id: [u8; 20],
    pub port: u16,
    // routing_table: RoutingTable,
}

impl DhtNode {
    pub fn new(port: u16) -> Self {
        let mut id = [0u8; 20];
        // Generate random node id
        for b in &mut id {
            *b = rand::random();
        }
        Self { id, port }
    }

    /// Parse an incoming KRPC query/response dictionary.
    pub fn parse_krpc(data: &[u8]) -> Result<KrpcMessage> {
        let root = bencode::decode(data)?;
        let dict = root
            .as_dict()
            .ok_or(CoreError::Other("DHT non-dict".into()))?;

        let msg_type = dict
            .get(b"y".as_ref())
            .and_then(|v| v.as_str())
            .ok_or(CoreError::Other("DHT missing y".into()))?;

        match msg_type {
            "q" => Ok(KrpcMessage::Query),
            "r" => Ok(KrpcMessage::Response),
            "e" => Ok(KrpcMessage::Error),
            _ => Err(CoreError::Other("DHT unknown type".into())),
        }
    }
}

pub enum KrpcMessage {
    Query,
    Response,
    Error,
}
