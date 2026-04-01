use crate::bencode::{self, BValue};
use crate::models::{TorrentStatus, TrackerInfo};
use crate::peer::manager::PeerManager;
use crate::peer::session::{PeerCommand, PeerEvent};
use crate::pieces::{ActivePiece, PieceManager, PieceState};
use crate::storage::Storage;
use crate::torrent::TorrentFile;
use crate::tracker::http::HttpTracker;
use crate::tracker::udp::UdpTracker;
use crate::tracker::{AnnounceEvent, AnnounceRequest, Tracker};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

#[derive(Debug)]
pub enum TorrentCommand {
    Pause,
    Resume,
    Stop,
}

#[derive(Debug, Clone)]
pub struct TorrentUpdate {
    pub id: usize,
    pub downloaded: u64,
    pub uploaded: u64,
    pub left: u64,
    pub num_peers: usize,
    pub num_seeds: usize,
    pub num_leechers: usize,
    pub status: TorrentStatus,
    
    // Metadata discovered via magnet (initially None)
    pub total_size: Option<u64>,
    pub piece_length: Option<u32>,
    pub num_pieces: Option<u32>,
}

const UT_METADATA_EXT_ID: u8 = 1;

pub struct TorrentTask {
    id: usize,
    info_hash: [u8; 20],
    peer_id: [u8; 20],

    // Core components
    trackers: Vec<Box<dyn Tracker>>,
    storage: Option<Storage>,
    piece_manager: Option<PieceManager>,
    command_rx: mpsc::Receiver<TorrentCommand>,
    update_tx: mpsc::Sender<TorrentUpdate>,

    // Peer management
    peer_manager: PeerManager,
    peer_events_tx: mpsc::Sender<PeerEvent>,
    peer_events_rx: mpsc::Receiver<PeerEvent>,
    active_peers: HashMap<SocketAddr, mpsc::Sender<PeerCommand>>,
    peer_bitfields: HashMap<SocketAddr, Vec<u8>>,
    peer_unchoked: HashSet<SocketAddr>,

    // Metadata exchange (BEP 9)
    metadata_size: Option<usize>,
    metadata_buf: Vec<u8>,
    metadata_pieces: HashSet<usize>,
    ut_metadata_id: HashMap<SocketAddr, u8>,

    // Progress
    downloaded: u64,
    uploaded: u64,
    left: u64,
    swarm_seeds: u32,
    swarm_leechers: u32,
    active_pieces: HashMap<u32, ActivePiece>,
    save_path: std::path::PathBuf,
}

impl TorrentTask {
    pub fn new(
        id: usize,
        info_hash: [u8; 20],
        peer_id: [u8; 20],
        tracker_infos: Vec<TrackerInfo>,
        torrent: Option<&TorrentFile>,
        storage: Option<Storage>,
        save_path: std::path::PathBuf,
        command_rx: mpsc::Receiver<TorrentCommand>,
        update_tx: mpsc::Sender<TorrentUpdate>,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::channel(1000);
        let piece_manager =
            torrent.map(|t| PieceManager::new(t.piece_length, t.total_size, t.pieces.clone()));
        let peer_manager = PeerManager::new(info_hash, peer_id, 300, event_tx.clone());

        let (left, name) = if let Some(t) = torrent {
            (t.total_size, t.name.clone())
        } else {
            (0, "Magnet download...".to_string())
        };

        let mut trackers: Vec<Box<dyn Tracker>> = Vec::new();
        for t in tracker_infos {
            if t.url.starts_with("http") {
                trackers.push(Box::new(HttpTracker::new(&t.url)));
            } else if t.url.starts_with("udp") {
                if let Ok(u) = UdpTracker::from_url(&t.url) {
                    trackers.push(Box::new(u));
                }
            }
        }

        Self {
            id,
            info_hash,
            peer_id,
            trackers,
            storage,
            piece_manager,
            command_rx,
            update_tx,
            peer_manager,
            peer_events_tx: event_tx,
            peer_events_rx: event_rx,
            active_peers: HashMap::new(),
            peer_bitfields: HashMap::new(),
            peer_unchoked: HashSet::new(),
            metadata_size: None,
            metadata_buf: Vec::new(),
            metadata_pieces: HashSet::new(),
            ut_metadata_id: HashMap::new(),
            downloaded: 0,
            uploaded: 0,
            left,
            swarm_seeds: 0,
            swarm_leechers: 0,
            active_pieces: HashMap::new(),
            save_path,
        }
    }

