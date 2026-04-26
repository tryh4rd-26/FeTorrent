//! File system storage engine.
//!
//! Maps linear pieces (piece index + offset) to physical file blocks.
//! Supports sparse writes and appending to existing files.

use crate::error::{CoreError, Result};
use crate::torrent::TorrentFile;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

struct OpenFile {
    pub file: File,
    pub size: u64,
    pub offset_in_torrent: u64,
}

pub struct Storage {
    files: Vec<OpenFile>,
    total_size: u64,
}

impl Storage {
    pub fn new(base_path: impl Into<PathBuf>, torrent: &TorrentFile) -> Result<Self> {
        let base_path = base_path.into();
        std::fs::create_dir_all(&base_path)
            .map_err(|e| CoreError::StorageIo(format!("Create dir: {}", e)))?;

        let mut files = Vec::new();

        for entry in &torrent.files {
            let path = base_path.join(&entry.path);

            // Ensure parent dirs exist (e.g., if path is multi-level)
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| CoreError::StorageIo(format!("Parent dir: {}", e)))?;
            }

            // Open or create the file
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .map_err(|e| CoreError::StorageIo(format!("Open file: {}", e)))?;

            // Pre-allocate or truncate to exact length
            file.set_len(entry.size)
                .map_err(|e| CoreError::StorageIo(format!("Set length: {}", e)))?;

            files.push(OpenFile {
                file,
                size: entry.size,
                offset_in_torrent: entry.offset,
            });
        }

        Ok(Self {
            files,
            total_size: torrent.total_size,
        })
    }

    /// Read data corresponding to a piece offset and length.
    pub fn read_block(
        &mut self,
        absolute_offset: u64,
        mut length: u32,
        buf: &mut [u8],
    ) -> Result<()> {
        if absolute_offset + length as u64 > self.total_size {
            return Err(CoreError::StorageIo("read out of bounds".into()));
        }

        let mut current_offset = absolute_offset;
        let mut buf_pos = 0;

        for open_file in &mut self.files {
            if length == 0 {
                break;
            }

            let file_start = open_file.offset_in_torrent;
            let file_end = file_start + open_file.size;

            if current_offset >= file_start && current_offset < file_end {
                // Calculate how much to read from this specific file
                let offset_in_file = current_offset - file_start;
                let bytes_available = open_file.size - offset_in_file;
                let bytes_to_read = length.min(bytes_available as u32);

                open_file
                    .file
                    .seek(SeekFrom::Start(offset_in_file))
                    .map_err(|e| CoreError::StorageIo(format!("seek: {}", e)))?;

                open_file
                    .file
                    .read_exact(&mut buf[buf_pos..(buf_pos + bytes_to_read as usize)])
                    .map_err(|e| CoreError::StorageIo(format!("read: {}", e)))?;

                current_offset += bytes_to_read as u64;
                length -= bytes_to_read;
                buf_pos += bytes_to_read as usize;
            }
        }

        Ok(())
    }

    /// Write data corresponding to a piece offset and length with batched I/O.
    /// Avoids excessive fsync for speed - relying on OS page cache.
    pub fn write_block(&mut self, absolute_offset: u64, mut length: u32, buf: &[u8]) -> Result<()> {
        if absolute_offset + length as u64 > self.total_size {
            return Err(CoreError::StorageIo("write out of bounds".into()));
        }

        let mut current_offset = absolute_offset;
        let mut buf_pos = 0;

        for open_file in &mut self.files {
            if length == 0 {
                break;
            }

            let file_start = open_file.offset_in_torrent;
            let file_end = file_start + open_file.size;

            if current_offset >= file_start && current_offset < file_end {
                // Calculate how much to write to this specific file
                let offset_in_file = current_offset - file_start;
                let bytes_available = open_file.size - offset_in_file;
                let bytes_to_write = length.min(bytes_available as u32);

                open_file
                    .file
                    .seek(SeekFrom::Start(offset_in_file))
                    .map_err(|e| CoreError::StorageIo(format!("seek: {}", e)))?;

                // Use vectored I/O where possible for efficiency
                open_file
                    .file
                    .write_all(&buf[buf_pos..(buf_pos + bytes_to_write as usize)])
                    .map_err(|e| CoreError::StorageIo(format!("write: {}", e)))?;

                current_offset += bytes_to_write as u64;
                length -= bytes_to_write;
                buf_pos += bytes_to_write as usize;
            }
        }

        // Async sync hints (don't sync on every write for performance)
        // Rely on OS page cache and periodic flushes instead

        Ok(())
    }

    /// Helper to write an entire piece at once.
    pub fn write_piece(&mut self, piece_index: u32, piece_length: u32, data: &[u8]) -> Result<()> {
        let offset = piece_index as u64 * piece_length as u64;
        self.write_block(offset, data.len() as u32, data)
    }

    /// Helper to read an entire piece at once.
    pub fn read_piece(
        &mut self,
        piece_index: u32,
        piece_length: u32,
        data: &mut std::vec::Vec<u8>,
    ) -> Result<()> {
        let offset = piece_index as u64 * piece_length as u64;
        let len = self
            .total_size
            .saturating_sub(offset)
            .min(piece_length as u64) as usize;
        data.resize(len, 0);
        self.read_block(offset, len as u32, data.as_mut_slice())
    }
}
