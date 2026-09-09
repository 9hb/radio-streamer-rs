use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use bytes::Bytes;
use id3::{Tag, TagLike};
use rand::prelude::IndexedRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use symphonia::core::codecs::CODEC_TYPE_NULL;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::{broadcast, Notify, RwLock};
use tokio::time::sleep;
use tracing::{error, info, warn};
use walkdir::WalkDir;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackInfo {
    pub id: usize,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub duration_secs: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CurrentMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub duration_secs: u64,
    pub elapsed_secs: u64,
    pub listeners: usize,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

#[derive(Deserialize)]
pub struct PlayNowReq {
    pub id: usize,
}

#[derive(Deserialize)]
pub struct ReorderReq {
    pub from: usize,
    pub to: usize,
}

#[derive(Clone)]
pub struct AppState {
    pub broadcast_tx: broadcast::Sender<Bytes>,
    pub metadata_tx: broadcast::Sender<CurrentMetadata>,
    pub current_meta: Arc<RwLock<CurrentMetadata>>,
    pub listener_count: Arc<std::sync::atomic::AtomicUsize>,
    pub skip_notify: Arc<Notify>,
    pub queue: Arc<RwLock<Vec<TrackInfo>>>,
    pub next_priority_track: Arc<RwLock<Option<TrackInfo>>>,
    pub all_tracks: Arc<RwLock<Vec<TrackInfo>>>,
}

const BROADCAST_CAPACITY: usize = 4;
const BUFFER_CHUNK_SIZE: usize = 1024;
const MAX_HISTORY: usize = 250;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let music_dir = std::env::var("MUSIC_DIR").unwrap_or_else(|_| "./music".to_string());
    let music_path = PathBuf::from(&music_dir);

    if !music_path.exists() {
        warn!(
            "Music directory '{}' does not exist! Creating empty folder...",
            music_dir
        );
        std::fs::create_dir_all(&music_path).ok();
    }

    let tracks = scan_music_dir(&music_path);
    info!("Scanned {} tracks from '{}'", tracks.len(), music_dir);

    let (broadcast_tx, _) = broadcast::channel::<Bytes>(BROADCAST_CAPACITY);
    let (metadata_tx, _) = broadcast::channel::<CurrentMetadata>(100);

    let initial_meta = CurrentMetadata {
        title: "Radio Starting...".to_string(),
        artist: "01337000 Radio".to_string(),
        album: "".to_string(),
        genre: "".to_string(),
        duration_secs: 0,
        elapsed_secs: 0,
        listeners: 0,
    };
    let current_meta = Arc::new(RwLock::new(initial_meta));
    let listener_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let skip_notify = Arc::new(Notify::new());
    let queue = Arc::new(RwLock::new(Vec::new()));
    let next_priority_track = Arc::new(RwLock::new(None));
    let all_tracks = Arc::new(RwLock::new(tracks.clone()));

    let state = AppState {
        broadcast_tx: broadcast_tx.clone(),
        metadata_tx: metadata_tx.clone(),
        current_meta: current_meta.clone(),
        listener_count: listener_count.clone(),
        skip_notify: skip_notify.clone(),
        queue: queue.clone(),
        next_priority_track: next_priority_track.clone(),
        all_tracks: all_tracks.clone(),
    };

    let b_tx = broadcast_tx.clone();
    let m_tx = metadata_tx.clone();
    let c_meta = current_meta.clone();
    let s_notify = skip_notify.clone();
    let q_lock = queue.clone();
    let p_track = next_priority_track.clone();
    tokio::spawn(async move {
        radio_loop(tracks, music_path, b_tx, m_tx, c_meta, s_notify, q_lock, p_track).await;
    });

    let app = Router::new()
        .route("/", get(handle_index_or_stream))
        .route("/stream", get(handle_stream))
        .route("/admin", get(handle_admin_page))
        .route("/api/metadata", get(handle_metadata_json))
        .route("/api/metadata/sse", get(handle_metadata_sse))
        .route("/now-playing", get(handle_metadata_json))
        .route("/api/admin/skip", post(handle_admin_skip))
        .route("/api/admin/queue", get(handle_admin_queue))
        .route("/api/admin/queue/reorder", post(handle_admin_queue_reorder))
        .route("/api/admin/search", get(handle_admin_search))
        .route("/api/admin/play-now", post(handle_admin_play_now))
        .route("/api/admin/queue/add", post(handle_admin_queue_add))
        .fallback(handle_fallback_404)
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "9191".to_string())
        .parse()
        .unwrap_or(9191);

    let addr = format!("0.0.0.0:{}", port);
    info!("Radio streamer running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn get_exact_duration_secs(path: &Path) -> u64 {
    if let Ok(file) = File::open(path) {
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            hint.with_extension(ext);
        }

        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();

        if let Ok(probed) = symphonia::default::get_probe().format(&hint, mss, &fmt_opts, &meta_opts) {
            let format = probed.format;
            if let Some(track) = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL) {
                let tb = track.codec_params.time_base;
                let n_frames = track.codec_params.n_frames;
                if let (Some(tb), Some(frames)) = (tb, n_frames) {
                    let time = tb.calc_time(frames);
                    if time.seconds > 0 {
                        return time.seconds;
                    }
                }
            }
        }
    }
    0
}