    pub async fn run(&mut self) {
        info!("Torrent task {} started", self.id);

        // Initial announce
        self.announce_all(AnnounceEvent::Started).await;

        let mut announce_interval = tokio::time::interval(Duration::from_secs(1800));
        let mut update_interval = tokio::time::interval(Duration::from_secs(1));
        let mut metadata_interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            tokio::select! {
                _ = update_interval.tick() => {
                    let _ = self.peer_manager.tick().await;

                    let num_seeds = if let Some(pm) = &self.piece_manager {
                        self.peer_bitfields.values()
                            .filter(|bf| pm.is_full_bitfield(bf))
                            .count()
                    } else {
                        0
                    };

                    let num_peers = self.active_peers.len();
                    let num_leechers = num_peers.saturating_sub(num_seeds);

                    let _ = self.update_tx.send(TorrentUpdate {
                        id: self.id,
                        downloaded: self.downloaded,
                        uploaded: self.uploaded,
                        left: self.left,
                        num_peers,
                        num_seeds: num_seeds.max(self.swarm_seeds as usize),
                        num_leechers: num_leechers.max(self.swarm_leechers as usize),
                        status: if self.left == 0 && self.piece_manager.is_some() {
                            TorrentStatus::Seeding
                        } else if self.piece_manager.is_none() {
                            TorrentStatus::DownloadingMetadata
                        } else {
                            TorrentStatus::Downloading
                        },
                        total_size: self.piece_manager.as_ref().map(|pm| pm.total_size()),
                        num_pieces: self.piece_manager.as_ref().map(|pm| pm.num_pieces()),
                        piece_length: self.piece_manager.as_ref().map(|pm| pm.piece_length()),
                    }).await;
                }
                _ = announce_interval.tick() => {
                    self.announce_all(AnnounceEvent::None).await;

                    // If we still have no peers, set a shorter interval for the next attempt
                    if self.active_peers.is_empty() {
                        announce_interval = tokio::time::interval_at(
                            tokio::time::Instant::now() + Duration::from_secs(30),
                            Duration::from_secs(30)
                        );
                    } else {
                        // Reset to standard interval (30 mins) if we found peers
                        announce_interval = tokio::time::interval_at(
                            tokio::time::Instant::now() + Duration::from_secs(1800),
                            Duration::from_secs(1800)
                        );
                    }
                }
                _ = metadata_interval.tick() => {
                    if self.piece_manager.is_none() {
                        // Still in metadata mode, re-request from all capable peers
                        for addr in self.ut_metadata_id.keys().cloned().collect::<Vec<_>>() {
                            self.request_metadata(addr).await;
                        }
                    }
                }
                Some(cmd) = self.command_rx.recv() => {
                    match cmd {
                        TorrentCommand::Stop => break,
                        _ => {}
                    }
                }
                Some(event) = self.peer_events_rx.recv() => {
                    self.handle_peer_event(event).await;
                }
            }
        }

