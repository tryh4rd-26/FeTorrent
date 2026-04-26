//! BitTorrent messages framing and types.
//!
//! Message format:
//! <length prefix><message ID><payload>
//!
//! Length is 4 bytes big-endian.
//! KeepAlive: length = 0
//! Choke: ID = 0
//! Unchoke: ID = 1
//! Interested: ID = 2
//! NotInterested: ID = 3
//! Have: ID = 4
//! Bitfield: ID = 5
//! Request: ID = 6
//! Piece: ID = 7
//! Cancel: ID = 8
//! Port: ID = 9

use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

pub const MAX_FRAME_SIZE: usize = 1024 * 1024; // 1MB upper limit against abusive peers (standard max is 16KB + header)

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    KeepAlive,
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have {
        piece_index: u32,
    },
    Bitfield(Vec<u8>),
    Request {
        index: u32,
        begin: u32,
        length: u32,
    },
    Piece {
        index: u32,
        begin: u32,
        block: Vec<u8>,
    },
    Cancel {
        index: u32,
        begin: u32,
        length: u32,
    },
    Port {
        listen_port: u16,
    },
    // BEP 10 Extended message support would go here
    Extended {
        msg_id: u8,
        payload: Vec<u8>,
    },
}

pub struct MessageCodec {}

impl Default for MessageCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageCodec {
    pub fn new() -> Self {
        Self {}
    }
}

// Implement Decoder for Tokio streams
impl Decoder for MessageCodec {
    type Item = Message;
    type Error = std::io::Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> std::result::Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            return Ok(None); // Need at least length prefix
        }

        // Peek the length prefix without consuming
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_be_bytes(length_bytes) as usize;

        if length > MAX_FRAME_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "frame too large",
            ));
        }

        if src.len() < 4 + length {
            return Ok(None); // Frame not fully buffered
        }

        // Consume frame
        src.advance(4); // Consume length

        if length == 0 {
            return Ok(Some(Message::KeepAlive));
        }

        let msg_id = src.get_u8(); // length > 0 means at least 1 byte ID

        let payload_len = length - 1;

        let msg = match msg_id {
            0 => {
                if payload_len != 0 {
                    return Err(bad_data("Choke payload must be 0"));
                }
                Message::Choke
            }
            1 => {
                if payload_len != 0 {
                    return Err(bad_data("Unchoke payload must be 0"));
                }
                Message::Unchoke
            }
            2 => {
                if payload_len != 0 {
                    return Err(bad_data("Interested payload must be 0"));
                }
                Message::Interested
            }
            3 => {
                if payload_len != 0 {
                    return Err(bad_data("NotInterested payload must be 0"));
                }
                Message::NotInterested
            }
            4 => {
                if payload_len != 4 {
                    return Err(bad_data("Have must have 4 byte payload"));
                }
                Message::Have {
                    piece_index: src.get_u32(),
                }
            }
            5 => {
                let mut bitfield = vec![0u8; payload_len];
                src.copy_to_slice(&mut bitfield);
                Message::Bitfield(bitfield)
            }
            6 => {
                if payload_len != 12 {
                    return Err(bad_data("Request must have 12 byte payload"));
                }
                Message::Request {
                    index: src.get_u32(),
                    begin: src.get_u32(),
                    length: src.get_u32(),
                }
            }
            7 => {
                if payload_len < 8 {
                    return Err(bad_data("Piece must have at least 8 bytes payload"));
                }
                let index = src.get_u32();
                let begin = src.get_u32();
                let mut block = vec![0u8; payload_len - 8];
                src.copy_to_slice(&mut block);
                Message::Piece {
                    index,
                    begin,
                    block,
                }
            }
            8 => {
                if payload_len != 12 {
                    return Err(bad_data("Cancel must have 12 byte payload"));
                }
                Message::Cancel {
                    index: src.get_u32(),
                    begin: src.get_u32(),
                    length: src.get_u32(),
                }
            }
            9 => {
                if payload_len != 2 {
                    return Err(bad_data("Port must have 2 byte payload"));
                }
                Message::Port {
                    listen_port: src.get_u16(),
                }
            }
            20 => {
                // BEP 10
                if payload_len < 1 {
                    return Err(bad_data("Extended message needs extended msg_id"));
                }
                let ext_id = src.get_u8();
                let mut ext_payload = vec![0u8; payload_len - 1];
                src.copy_to_slice(&mut ext_payload);
                Message::Extended {
                    msg_id: ext_id,
                    payload: ext_payload,
                }
            }
            _ => {
                // Ignore unknown message types but consume payload
                let mut ignored = vec![0u8; payload_len];
                src.copy_to_slice(&mut ignored);
                tracing::debug!("Ignored unknown message id {}", msg_id);
                // Return a keep-alive as a no-op instead of failing the stream
                return Ok(Some(Message::KeepAlive));
            }
        };

        Ok(Some(msg))
    }
}

// Implement Encoder for Tokio streams
impl Encoder<Message> for MessageCodec {
    type Error = std::io::Error;

    fn encode(
        &mut self,
        item: Message,
        dst: &mut BytesMut,
    ) -> std::result::Result<(), Self::Error> {
        match item {
            Message::KeepAlive => {
                dst.put_u32(0);
            }
            Message::Choke => {
                dst.put_u32(1);
                dst.put_u8(0);
            }
            Message::Unchoke => {
                dst.put_u32(1);
                dst.put_u8(1);
            }
            Message::Interested => {
                dst.put_u32(1);
                dst.put_u8(2);
            }
            Message::NotInterested => {
                dst.put_u32(1);
                dst.put_u8(3);
            }
            Message::Have { piece_index } => {
                dst.put_u32(5);
                dst.put_u8(4);
                dst.put_u32(piece_index);
            }
            Message::Bitfield(bitfield) => {
                dst.put_u32(1 + bitfield.len() as u32);
                dst.put_u8(5);
                dst.extend_from_slice(&bitfield);
            }
            Message::Request {
                index,
                begin,
                length,
            } => {
                dst.put_u32(13);
                dst.put_u8(6);
                dst.put_u32(index);
                dst.put_u32(begin);
                dst.put_u32(length);
            }
            Message::Piece {
                index,
                begin,
                block,
            } => {
                dst.put_u32(9 + block.len() as u32);
                dst.put_u8(7);
                dst.put_u32(index);
                dst.put_u32(begin);
                dst.extend_from_slice(&block);
            }
            Message::Cancel {
                index,
                begin,
                length,
            } => {
                dst.put_u32(13);
                dst.put_u8(8);
                dst.put_u32(index);
                dst.put_u32(begin);
                dst.put_u32(length);
            }
            Message::Port { listen_port } => {
                dst.put_u32(3);
                dst.put_u8(9);
                dst.put_u16(listen_port);
            }
            Message::Extended { msg_id, payload } => {
                dst.put_u32(2 + payload.len() as u32);
                dst.put_u8(20);
                dst.put_u8(msg_id);
                dst.extend_from_slice(&payload);
            }
        }
        Ok(())
    }
}

fn bad_data(msg: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, msg)
}
