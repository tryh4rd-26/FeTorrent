use crate::error::Result;
use crate::peer::session::{PeerCommand, PeerEvent, PeerSession};
use crate::tracker::TrackerPeer;
use std::collections::{HashSet, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

pub struct PeerManager {
    info_hash: [u8; 20],
    peer_id: [u8; 20],

    // Pool state
    max_connections: usize,
    active_peers: HashSet<SocketAddr>,
    candidate_peers: VecDeque<SocketAddr>,

    // Channels to communicate with TorrentTask
    event_tx: mpsc::Sender<PeerEvent>,
}

impl PeerManager {
    pub fn new(
        info_hash: [u8; 20],
        peer_id: [u8; 20],
        max_connections: usize,
        event_tx: mpsc::Sender<PeerEvent>,
    ) -> Self {
        Self {
            info_hash,
            peer_id,
            max_connections,
            active_peers: HashSet::new(),
            candidate_peers: VecDeque::new(),
            event_tx,
        }
    }

    pub fn add_candidates(&mut self, peers: Vec<TrackerPeer>) {
        for p in peers {
            let addr = p.addr();
            if !self.active_peers.contains(&addr) && !self.candidate_peers.contains(&addr) {
                self.candidate_peers.push_back(addr);
            }
        }
    }

    pub fn remove_peer(&mut self, addr: &SocketAddr) {
        self.active_peers.remove(addr);
    }

    pub async fn tick(&mut self) -> Result<()> {
        // Dial new peers if we have room
        while self.active_peers.len() < self.max_connections {
            if let Some(addr) = self.candidate_peers.pop_front() {
                self.dial_peer(addr).await;
            } else {
                break;
            }
        }
        Ok(())
    }

    async fn dial_peer(&mut self, addr: SocketAddr) {
        tracing::debug!("Dialing peer: {}", addr);
        self.active_peers.insert(addr);

        let info_hash = self.info_hash;
        let peer_id = self.peer_id;
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            let stream = match TcpStream::connect(addr).await {
                Ok(s) => s,
                Err(_) => {
                    let _ = event_tx.send(PeerEvent::Disconnected { addr }).await;
                    return;
                }
            };

            let (cmd_tx, cmd_rx) = mpsc::channel(100);

            // Notify TorrentTask about the new peer before starting the session loop
            let _ = event_tx.send(PeerEvent::NewPeer { addr, tx: cmd_tx }).await;

            if let Err(e) =
                PeerSession::start(addr, stream, info_hash, peer_id, cmd_rx, event_tx.clone()).await
            {
                tracing::debug!("Peer {} session ended: {}", addr, e);
            }

            let _ = event_tx.send(PeerEvent::Disconnected { addr }).await;
        });
    }
}
