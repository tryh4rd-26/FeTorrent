//! Core BitTorrent Engine — The top-level orchestrator.
//!
//! Manages torrents, peer connections, trackers, and storage.

use chrono::Utc;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::debug;

use crate::error::{CoreError, Result};
use crate::magnet::MagnetLink;
use crate::models::{GlobalStats, TorrentEvent, TorrentInfo, TorrentStatus};
use crate::storage::Storage;
use crate::torrent::TorrentFile;
use crate::torrent_task::{TorrentCommand, TorrentTask, TorrentTaskParams};

pub struct Engine {
    // Non-blocking concurrent access to torrents
    torrents: Arc<dashmap::DashMap<usize, TorrentHandle>>,
    event_tx: broadcast::Sender<TorrentEvent>,
    next_id: std::sync::atomic::AtomicUsize,

    // Internal signaling
    update_tx: mpsc::Sender<crate::torrent_task::TorrentUpdate>,

    // Config
    config: Arc<std::sync::RwLock<crate::config::FeConfig>>,
}

struct TorrentHandle {
    info: TorrentInfo,
    command_tx: mpsc::Sender<TorrentCommand>,
}

pub enum AddMode {
    Magnet(String),
    TorrentFile(Vec<u8>),
}

impl Engine {
    pub fn new(config: crate::config::FeConfig) -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(100);
        let (update_tx, mut update_rx) = mpsc::channel(1000);

        let engine = Arc::new(Self {
            torrents: Arc::new(dashmap::DashMap::new()),
            event_tx: tx,
            next_id: std::sync::atomic::AtomicUsize::new(1),
            update_tx,
            config: Arc::new(std::sync::RwLock::new(config)),
        });

        // Background loop to process updates from torrent tasks
        let engine_clone = engine.clone();
        tokio::spawn(async move {
            while let Some(update) = update_rx.recv().await {
                let info = {
                    if let Some(mut t) = engine_clone.torrents.get_mut(&update.id) {
                        // Apply metadata updates if they arrived
                        if let Some(size) = update.total_size {
                            t.info.total_size = size;
                        }
                        if let Some(plen) = update.piece_length {
                            t.info.piece_length = plen;
                        }
                        if let Some(npieces) = update.num_pieces {
                            t.info.num_pieces = npieces;
                        }

                        let delta_dl = update.downloaded.saturating_sub(t.info.downloaded);
                        let delta_ul = update.uploaded.saturating_sub(t.info.uploaded);
                        // Exponential smoothing to avoid 0/non-0 jitter in UI/CLI speed display.
                        t.info.dl_speed = t.info.dl_speed.saturating_mul(3).saturating_add(delta_dl) / 4;
                        t.info.ul_speed = t.info.ul_speed.saturating_mul(3).saturating_add(delta_ul) / 4;

                        t.info.downloaded = update.downloaded;
                        t.info.uploaded = update.uploaded;
                        t.info.progress = if t.info.total_size > 0 {
                            update.downloaded as f32 / t.info.total_size as f32
                        } else {
                            0.0
                        };
                        t.info.num_leechers = update.num_leechers;
                        t.info.status = update.status;

                        // Append logs and keep a history (e.g., last 100)
                        if !update.logs.is_empty() {
                            t.info.logs.extend(update.logs);
                            if t.info.logs.len() > 100 {
                                let start = t.info.logs.len() - 100;
                                t.info.logs.drain(0..start);
                            }
                        }

                        let left = t.info.total_size.saturating_sub(t.info.downloaded);
                        t.info.eta_secs = if t.info.dl_speed > 0 {
                            Some(left / t.info.dl_speed)
                        } else {
                            None
                        };

                        if t.info.downloaded > 0 {
                            t.info.ratio = t.info.uploaded as f32 / t.info.downloaded as f32;
                        }
                        Some(t.info.clone())
                    } else {
                        None
                    }
                };

                if let Some(info) = info {
                    // Broadcast update to UI
                    let _ = engine_clone.event_tx.send(TorrentEvent::StatsUpdate {
                        torrents: vec![info],
                        global: engine_clone.get_global_stats(),
                    });
                }
            }
        });

