use axum::{
    Router,
    routing::{get, post},
};
use bytes::Bytes;
use radio_streamer::{
    config::AppConfig,
    models::CurrentMetadata,
    radio::run_radio_loop,
    routes::{
        handle_admin_genres, handle_admin_history, handle_admin_page, handle_admin_play_now,
        handle_admin_queue, handle_admin_queue_add, handle_admin_queue_reorder,
        handle_admin_search, handle_admin_skip, handle_admin_stats, handle_fallback_404,
        handle_index_or_stream, handle_metadata_json, handle_metadata_sse, handle_metrics,
        handle_stream,
    },
    state::AppState,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

const BROADCAST_CAPACITY: usize = 16;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config = Arc::new(AppConfig::load());
    let music_path = PathBuf::from(&config.station.music_dir);

    if !music_path.exists() {
        warn!(
            "Music directory '{}' does not exist! Creating empty folder...",
            config.station.music_dir
        );
        std::fs::create_dir_all(&music_path).ok();
    }

    let (tx, _) = broadcast::channel::<Bytes>(BROADCAST_CAPACITY);
    let (meta_tx, _) = broadcast::channel::<CurrentMetadata>(100);

    let state = Arc::new(AppState::new(tx, meta_tx, config.clone()));

    // Spawn radio streaming background worker
    let loop_state = Arc::clone(&state);
    let loop_path = music_path.clone();
    tokio::spawn(async move {
        run_radio_loop(loop_state, loop_path).await;
    });

    let app = Router::new()
        .route("/", get(handle_index_or_stream))
        .route("/stream", get(handle_stream))
        .route("/now-playing", get(handle_metadata_json))
        .route("/metrics", get(handle_metrics))
        .route("/admin", get(handle_admin_page))
        .route("/api/metadata", get(handle_metadata_json))
        .route("/api/metadata/sse", get(handle_metadata_sse))
        .route("/api/admin/stats", get(handle_admin_stats))
        .route("/api/admin/skip", post(handle_admin_skip))
        .route("/api/admin/queue", get(handle_admin_queue))
        .route("/api/admin/queue/reorder", post(handle_admin_queue_reorder))
        .route("/api/admin/history", get(handle_admin_history))
        .route("/api/admin/genres", get(handle_admin_genres))
        .route("/api/admin/search", get(handle_admin_search))
        .route("/api/admin/play-now", post(handle_admin_play_now))
        .route("/api/admin/queue/add", post(handle_admin_queue_add))
        .fallback(handle_fallback_404)
        .with_state((*state).clone());

    let addr = format!("{}:{}", config.server.bind_address, config.server.port);
    info!("Radio streamer running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
