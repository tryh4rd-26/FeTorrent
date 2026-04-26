use fetorrent_core::config::FeConfig;
use fetorrent_core::engine::AddMode;
use fetorrent_core::Engine;
use std::sync::Arc;
use tokio;

#[tokio::test]
async fn test_engine_add_torrent_file() {
    let config = FeConfig::default();
    let engine = Engine::new(config);

    // Minimal valid bencoded info dict for testing (Single-file)
    // d4:infod4:name4:test12:piece lengthi16384e6:pieces20:000000000000000000006:lengthi1024eee
    let torrent_bytes = b"d4:infod4:name4:test12:piece lengthi16384e6:pieces20:000000000000000000006:lengthi1024eee";

    let result = engine
        .add_torrent(AddMode::TorrentFile(torrent_bytes.to_vec()), None)
        .await;

    assert!(result.is_ok(), "Failed to add torrent: {:?}", result.err());
    let id = result.unwrap();
    assert_eq!(id, 1);

    let torrents = engine.get_torrents();
    assert_eq!(torrents.len(), 1);
    assert_eq!(torrents[0].name, "test");
}