fn scan_music_dir(dir: &Path) -> Vec<TrackInfo> {
    let mut tracks = Vec::new();
    let mut id_counter = 0;

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let ext_lower = ext.to_lowercase();
                if ext_lower == "mp3" || ext_lower == "wav" {
                    let mut title = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Unknown Track".to_string());
                    let mut artist = "Unknown Artist".to_string();
                    let mut album = "Unknown Album".to_string();
                    let mut genre = "Unknown Genre".to_string();

                    let duration_secs = get_exact_duration_secs(path);

                    if ext_lower == "mp3" {
                        if let Ok(tag) = Tag::read_from_path(path) {
                            if let Some(t) = tag.title() {
                                title = t.to_string();
                            }
                            if let Some(a) = tag.artist() {
                                artist = a.to_string();
                            }
                            if let Some(alb) = tag.album() {
                                album = alb.to_string();
                            }
                            if let Some(g) = tag.genre() {
                                genre = g.to_string();
                            }
                        }
                    }

                    id_counter += 1;
                    tracks.push(TrackInfo {
                        id: id_counter,
                        path: path.to_path_buf(),
                        title,
                        artist,
                        album,
                        genre,
                        duration_secs,
                    });
                }
            }
        }
    }
    tracks
}

async fn radio_loop(
    mut tracks: Vec<TrackInfo>,
    music_path: PathBuf,
    tx: broadcast::Sender<Bytes>,
    meta_tx: broadcast::Sender<CurrentMetadata>,
    current_meta: Arc<RwLock<CurrentMetadata>>,
    skip_notify: Arc<Notify>,
    queue: Arc<RwLock<Vec<TrackInfo>>>,
    next_priority_track: Arc<RwLock<Option<TrackInfo>>>,
) {
    let mut history: VecDeque<usize> = VecDeque::with_capacity(MAX_HISTORY + 1);
    let mut current_genre: Option<String> = None;
    let mut cluster_remaining = 0;

    loop {
        if tracks.is_empty() {
            tracks = scan_music_dir(&music_path);
            if tracks.is_empty() {
                warn!("No MP3/WAV tracks found in music directory. Retrying in 5 seconds...");
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        }

        let track = if let Some(priority) = next_priority_track.write().await.take() {
            priority
        } else {
            let mut q = queue.write().await;
            if !q.is_empty() {
                q.remove(0)
            } else {
                let selected_index = select_next_track(
                    &tracks,
                    &history,
                    &mut current_genre,
                    &mut cluster_remaining,
                );
                tracks[selected_index].clone()
            }
        };

        history.push_back(track.id);
        if history.len() > MAX_HISTORY {
            history.pop_front();
        }

        {
            let mut q = queue.write().await;
            if q.len() < 10 {
                let mut temp_history = history.clone();
                let mut temp_genre = current_genre.clone();
                let mut temp_cluster = cluster_remaining;

                while q.len() < 10 {
                    let next_idx = select_next_track(
                        &tracks,
                        &temp_history,
                        &mut temp_genre,
                        &mut temp_cluster,
                    );
                    let next_track = tracks[next_idx].clone();
                    temp_history.push_back(next_track.id);
                    q.push(next_track);
                }
            }
        }

        info!(
            "Now playing [{}/{}]: '{}' by '{}' (Genre: '{}', Dur: {}s)",
            track.id,
            tracks.len(),
            track.title,
            track.artist,
            track.genre,
            track.duration_secs
        );

        let mmap = match File::open(&track.path).and_then(|f| unsafe { memmap2::MmapOptions::new().map(&f) }) {
            Ok(m) => Arc::new(m),
            Err(e) => {
                error!("Failed to mmap track file {:?}: {}", track.path, e);
                sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let file_len = mmap.len() as u64;
        let track_dur = if track.duration_secs > 0 {
            track.duration_secs
        } else {
            (file_len * 8) / (320 * 1000)
        };

        let bytes_per_sec = if track_dur > 0 {
            file_len as f64 / track_dur as f64
        } else {
            40000.0
        };

        let track_start_instant = Instant::now();

        {
            let mut meta = current_meta.write().await;
            meta.title = track.title.clone();
            meta.artist = track.artist.clone();
            meta.album = track.album.clone();
            meta.genre = track.genre.clone();
            meta.duration_secs = track_dur;
            meta.elapsed_secs = 0;

            let _ = meta_tx.send(meta.clone());
        }

        let mut offset = 0usize;
        let mut total_bytes_sent = 0u64;

        loop {
            if offset >= mmap.len() {
                let actual_real_secs = track_start_instant.elapsed().as_secs_f64();
                let target_secs = track_dur as f64;
                if target_secs > actual_real_secs {
                    let remaining = target_secs - actual_real_secs;
                    sleep(Duration::from_secs_f64(remaining)).await;
                }
                break;
            }

            tokio::select! {
                _ = skip_notify.notified() => {
                    info!("Track skip requested via admin panel!");
                    break;
                }
                _ = tokio::task::yield_now() => {}
            }

            let chunk_end = (offset + BUFFER_CHUNK_SIZE).min(mmap.len());
            let chunk_bytes = Bytes::copy_from_slice(&mmap[offset..chunk_end]);
            let n = chunk_end - offset;
            offset = chunk_end;
            total_bytes_sent += n as u64;

            let _ = tx.send(chunk_bytes);

            let elapsed_real_secs = track_start_instant.elapsed().as_secs();
            let current_elapsed_secs = elapsed_real_secs.min(track_dur);
            {
                let mut meta = current_meta.write().await;
                if meta.elapsed_secs != current_elapsed_secs {
                    meta.elapsed_secs = current_elapsed_secs;
                    let _ = meta_tx.send(meta.clone());
                }
            }

            let expected_real_secs = total_bytes_sent as f64 / bytes_per_sec;
            let actual_real_secs = track_start_instant.elapsed().as_secs_f64();

            if expected_real_secs > actual_real_secs {
                let sleep_secs = expected_real_secs - actual_real_secs;
                sleep(Duration::from_secs_f64(sleep_secs)).await;
            }
        }
    }
}

fn select_next_track(
    tracks: &[TrackInfo],
    history: &VecDeque<usize>,
    current_genre: &mut Option<String>,
    cluster_remaining: &mut usize,
) -> usize {
    let mut rng = rand::rng();

    let history_set: std::collections::HashSet<usize> = history.iter().cloned().collect();

    let mut eligible: Vec<usize> = (0..tracks.len())
        .filter(|&idx| !history_set.contains(&tracks[idx].id))
        .collect();

    if eligible.is_empty() {
        let recent_recent: std::collections::HashSet<usize> =
            history.iter().rev().take(history.len() / 2).cloned().collect();
        eligible = (0..tracks.len())
            .filter(|&idx| !recent_recent.contains(&tracks[idx].id))
            .collect();

        if eligible.is_empty() {
            eligible = (0..tracks.len()).collect();
        }
    }

    if *cluster_remaining == 0 || current_genre.is_none() {
        let chosen_idx = *eligible.choose(&mut rng).unwrap_or(&0);
        *current_genre = Some(tracks[chosen_idx].genre.clone());
        *cluster_remaining = rng.random_range(3..=5);
        return chosen_idx;
    } else {
        let genre_name = current_genre.as_ref().unwrap();
        let same_genre_candidates: Vec<usize> = eligible
            .iter()
            .cloned()
            .filter(|&idx| &tracks[idx].genre == genre_name && tracks[idx].genre != "Unknown Genre")
            .collect();

        if !same_genre_candidates.is_empty() {
            *cluster_remaining -= 1;
            return *same_genre_candidates.choose(&mut rng).unwrap();
        } else {
            let chosen_idx = *eligible.choose(&mut rng).unwrap_or(&0);
            *current_genre = Some(tracks[chosen_idx].genre.clone());
            *cluster_remaining = rng.random_range(3..=5);
            return chosen_idx;
        }
    }
}

fn is_admin_ip(headers: &HeaderMap) -> bool {
    let client_ip = headers
        .get("cf-connecting-ip")
        .or_else(|| headers.get("x-real-ip"))
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    client_ip.contains("89.103.195.115") || client_ip.contains("2a02:8309:810c:7600::6d64")
}

async fn handle_admin_page(headers: HeaderMap) -> Response {
    if !is_admin_ip(&headers) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    Html(include_str!("admin.html")).into_response()
}

async fn handle_admin_skip(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if !is_admin_ip(&headers) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    state.skip_notify.notify_one();
    (StatusCode::OK, "Track skipped").into_response()
}

async fn handle_admin_queue(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if !is_admin_ip(&headers) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    let q = state.queue.read().await.clone();
    Json(q).into_response()
}

async fn handle_admin_queue_reorder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ReorderReq>,
) -> Response {
    if !is_admin_ip(&headers) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    let mut q = state.queue.write().await;
    if payload.from < q.len() && payload.to < q.len() {
        let item = q.remove(payload.from);
        q.insert(payload.to, item);
        (StatusCode::OK, "Queue reordered").into_response()
    } else {
        (StatusCode::BAD_REQUEST, "Invalid range").into_response()
    }
}

async fn handle_admin_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Response {
    if !is_admin_ip(&headers) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    let q_str = query.q.unwrap_or_default().to_lowercase();
    let all = state.all_tracks.read().await;

    let filtered: Vec<TrackInfo> = if q_str.is_empty() {
        all.iter().take(20).cloned().collect()
    } else {
        all.iter()
            .filter(|t| {
                t.title.to_lowercase().contains(&q_str)
                    || t.artist.to_lowercase().contains(&q_str)
                    || t.album.to_lowercase().contains(&q_str)
            })
            .take(50)
            .cloned()
            .collect()
    };

    Json(filtered).into_response()
}

async fn handle_admin_play_now(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<PlayNowReq>,
) -> Response {
    if !is_admin_ip(&headers) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    let all = state.all_tracks.read().await;
    if let Some(track) = all.iter().find(|t| t.id == payload.id) {
        *state.next_priority_track.write().await = Some(track.clone());
        state.skip_notify.notify_one();
        (StatusCode::OK, "Track set to play now").into_response()
    } else {
        (StatusCode::NOT_FOUND, "Track not found").into_response()
    }
}

async fn handle_admin_queue_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<PlayNowReq>,
) -> Response {
    if !is_admin_ip(&headers) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    let all = state.all_tracks.read().await;
    if let Some(track) = all.iter().find(|t| t.id == payload.id) {
        state.queue.write().await.push(track.clone());
        (StatusCode::OK, "Track added to queue").into_response()
    } else {
        (StatusCode::NOT_FOUND, "Track not found").into_response()
    }
}

async fn handle_fallback_404() -> Response {
    (StatusCode::NOT_FOUND, "404 Not Found").into_response()
}

async fn handle_index_or_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    if accept.contains("audio/")
        || user_agent.contains("vlc")
        || user_agent.contains("mpv")
        || user_agent.contains("ffmpeg")
        || user_agent.contains("mplayer")
        || user_agent.contains("foobar")
    {
        return stream_audio_response(state).into_response();
    }

    Html(include_str!("index.html")).into_response()
}

