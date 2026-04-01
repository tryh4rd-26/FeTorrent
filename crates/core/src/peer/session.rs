//! Asynchronous per-peer session manager.

use futures::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::codec::Framed;

use super::handshake::Handshake;
use super::message::{Message, MessageCodec, MAX_FRAME_SIZE};
use crate::error::{CoreError, Result};

pub const BLOCK_SIZE: u32 = 262144; // 256 KiB - significantly faster than 16KiB
pub const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(60);
pub const MAX_INFLIGHT_REQUESTS: usize = 16; // Pipeline requests for better throughput

/// Events sent from a PeerSession back to the Engine.
#[derive(Debug)]
pub enum PeerEvent {
    HandshakeOk { addr: std::net::SocketAddr },
    BitfieldReceived { addr: std::net::SocketAddr, bitfield: Vec<u8> },
    HaveReceived { addr: std::net::SocketAddr, piece_index: u32 },
    BlockReceived { addr: std::net::SocketAddr, index: u32, begin: u32, block: Vec<u8> },
    Choked { addr: std::net::SocketAddr },
    Unchoked { addr: std::net::SocketAddr },
    Interested { addr: std::net::SocketAddr },
    NotInterested { addr: std::net::SocketAddr },
    RequestReceived { addr: std::net::SocketAddr, index: u32, begin: u32, length: u32 },
    Disconnected { addr: std::net::SocketAddr },
    NewPeer { addr: std::net::SocketAddr, tx: mpsc::Sender<PeerCommand> },
    ExtendedHandshakeReceived { addr: std::net::SocketAddr, p: Vec<u8> },
    ExtendedMessageReceived { addr: std::net::SocketAddr, msg_id: u8, p: Vec<u8> },
}

/// Commands sent from the Engine to a PeerSession.
#[derive(Debug, Clone)]
pub enum PeerCommand {
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Request { index: u32, begin: u32, length: u32 },
    Cancel { index: u32, begin: u32, length: u32 },
    SendPiece { index: u32, begin: u32, block: Vec<u8> },
    SendHave { index: u32 },
    SendBitfield(Vec<u8>),
    SendExtended { msg_id: u8, payload: Vec<u8> },
    Disconnect,
}

#[derive(Debug, Default)]
pub struct PeerState {
    pub am_choking: bool,
    pub am_interested: bool,
    pub peer_choking: bool,
    pub peer_interested: bool,
}

impl PeerState {
    pub fn new() -> Self {
        Self {
            am_choking: true,
            am_interested: false,
            peer_choking: true,
            peer_interested: false,
        }
    }
}

pub struct PeerSession {
    addr: std::net::SocketAddr,
    stream: Framed<TcpStream, MessageCodec>,
    own_id: [u8; 20],
    info_hash: [u8; 20],
    state: PeerState,
    
    // Engine -> PeerSession
    command_rx: mpsc::Receiver<PeerCommand>,
    // PeerSession -> Engine
    event_tx: mpsc::Sender<PeerEvent>,
}

impl PeerSession {
    pub async fn start(
        addr: std::net::SocketAddr,
        mut socket: TcpStream,
        info_hash: [u8; 20],
        own_id: [u8; 20],
        command_rx: mpsc::Receiver<PeerCommand>,
        event_tx: mpsc::Sender<PeerEvent>,
    ) -> Result<()> {
        let handshake = Handshake::new(info_hash, own_id);
        
        let hs_result = timeout(Duration::from_secs(10), Handshake::exchange(&mut socket, &handshake)).await
             .map_err(|_| CoreError::PeerHandshake("Timeout during handshake".into()))??;

        let stream = Framed::new(socket, MessageCodec::new());

        let mut session = Self {
            addr,
            stream,
            own_id,
            info_hash,
            state: PeerState::new(),
            command_rx,
            event_tx,
        };

        let _ = session.event_tx.send(PeerEvent::HandshakeOk { addr }).await;
        
        // Loop runs until an error occurs or the connection drops
        session.run_loop().await
    }

