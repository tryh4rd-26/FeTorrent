use axum::{
    routing::{get, post, delete},
    Router,
};
use tower_http::{
    cors::CorsLayer,
    services::ServeDir,
    trace::TraceLayer,
};
use std::net::SocketAddr;
use std::sync::Arc;
use fetorrent_core::Engine;

use crate::routes::*;
use crate::ws::ws_handler;

pub async fn start_server(
    engine: Arc<Engine>,
    bind: &str,
    port: u16,
    ui_dir: &std::path::Path,
) -> anyhow::Result<()> {
    // Restrict CORS to localhost (or specify exact origin in production)
    let cors = CorsLayer::permissive()
        .allow_origin(axum::http::HeaderValue::from_static("http://localhost:3000"))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers(vec![axum::http::header::CONTENT_TYPE]);
        
    let api_routes = Router::new()
        .route("/torrents", get(list_torrents))
        .route("/torrents/add", post(add_torrent))
        .route("/torrents/:id", get(get_torrent))
        .route("/torrents/:id/pause", post(pause_torrent))
        .route("/torrents/:id/resume", post(resume_torrent))
        .route("/torrents/:id", delete(remove_torrent))
        .route("/torrents/:id/files", get(get_files))
        .route("/torrents/:id/peers", get(get_peers))
        .route("/stats", get(get_stats))
        .route("/settings", get(get_settings).post(update_settings))
        .route("/ws", get(ws_handler))
        .with_state(engine);

    // Mount UI, fallback to index.html for SPA routing
    let ui_service = ServeDir::new(ui_dir);

    let app = Router::new()
        .nest("/api/v1", api_routes)
        .fallback_service(ui_service)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = format!("{}:{}", bind, port).parse()?;
    tracing::info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