async fn handle_stream(State(state): State<AppState>) -> Response {
    stream_audio_response(state).into_response()
}

fn stream_audio_response(state: AppState) -> Response {
    let mut rx = state.broadcast_tx.subscribe();
    state.listener_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let count_ref = state.listener_count.clone();

    let stream = async_stream::stream! {
        struct Guard(Arc<std::sync::atomic::AtomicUsize>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        let _guard = Guard(count_ref);

        loop {
            match rx.recv().await {
                Ok(bytes) => yield Ok::<Bytes, axum::Error>(bytes),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    let body = Body::from_stream(stream);

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("audio/mpeg"),
    );
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    response_headers.insert(
        header::CONNECTION,
        HeaderValue::from_static("keep-alive"),
    );
    response_headers.insert(
        header::TRANSFER_ENCODING,
        HeaderValue::from_static("chunked"),
    );
    response_headers.insert(
        HeaderName::from_static("icy-name"),
        HeaderValue::from_static("01337000 Radio"),
    );

    (StatusCode::OK, response_headers, body).into_response()
}

async fn handle_metadata_json(State(state): State<AppState>) -> Json<CurrentMetadata> {
    let mut meta = state.current_meta.read().await.clone();
    meta.listeners = state.listener_count.load(std::sync::atomic::Ordering::Relaxed);
    Json(meta)
}

async fn handle_metadata_sse(State(state): State<AppState>) -> Response {
    let mut rx = state.metadata_tx.subscribe();
    let count_ref = state.listener_count.clone();
    let initial = state.current_meta.read().await.clone();

    let stream = async_stream::stream! {
        let mut first = initial;
        first.listeners = count_ref.load(std::sync::atomic::Ordering::Relaxed);
        let json_str = serde_json::to_string(&first).unwrap_or_default();
        yield Ok::<String, axum::Error>(format!("data: {}\n\n", json_str));

        loop {
            match rx.recv().await {
                Ok(mut meta) => {
                    meta.listeners = count_ref.load(std::sync::atomic::Ordering::Relaxed);
                    let json_str = serde_json::to_string(&meta).unwrap_or_default();
                    yield Ok::<String, axum::Error>(format!("data: {}\n\n", json_str));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));

    (headers, Body::from_stream(stream)).into_response()
}