        engine
    }

    /// Subscribe to real-time events (WebSocket can consume this).
    pub fn subscribe(&self) -> broadcast::Receiver<TorrentEvent> {
        self.event_tx.subscribe()
    }

    /// Add a torrent from magnet or .torrent bytes.
    pub async fn add_torrent(
        &self,
        mode: AddMode,
        custom_save_path: Option<String>,
    ) -> Result<usize> {
        let (name, info_hash, magnet_uri, total_size, num_pieces, piece_length, files, trackers) =
            match mode {
                AddMode::Magnet(ref uri) => {
                    let m = MagnetLink::parse(uri)?;
                    (
                        m.name().to_string(),
                        m.info_hash_hex(),
                        Some(uri.clone()),
                        0, // Unknown until metadata downloaded
                        0,
                        0,
                        Vec::new(),
                        m.trackers
                            .into_iter()
                            .map(|u| crate::models::TrackerInfo {
                                url: u,
                                status: "pending".into(),
                                seeders: None,
                                leechers: None,
                                last_announce: None,
                                next_announce: None,
                            })
                            .collect(),
                    )
                }
                AddMode::TorrentFile(ref bytes) => {
                    let t = TorrentFile::from_bytes(bytes)?;
                    (
                        t.name.clone(),
                        t.info_hash_hex(),
                        None,
                        t.total_size,
                        t.num_pieces(),
                        t.piece_length,
                        t.into_file_infos(),
                        t.into_tracker_infos(),
                    )
                }
            };

        // Check if already exists
        if self
            .torrents
            .iter()
            .any(|entry| entry.value().info.info_hash == info_hash)
        {
            return Err(CoreError::TorrentAlreadyExists);
        }

        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let save_path = if let Some(p) = custom_save_path {
            std::path::PathBuf::from(p)
                .join(&name)
                .to_string_lossy()
                .to_string()
        } else {
            let config = self.config.read().unwrap();
            std::path::PathBuf::from(&config.downloads.directory)
                .join(&name)
                .to_string_lossy()
                .to_string()
        };

        let info = TorrentInfo {
            id,
            name,
            info_hash: info_hash.clone(),
            magnet: magnet_uri,
            total_size,
            downloaded: 0,
            uploaded: 0,
            progress: 0.0,
            dl_speed: 0,
            ul_speed: 0,
            eta_secs: None,
            ratio: 0.0,
            status: TorrentStatus::Queued,
            num_peers: 0,
            num_seeds: 0,
            num_leechers: 0,
            num_pieces,
            piece_length,
            files: files.clone(),
            trackers: trackers.clone(),
            logs: Vec::new(),
            save_path: save_path.clone(),
            added_at: Utc::now(),
        };

        let (command_tx, command_rx) = mpsc::channel(100);

        // Broadcast event
        let _ = self.event_tx.send(TorrentEvent::TorrentAdded {
            torrent: info.clone(),
        });

        // Initialize Task components
        let (storage, torrent_file) = match mode {
            AddMode::TorrentFile(ref bytes) => {
                let t = TorrentFile::from_bytes(bytes)?;
                let s = Storage::new(&save_path, &t)?;
                (Some(s), Some(t))
            }
            _ => (None, None),
        };

        let peer_id = generate_peer_id();
        let mut task = TorrentTask::new(
            id,
            info.get_info_hash_bytes()?,
            peer_id,
            TorrentTaskParams {
                tracker_infos: trackers,
                torrent: torrent_file,
                storage,
                save_path: std::path::PathBuf::from(&info.save_path),
                command_rx,
                update_tx: self.update_tx.clone(),
            },
        );

        tokio::spawn(async move {
            task.run().await;
        });

        let handle = TorrentHandle {
            info: info.clone(),
            command_tx,
        };
        self.torrents.insert(id, handle);

        Ok(id)
    }

    pub fn get_torrents(&self) -> Vec<TorrentInfo> {
        debug!("engine.get_torrents: fetching torrents");
        let mut list: Vec<_> = self
            .torrents
            .iter()
            .map(|ref_multi| ref_multi.value().info.clone())
            .collect();
        // Sort by ID to ensure stable ordering
        list.sort_by_key(|t| t.id);
        debug!(count = list.len(), "engine.get_torrents: completed");
        list
    }

    pub fn get_torrent(&self, id: usize) -> Result<TorrentInfo> {
        self.torrents
            .get(&id)
            .map(|t| t.value().info.clone())
            .ok_or(CoreError::TorrentNotFound(id))
    }

    pub async fn pause_torrent(&self, id: usize) -> Result<()> {
        let mut t = self
            .torrents
            .get_mut(&id)
            .ok_or(CoreError::TorrentNotFound(id))?;
        let _ = t.command_tx.send(TorrentCommand::Pause).await;
        t.info.status = TorrentStatus::Paused;
        let info = t.info.clone();
        drop(t); // Release the lock before sending broadcast
        let _ = self
            .event_tx
            .send(TorrentEvent::TorrentUpdated { torrent: info });
        Ok(())
    }

    pub async fn resume_torrent(&self, id: usize) -> Result<()> {
        let mut t = self
            .torrents
            .get_mut(&id)
            .ok_or(CoreError::TorrentNotFound(id))?;
        if t.info.status == TorrentStatus::Paused
            || matches!(t.info.status, TorrentStatus::Error(_))
        {
            let _ = t.command_tx.send(TorrentCommand::Resume).await;
            t.info.status = TorrentStatus::Downloading;
            let info = t.info.clone();
            drop(t); // Release the lock before sending broadcast
            let _ = self
                .event_tx
                .send(TorrentEvent::TorrentUpdated { torrent: info });
        }
        Ok(())
    }

    pub async fn remove_torrent(&self, id: usize, delete_files: bool) -> Result<()> {
        let handle = self
            .torrents
            .remove(&id)
            .ok_or(CoreError::TorrentNotFound(id))?
            .1;
        let _ = handle.command_tx.send(TorrentCommand::Stop).await;
        let _ = self.event_tx.send(TorrentEvent::TorrentRemoved { id });

        if delete_files {
            let path = std::path::PathBuf::from(handle.info.save_path);
            if path.exists() {
                if path.is_dir() {
                    let _ = std::fs::remove_dir_all(path);
                } else {
                    let _ = std::fs::remove_file(path);
                }
            }
        }

        Ok(())
    }

    pub fn get_global_stats(&self) -> GlobalStats {
        let mut s = GlobalStats::default();
        debug!("engine.get_global_stats: computing stats");

        for ref_multi in self.torrents.iter() {
            let t = ref_multi.value();
            let info = &t.info;
            s.dl_speed += info.dl_speed;
            s.ul_speed += info.ul_speed;
            s.total_downloaded += info.downloaded;
            s.total_uploaded += info.uploaded;

            match info.status {
                TorrentStatus::Downloading => s.active_torrents += 1,
                TorrentStatus::Seeding => {
                    s.seeding_torrents += 1;
                    s.active_torrents += 1;
                }
                TorrentStatus::Paused => s.paused_torrents += 1,
                _ => {}
            }
        }
        if s.total_downloaded > 0 {
            s.ratio = s.total_uploaded as f32 / s.total_downloaded as f32;
        }
        debug!(
            torrents = self.torrents.len(),
            "engine.get_global_stats: completed"
        );
        s
    }

    pub fn get_config(&self) -> crate::config::FeConfig {
        self.config.read().unwrap().clone()
    }

    pub fn update_config(&self, new_config: crate::config::FeConfig) -> Result<()> {
        {
            let mut config = self.config.write().unwrap();
            *config = new_config.clone();
        }
        new_config
            .save()
            .map_err(|e| CoreError::Other(format!("Failed to save config: {}", e)))
    }

    pub fn tick_simulate(&self) {
        // Mock removed: background tasks handle real updates now.
    }
}

fn generate_peer_id() -> [u8; 20] {
    use rand::Rng;
    let mut id = [0u8; 20];
    id[0..8].copy_from_slice(b"-FT0001-");
    let mut rng = rand::thread_rng();
    for byte in id.iter_mut().skip(8) {
        *byte = rng.gen_range(33..126); // Readable ASCII
    }
    id
}