    async fn run_loop(&mut self) -> Result<()> {
        let mut keepalive_timer = tokio::time::interval(KEEP_ALIVE_INTERVAL);
        keepalive_timer.tick().await; // skip immediate tick

        loop {
            tokio::select! {
                _ = keepalive_timer.tick() => {
                    self.send_message(Message::KeepAlive).await?;
                }

                cmd = self.command_rx.recv() => {
                    if let Some(cmd) = cmd {
                        self.handle_command(cmd).await?;
                    } else {
                        // Manager dropped command tx, shutdown.
                        break;
                    }
                }

                msg = self.stream.next() => {
                    match msg {
                        Some(Ok(msg)) => self.handle_message(msg).await?,
                        Some(Err(e)) => return Err(CoreError::PeerProtocol(format!("Decode err: {}", e))),
                        None => { return Err(CoreError::PeerDisconnected); }
                    }
                }
            }
        }
        Ok(())
    }

    async fn handle_command(&mut self, cmd: PeerCommand) -> Result<()> {
        match cmd {
            PeerCommand::Choke => {
                self.state.am_choking = true;
                self.send_message(Message::Choke).await?;
            }
            PeerCommand::Unchoke => {
                self.state.am_choking = false;
                self.send_message(Message::Unchoke).await?;
            }
            PeerCommand::Interested => {
                self.state.am_interested = true;
                self.send_message(Message::Interested).await?;
            }
            PeerCommand::NotInterested => {
                self.state.am_interested = false;
                self.send_message(Message::NotInterested).await?;
            }
            PeerCommand::Request { index, begin, length } => {
                self.send_message(Message::Request { index, begin, length }).await?;
            }
            PeerCommand::Cancel { index, begin, length } => {
                self.send_message(Message::Cancel { index, begin, length }).await?;
            }
            PeerCommand::SendPiece { index, begin, block } => {
                self.send_message(Message::Piece { index, begin, block }).await?;
            }
            PeerCommand::SendHave { index } => {
                self.send_message(Message::Have { piece_index: index }).await?;
            }
            PeerCommand::SendBitfield(bitfield) => {
                self.send_message(Message::Bitfield(bitfield)).await?;
            }
            PeerCommand::SendExtended { msg_id, payload } => {
                self.send_message(Message::Extended { msg_id, payload }).await?;
            }
            PeerCommand::Disconnect => {
                return Err(CoreError::PeerDisconnected);
            }
        }
        Ok(())
    }

    async fn handle_message(&mut self, msg: Message) -> Result<()> {
        let addr = self.addr;
        match msg {
            Message::KeepAlive => {}
            Message::Choke => {
                self.state.peer_choking = true;
                let _ = self.event_tx.send(PeerEvent::Choked { addr }).await;
            }
            Message::Unchoke => {
                self.state.peer_choking = false;
                let _ = self.event_tx.send(PeerEvent::Unchoked { addr }).await;
            }
            Message::Interested => {
                self.state.peer_interested = true;
                let _ = self.event_tx.send(PeerEvent::Interested { addr }).await;
            }
            Message::NotInterested => {
                self.state.peer_interested = false;
                let _ = self.event_tx.send(PeerEvent::NotInterested { addr }).await;
            }
            Message::Have { piece_index } => {
                let _ = self.event_tx.send(PeerEvent::HaveReceived { addr, piece_index }).await;
            }
            Message::Bitfield(bitfield) => {
                let _ = self.event_tx.send(PeerEvent::BitfieldReceived { addr, bitfield }).await;
            }
            Message::Request { index, begin, length } => {
                if !self.state.am_choking && length > 0 && length <= MAX_FRAME_SIZE as u32 {
                     let _ = self.event_tx.send(PeerEvent::RequestReceived { addr, index, begin, length }).await;
                }
            }
            Message::Piece { index, begin, block } => {
                let _ = self.event_tx.send(PeerEvent::BlockReceived { addr, index, begin, block }).await;
            }
            Message::Cancel { index, begin, length } => {
                // Not tightly implemented yet, usually engine tracks pending requests
            }
            Message::Port { listen_port } => {
                // Usually ignored unless we implement DHT routing table injection
                tracing::debug!("Peer suggested DHT port {}", listen_port);
            }
            Message::Extended { msg_id, payload } => {
                if msg_id == 0 {
                    let _ = self.event_tx.send(PeerEvent::ExtendedHandshakeReceived { addr, p: payload }).await;
                } else {
                    let _ = self.event_tx.send(PeerEvent::ExtendedMessageReceived { addr, msg_id, p: payload }).await;
                }
            }
        }
        Ok(())
    }

    async fn send_message(&mut self, msg: Message) -> Result<()> {
        self.stream.send(msg).await.map_err(|e| CoreError::PeerProtocol(format!("Send failed: {}", e)))
    }
}
