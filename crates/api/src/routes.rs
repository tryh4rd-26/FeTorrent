use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use dirs;
use fetorrent_core::engine::AddMode;
use fetorrent_core::{models::*, Engine};
use serde_json::json;
use std::sync::Arc;

pub type AppState = Arc<Engine>;

pub async fn list_torrents(State(engine): State<AppState>) -> impl IntoResponse {
    tracing::debug!("list_torrents: start");
    let torrents = engine.get_torrents();
    tracing::debug!(count = torrents.len(), "list_torrents: finish");
    Json(torrents)
}

pub async fn get_torrent(
    State(engine): State<AppState>,
    Path(id): Path<usize>,
) -> impl IntoResponse {
    match engine.get_torrent(id) {
        Ok(t) => (StatusCode::OK, Json(json!(t))),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Torrent not found"})),
        ),
    }
}

pub async fn add_torrent(
    State(engine): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut magnet = None;
    let mut file_bytes = None;
    let mut custom_dir = None;

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let name = field.name().unwrap_or("").to_string();
        if name == "magnet" {
            let data = field.text().await.unwrap_or_default();
            magnet = Some(data);
        } else if name == "file" {
            let data = field.bytes().await.unwrap_or_default();
            file_bytes = Some(data.to_vec());
        } else if name == "dir" {
            let data = field.text().await.unwrap_or_default();
            if !data.is_empty() {
                // SECURITY: Validate path before using
                match validate_save_path(&data) {
                    Ok(validated_path) => {
                        custom_dir = Some(validated_path.to_string_lossy().to_string());
                    }
                    Err(e) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(json!({"error": format!("Invalid path: {}", e)})),
                        );
                    }
                }
            }
        }
    }

    let result = if let Some(m) = magnet {
        engine.add_torrent(AddMode::Magnet(m), custom_dir).await
    } else if let Some(b) = file_bytes {
        engine
            .add_torrent(AddMode::TorrentFile(b), custom_dir)
            .await
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Provide magnet or file"})),
        );
    };

    match result {
        Ok(id) => (StatusCode::OK, Json(json!({"id": id}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": e.to_string()})),
        ),
    }
}

/// Validate save directory path.
/// Absolute paths and ~-prefixed paths are accepted.
/// Relative paths are resolved under ~/Downloads.
fn validate_save_path(path_str: &str) -> Result<std::path::PathBuf, String> {
    let trimmed = path_str.trim();
    if trimmed.is_empty() {
        return Err("Path cannot be empty".into());
    }

    let path = if let Some(rest) = trimmed.strip_prefix("~/") {
        let home = dirs::home_dir().ok_or_else(|| "Cannot resolve home directory".to_string())?;
        home.join(rest)
    } else {
        std::path::PathBuf::from(trimmed)
    };

    if path.is_absolute() {
        return Ok(path);
    }

    // Reject parent directory traversal
    if path
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return Err("Path traversal not allowed".into());
    }

    // Restrict to user's home/downloads area
    let user_home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let safe_dir = user_home.join("Downloads");
    let resolved = safe_dir.join(&path);

    // Double check: resolved path must still be under safe_dir
    if !resolved.starts_with(&safe_dir) {
        return Err("Invalid path".into());
    }

    Ok(resolved)
}

pub async fn select_directory() -> impl IntoResponse {
    let chosen = tokio::task::spawn_blocking(|| {
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg("POSIX path of (choose folder)")
                .output();

            if let Ok(out) = output {
                if out.status.success() {
                    let path_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !path_str.is_empty() {
                        return Some(std::path::PathBuf::from(path_str));
                    }
                }
            }
            None
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            let start_dir = dirs::download_dir()
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| std::path::PathBuf::from("."));

            rfd::FileDialog::new().set_directory(start_dir).pick_folder()
        }
    })
    .await;

    match chosen {
        Ok(Some(path)) => (StatusCode::OK, Json(json!({ "path": path.to_string_lossy() }))),
        Ok(None) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Directory selection cancelled" })),
        ),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to open directory picker" })),
        ),
    }
}

pub async fn pause_torrent(
    State(engine): State<AppState>,
    Path(id): Path<usize>,
) -> impl IntoResponse {
    match engine.pause_torrent(id).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))),
        Err(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))),
    }
}

pub async fn resume_torrent(
    State(engine): State<AppState>,
    Path(id): Path<usize>,
) -> impl IntoResponse {
    match engine.resume_torrent(id).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))),
        Err(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))),
    }
}

pub async fn remove_torrent(
    State(engine): State<AppState>,
    Path(id): Path<usize>,
) -> impl IntoResponse {
    match engine.remove_torrent(id, false).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))),
        Err(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))),
    }
}

pub async fn get_stats(State(engine): State<AppState>) -> impl IntoResponse {
    tracing::debug!("get_stats: start");
    let stats = engine.get_global_stats();
    tracing::debug!("get_stats: finish");
    Json(stats)
}

pub async fn get_files(State(engine): State<AppState>, Path(id): Path<usize>) -> impl IntoResponse {
    match engine.get_torrent(id) {
        Ok(t) => (StatusCode::OK, Json(json!(t.files))),
        Err(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))),
    }
}

pub async fn get_peers(State(engine): State<AppState>, Path(id): Path<usize>) -> impl IntoResponse {
    match engine.get_torrent(id) {
        // Mocked or collected active peer list goes here. For now returning empty list if ok.
        Ok(_) => (StatusCode::OK, Json(json!(Vec::<PeerInfo>::new()))),
        Err(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))),
    }
}

pub async fn get_settings(State(engine): State<AppState>) -> impl IntoResponse {
    let config = engine.get_config();
    Json(config)
}

pub async fn update_settings(
    State(engine): State<AppState>,
    Json(new_config): Json<fetorrent_core::config::FeConfig>,
) -> impl IntoResponse {
    match engine.update_config(new_config) {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        ),
    }
}