        info!("Torrent task {} stopped", self.id);
    }

    async fn handle_peer_event(&mut self, event: PeerEvent) {
        match event {
            PeerEvent::NewPeer { addr, tx } => {
                debug!("New peer connected: {}", addr);
                self.active_peers.insert(addr, tx.clone());

                // Send Extended Handshake (BEP 10)
                let mut m = BTreeMap::new();
                m.insert(b"ut_metadata".to_vec(), BValue::Int(UT_METADATA_EXT_ID as i64));
                let mut handshake = BTreeMap::new();
                handshake.insert(b"m".to_vec(), BValue::Dict(m));

                let payload = bencode::encode(&BValue::Dict(handshake));
                let _ = tx
                    .send(PeerCommand::SendExtended { msg_id: 0, payload })
                    .await;

                let _ = tx.send(PeerCommand::Interested).await;
            }
            PeerEvent::Disconnected { addr } => {
                debug!("Peer disconnected: {}", addr);
                self.active_peers.remove(&addr);
                self.peer_bitfields.remove(&addr);
                self.peer_unchoked.remove(&addr);
                self.peer_manager.remove_peer(&addr);
            }
            PeerEvent::BitfieldReceived { addr, bitfield } => {
                debug!("Received bitfield from {} (len {})", addr, bitfield.len());
                self.peer_bitfields.insert(addr, bitfield);
                self.pick_and_request(addr).await;
            }
            PeerEvent::HaveReceived { addr, piece_index } => {
                if let Some(bf) = self.peer_bitfields.get_mut(&addr) {
                    let byte_idx = (piece_index / 8) as usize;
                    let bit_idx = 7 - (piece_index % 8);
                    if byte_idx < bf.len() {
                        bf[byte_idx] |= 1 << bit_idx;
                    }
                }
                self.pick_and_request(addr).await;
            }
            PeerEvent::Unchoked { addr } => {
                debug!("Peer unchoked us: {}", addr);
                self.peer_unchoked.insert(addr);
                self.pick_and_request(addr).await;
            }
            PeerEvent::Choked { addr } => {
                debug!("Peer choked us: {}", addr);
                self.peer_unchoked.remove(&addr);
            }
            PeerEvent::BlockReceived {
                index,
                begin,
                block,
                ..
            } => {
                if let Some(active) = self.active_pieces.get_mut(&index) {
                    if active.add_block(begin, &block) {
                        self.downloaded += block.len() as u64;
                        if active.is_complete() {
                            let piece_data = active.data.clone();
                            if let (Some(pm), Some(storage)) =
                                (&mut self.piece_manager, &mut self.storage)
                            {
                                if pm.verify_hash(index, &piece_data) {
                                    info!("Piece {} verified!", index);
                                    if let Err(e) = storage.write_piece(
                                        index,
                                        pm.get_piece_length(index),
                                        &piece_data,
                                    ) {
                                        error!("Failed to write piece {} to storage: {}", index, e);
                                    } else {
                                        pm.states[index as usize] = PieceState::Verified;
                                        pm.set_bit(index);
                                        self.left -= piece_data.len() as u64;
                                        self.active_pieces.remove(&index);

                                        // Broadcast HAVE to all active peers
                                        for tx in self.active_peers.values() {
                                            let _ = tx.send(PeerCommand::SendHave { index }).await;
                                        }
                                    }
                                } else {
                                    error!("Piece {} hash mismatch, retrying...", index);
                                    pm.states[index as usize] = PieceState::Missing;
                                    self.active_pieces.remove(&index);
                                }
                            }
                        }
                    }
                }
            }
            PeerEvent::ExtendedHandshakeReceived { addr, p } => {
                if let Ok(BValue::Dict(d)) = bencode::decode(&p) {
                    if let Some(BValue::Dict(m)) = d.get(b"m".as_ref()) {
                        if let Some(BValue::Int(id)) = m.get(b"ut_metadata".as_ref()) {
                            debug!("Peer {} supports ut_metadata with id {}", addr, id);
                            self.ut_metadata_id.insert(addr, *id as u8);

                            if let Some(BValue::Int(size)) = d.get(b"metadata_size".as_ref()) {
                                let size = *size as usize;
                                if self.metadata_size.is_none() && size > 0 {
                                    self.metadata_size = Some(size);
                                    self.metadata_buf = vec![0u8; size];
                                    let num_pieces = (size + 16383) / 16384;
                                    info!("Metadata size discovered: {} bytes ({} pieces)", size, num_pieces);
                                }
                            }

                            self.request_metadata(addr).await;
                        }
                    }
                }
            }
            PeerEvent::ExtendedMessageReceived { addr, msg_id, p } => {
                if msg_id == UT_METADATA_EXT_ID {
                    self.handle_metadata_message(addr, p).await;
                }
            }
            _ => {}
        }
    }

    async fn pick_and_request(&mut self, addr: SocketAddr) {
        if !self.peer_unchoked.contains(&addr) {
            return;
        }

        let mut piece_index = None;
        for (&idx, active) in &self.active_pieces {
            if let Some(bf) = self.peer_bitfields.get(&addr) {
                let byte_idx = (idx / 8) as usize;
                let bit_idx = 7 - (idx % 8);
                if byte_idx < bf.len() && (bf[byte_idx] & (1 << bit_idx)) != 0 {
                    if active.blocks_received < active.total_blocks {
                        piece_index = Some(idx);
                        break;
                    }
                }
            }
        }

        if piece_index.is_none() {
            if let (Some(pm), Some(bf)) = (&mut self.piece_manager, self.peer_bitfields.get(&addr))
            {
                if let Some(idx) = pm.pick_next_piece(bf) {
                    let piece_len = pm.get_piece_length(idx);
                    self.active_pieces
                        .insert(idx, ActivePiece::new(idx, piece_len));
                    pm.states[idx as usize] = PieceState::Downloading;
                    piece_index = Some(idx);
                }
            }
        }

        if let Some(idx) = piece_index {
            if let (Some(active), Some(pm)) = (self.active_pieces.get(&idx), &self.piece_manager) {
                if let Some(tx) = self.active_peers.get(&addr) {
                    // Increased from 5 to 16 for aggressive pipelining
                    let missing = active.get_missing_blocks(pm, 16);
                    for (begin, length) in missing {
                        let _ = tx
                            .send(PeerCommand::Request {
                                index: idx,
                                begin,
                                length,
                            })
                            .await;
                    }
                }
            }
        }
    }

    async fn request_metadata(&mut self, addr: SocketAddr) {
        if self.piece_manager.is_some() {
            return;
        }

        let ext_id = match self.ut_metadata_id.get(&addr) {
            Some(&id) => id,
            None => return,
        };

        let tx = match self.active_peers.get(&addr) {
            Some(tx) => tx,
            None => return,
        };

        if let Some(size) = self.metadata_size {
            let num_pieces = (size + 16383) / 16384;
            for p in 0..num_pieces {
                if !self.metadata_pieces.contains(&p) {
                    let mut req = BTreeMap::new();
                    req.insert(b"msg_type".to_vec(), BValue::Int(0)); // request
                    req.insert(b"piece".to_vec(), BValue::Int(p as i64));
                    let payload = bencode::encode(&BValue::Dict(req));
                    let _ = tx.send(PeerCommand::SendExtended { msg_id: ext_id, payload }).await;
                }
            }
        } else {
            // Request piece 0 anyway to try and get size in response
            let mut req = BTreeMap::new();
            req.insert(b"msg_type".to_vec(), BValue::Int(0));
            req.insert(b"piece".to_vec(), BValue::Int(0));
            let payload = bencode::encode(&BValue::Dict(req));
            let _ = tx.send(PeerCommand::SendExtended { msg_id: ext_id, payload }).await;
        }
    }

    async fn handle_metadata_message(&mut self, addr: SocketAddr, payload: Vec<u8>) {
        if self.piece_manager.is_some() {
            return;
        }

        let mut decoder = bencode::Decoder::new(&payload);
        if let Ok(BValue::Dict(d)) = decoder.decode() {
            let msg_type = d.get(b"msg_type".as_ref()).and_then(|v| v.as_int());
            let piece = d.get(b"piece".as_ref()).and_then(|v| v.as_int());
            let total_size = d.get(b"total_size".as_ref()).and_then(|v| v.as_int());

            if msg_type == Some(1) { // data
                if let Some(p_idx) = piece {
                    let p_idx = p_idx as usize;
                    if self.metadata_pieces.contains(&p_idx) { return; }

                    let data = &payload[decoder.position()..];
                    
                    if let Some(size) = self.metadata_size {
                         let offset = p_idx * 16384;
                         if offset + data.len() <= size {
                             self.metadata_buf[offset..offset+data.len()].copy_from_slice(data);
                             self.metadata_pieces.insert(p_idx);
                             info!("Received metadata piece {}/{} from {} (len {})", p_idx + 1, self.metadata_size.map(|s| (s+16383)/16384).unwrap_or(0), addr, data.len());
                         }
                    } else if let Some(tsize) = total_size {
                         let tsize = tsize as usize;
                         self.metadata_size = Some(tsize);
                         self.metadata_buf = vec![0u8; tsize];
                         self.metadata_buf[0..data.len()].copy_from_slice(data);
                         self.metadata_pieces.insert(p_idx);
                         info!("Metadata size discovered from message: {} bytes", tsize);
                    }

                    // Check if complete
                    if let Some(size) = self.metadata_size {
                        let num_pieces = (size + 16383) / 16384;
                        if self.metadata_pieces.len() == num_pieces {
                            info!("All metadata pieces received. Parsing...");
                            if let Ok(mut t) = TorrentFile::from_info_dict(&self.metadata_buf) {
                                info!("Successfully parsed metadata for torrent: {}", t.name);
                                let pm = PieceManager::new(t.piece_length, t.total_size, t.pieces.clone());
                                if let Ok(storage) = Storage::new(&self.save_path, &t) {
                                    self.piece_manager = Some(pm);
                                    self.storage = Some(storage);
                                    self.left = t.total_size;
                                    self.announce_all(AnnounceEvent::None).await;
                                }
                            } else if let Err(e) = TorrentFile::from_info_dict(&self.metadata_buf) {
                                error!("Failed to parse assembled metadata: {}! Clearing and retrying...", e);
                                self.metadata_pieces.clear();
                            }
                        }
                    }
                }
            }
        }
    }

    async fn announce_all(&mut self, event: AnnounceEvent) {
        use futures::future::join_all;

        // Announce to all trackers in parallel for faster peer discovery
        let handles: Vec<_> = self.trackers.iter_mut().map(|tracker| {
            let req = AnnounceRequest {
                info_hash: self.info_hash,
                peer_id: self.peer_id,
                port: 6881,
                uploaded: self.uploaded,
                downloaded: self.downloaded,
                left: self.left,
                event: event.clone(),
                num_want: 200,  // Request more peers initially for faster connection
            };
            tracker.announce(req)
        }).collect();

        let results = join_all(handles).await;

        for result in results {
            match result {
                Ok(resp) => {
                    info!(
                        "Tracker returned {} peers (Seeds: {}, Leechers: {})",
                        resp.peers.len(),
                        resp.seeders,
                        resp.leechers
                    );
                    self.swarm_seeds = self.swarm_seeds.max(resp.seeders);
                    self.swarm_leechers = self.swarm_leechers.max(resp.leechers);
                    self.peer_manager.add_candidates(resp.peers);
                }
                Err(e) => {
                    error!("Tracker failed: {}", e);
                }
            }
        }
    }
}
