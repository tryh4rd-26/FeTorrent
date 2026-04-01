//! Piece and block management logic.
//!
//! A torrent is divided into pieces (typically 256KB-4MB).
//! Pieces are requested in blocks (typically 16KB).

use sha1::{Digest, Sha1};
use std::collections::HashSet;

pub const BLOCK_SIZE: u32 = 16384; // 16 KiB

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PieceState {
    Missing,
    Downloading,
    Complete,     // All blocks received, hash not yet verified (or hashing now)
    Verified,     // Hash matched
    Failed,       // Hash mismatched, ready to retry
}

pub struct PieceManager {
    piece_length: u32,
    total_size: u64,
    pub num_pieces: u32,
    hashes: Vec<[u8; 20]>,
    
    // Status tracking
    pub states: Vec<PieceState>,
    
    // A bitfield mapping: have is 1, missing/other is 0.
    // Kept up-to-date with Verified state.
    pub bitfield: Vec<u8>,
}

impl PieceManager {
    pub fn new(piece_length: u32, total_size: u64, hashes: Vec<[u8; 20]>) -> Self {
        let num_pieces = hashes.len() as u32;
        let bf_len = ((num_pieces as f32) / 8.0).ceil() as usize;
        
        Self {
            piece_length,
            total_size,
            num_pieces,
            hashes,
            states: vec![PieceState::Missing; num_pieces as usize],
            bitfield: vec![0u8; bf_len],
        }
    }

    pub fn piece_length(&self) -> u32 {
        self.piece_length
    }

    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    pub fn num_pieces(&self) -> u32 {
        self.num_pieces
    }

    /// Setup the initial bitfield after a storage scan.
    pub fn init_from_storage(&mut self, have_pieces: &[bool]) {
        for (idx, &have) in have_pieces.iter().enumerate() {
            if have {
                self.states[idx] = PieceState::Verified;
                self.set_bit(idx as u32);
            }
        }
    }

    pub fn set_bit(&mut self, index: u32) {
        let byte_idx = (index / 8) as usize;
        let bit_idx = 7 - (index % 8);
        if byte_idx < self.bitfield.len() {
            self.bitfield[byte_idx] |= 1 << bit_idx;
        }
    }

    /// Check if a remote bitfield represents a seeder (all pieces present).
    pub fn is_full_bitfield(&self, bf: &[u8]) -> bool {
        if bf.len() != self.bitfield.len() {
            return false;
        }
        
        // Check all but the last byte (they must be 0xFF)
        for i in 0..bf.len().saturating_sub(1) {
            if bf[i] != 0xFF {
                return false;
            }
        }
        
        // Handle the last byte separately due to possible padding
        if let Some(&last_byte) = bf.last() {
            let num_bits_last_byte = self.num_pieces % 8;
            if num_bits_last_byte == 0 {
                return last_byte == 0xFF;
            } else {
                let mask = (0xFF << (8 - num_bits_last_byte)) as u8;
                return (last_byte & mask) == mask;
            }
        }
        
        true
    }

    pub fn has_bit(&self, index: u32) -> bool {
        let byte_idx = (index / 8) as usize;
        let bit_idx = 7 - (index % 8);
        if byte_idx < self.bitfield.len() {
            (self.bitfield[byte_idx] & (1 << bit_idx)) != 0
        } else {
            false
        }
    }

    pub fn is_complete(&self) -> bool {
        self.states.iter().all(|s| *s == PieceState::Verified)
    }

    pub fn get_piece_length(&self, index: u32) -> u32 {
        if index == self.num_pieces - 1 {
            let rem = self.total_size % self.piece_length as u64;
            if rem == 0 {
                self.piece_length
            } else {
                rem as u32
            }
        } else {
            self.piece_length
        }
    }

    pub fn get_num_blocks(&self, index: u32) -> u32 {
        let len = self.get_piece_length(index);
        (len + BLOCK_SIZE - 1) / BLOCK_SIZE
    }

    pub fn get_block_length(&self, piece_index: u32, block_index: u32) -> u32 {
        let piece_len = self.get_piece_length(piece_index);
        let offset = block_index * BLOCK_SIZE;
        if offset + BLOCK_SIZE > piece_len {
            piece_len - offset
        } else {
            BLOCK_SIZE
        }
    }

    pub fn verify_hash(&self, index: u32, data: &[u8]) -> bool {
        if (index as usize) < self.hashes.len() {
            let mut hasher = Sha1::new();
            hasher.update(data);
            let res: [u8; 20] = hasher.finalize().into();
            res == self.hashes[index as usize]
        } else {
            false
        }
    }

    /// Picks the next piece to download given a peer's bitfield.
    pub fn pick_next_piece(&self, peer_bitfield: &[u8]) -> Option<u32> {
        for i in 0..self.num_pieces {
            if self.states[i as usize] == PieceState::Missing {
                // Check if peer has it
                let byte_idx = (i / 8) as usize;
                let bit_idx = 7 - (i % 8);
                if byte_idx < peer_bitfield.len() && (peer_bitfield[byte_idx] & (1 << bit_idx)) != 0 {
                    return Some(i);
                }
            }
        }
        None
    }
}

/// Tracks the block-level state for a single piece being downloaded.
#[derive(Debug)]
pub struct ActivePiece {
    pub index: u32,
    pub total_blocks: u32,
    pub blocks_received: u32,
    pub block_mask: Vec<bool>, // true if we have the block
    pub data: Vec<u8>,         // pre-allocated buffer for the whole piece
}

impl ActivePiece {
    pub fn new(index: u32, piece_len: u32) -> Self {
        let total_blocks = (piece_len + BLOCK_SIZE - 1) / BLOCK_SIZE;
        Self {
            index,
            total_blocks,
            blocks_received: 0,
            block_mask: vec![false; total_blocks as usize],
            data: vec![0u8; piece_len as usize],
        }
    }

    pub fn add_block(&mut self, begin: u32, block_data: &[u8]) -> bool {
        let block_idx = begin / BLOCK_SIZE;
        if (block_idx as usize) < self.block_mask.len() && !self.block_mask[block_idx as usize] {
            let end = begin as usize + block_data.len();
            if end <= self.data.len() {
                self.data[begin as usize..end].copy_from_slice(block_data);
                self.block_mask[block_idx as usize] = true;
                self.blocks_received += 1;
                return true;
            }
        }
        false
    }

    pub fn is_complete(&self) -> bool {
        self.blocks_received == self.total_blocks
    }

    /// Returns the missing blocks (begin offset, length) pairs, limited to `max_requests`.
    pub fn get_missing_blocks(&self, piece_manager: &PieceManager, max_requests: usize) -> Vec<(u32, u32)> {
        let mut missing = Vec::new();
        for i in 0..self.total_blocks {
            if !self.block_mask[i as usize] {
                let begin = i * BLOCK_SIZE;
                let len = piece_manager.get_block_length(self.index, i);
                missing.push((begin, len));
                if missing.len() >= max_requests {
                    break;
                }
            }
        }
        missing
    }
}
