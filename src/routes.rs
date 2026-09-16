use crate::icy::{IcyInterleaver, build_icy_headers, client_requests_icy_metadata};
use crate::models::{CurrentMetadata, PlayNowReq, ReorderReq, SearchQuery, TrackInfo};
use crate::state::AppState;
use axum::{
    Json,
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use bytes::Bytes;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::broadcast;

pub fn is_admin_ip(headers: &HeaderMap, allowed_ips: &[String]) -> bool {
    if allowed_ips.is_empty() {
        return true;
    }

    let client_ip = get_client_ip(headers);
    allowed_ips
        .iter()
        .any(|allowed| client_ip.contains(allowed.as_str()))
}

pub fn get_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("cf-connecting-ip")
        .or_else(|| headers.get("x-real-ip"))
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1")
        .to_string()
}

pub async fn handle_admin_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !is_admin_ip(&headers, &state.config.admin.allowed_ips) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    Html(include_str!("admin.html")).into_response()
}

pub async fn handle_admin_skip(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !is_admin_ip(&headers, &state.config.admin.allowed_ips) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    state.skip_notify.notify_one();
    (StatusCode::OK, "Track skipped").into_response()
}

pub async fn handle_admin_queue(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !is_admin_ip(&headers, &state.config.admin.allowed_ips) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    let q = state.queue.read().await.clone();
    Json(q).into_response()
}

pub async fn handle_admin_history(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !is_admin_ip(&headers, &state.config.admin.allowed_ips) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    let h = state.history.read().await.clone();
    Json(h).into_response()
}

pub async fn handle_admin_stats(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !is_admin_ip(&headers, &state.config.admin.allowed_ips) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    Json(state.stats.get_admin_stats()).into_response()
}

pub async fn handle_admin_genres(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !is_admin_ip(&headers, &state.config.admin.allowed_ips) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    let all = state.all_tracks.read().await;
    let mut genres: std::collections::HashSet<String> = HashSet::new();
    for t in all.iter() {
        if !t.genre.is_empty() {
            genres.insert(t.genre.clone());
        }
    }
    let mut list: Vec<String> = genres.into_iter().collect();
    list.sort();
    Json(list).into_response()
}

pub async fn handle_admin_queue_reorder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ReorderReq>,
) -> Response {
    if !is_admin_ip(&headers, &state.config.admin.allowed_ips) {
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

pub async fn handle_admin_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Response {
    if !is_admin_ip(&headers, &state.config.admin.allowed_ips) {
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

pub async fn handle_admin_play_now(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<PlayNowReq>,
) -> Response {
    if !is_admin_ip(&headers, &state.config.admin.allowed_ips) {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    let all = state.all_tracks.read().await;
    if let Some(track) = all.iter().find(|t| t.id == payload.id) {
        let mut q = state.queue.write().await;
        q.insert(0, track.clone());
        state.skip_notify.notify_one();
        (StatusCode::OK, "Track set to play now").into_response()
    } else {
        (StatusCode::NOT_FOUND, "Track not found").into_response()
    }
}

pub async fn handle_admin_queue_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<PlayNowReq>,
) -> Response {
    if !is_admin_ip(&headers, &state.config.admin.allowed_ips) {
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

pub async fn handle_fallback_404() -> Response {
    (StatusCode::NOT_FOUND, "404 Not Found").into_response()
}

pub async fn handle_index_or_stream(State(state): State<AppState>, headers: HeaderMap) -> Response {
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
        return stream_audio_response(state, &headers).into_response();
    }

    Html(include_str!("index.html")).into_response()
}

pub async fn handle_stream(State(state): State<AppState>, headers: HeaderMap) -> Response {
    stream_audio_response(state, &headers).into_response()
}

pub async fn handle_metrics(State(state): State<AppState>) -> Response {
    let body = state.metrics.render_prometheus();
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.04; charset=utf-8",
        )],
        body,
    )
        .into_response()
}

fn stream_audio_response(state: AppState, req_headers: &HeaderMap) -> Response {
    let client_ip = get_client_ip(req_headers);
    let guard = match state.limiter.acquire(client_ip) {
        Ok(g) => g,
        Err(_) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "429 Too Many Requests - Stream Rate Limit Exceeded",
            )
                .into_response();
        }
    };

    let ua_str = req_headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    state.metrics.inc_listener(ua_str);
    state
        .stats
        .record_listener_sample(state.metrics.active_listeners());

    let wants_icy = state.config.icy.enabled && client_requests_icy_metadata(req_headers);
    let mut rx = state.tx.subscribe();
    let mut meta_rx = state.meta_tx.subscribe();

    let metrics = Arc::clone(&state.metrics);
    let metaint = state.config.icy.metaint;

    // Stream generator
    let stream = async_stream::stream! {
        let _guard = guard;
        let mut interleaver = if wants_icy {
            Some(IcyInterleaver::new(metaint))
        } else {
            None
        };

        // Initialize interleaver metadata
        if let Some(inter) = &mut interleaver {
            let cur = state.current_meta.read().await;
            inter.set_metadata(&cur.artist, &cur.title);
        }

        loop {
            tokio::select! {
                meta_res = meta_rx.recv() => {
                    if let Ok(meta) = meta_res
                        && let Some(inter) = &mut interleaver
                    {
                        inter.set_metadata(&meta.artist, &meta.title);
                    }

                }
                msg = rx.recv() => {
                    match msg {
                        Ok(chunk) => {
                            let out_chunk = if let Some(inter) = &mut interleaver {
                                inter.process_audio_chunk(&chunk)
                            } else {
                                chunk
                            };

                            metrics.add_streamed_bytes(out_chunk.len() as u64);
                            yield Ok::<Bytes, axum::Error>(out_chunk);
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            metrics.inc_buffer_underrun();
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            }
        }

        metrics.dec_listener();
    };

    let body = Body::from_stream(stream);

    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/mpeg"));
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    response_headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    response_headers.insert(
        header::TRANSFER_ENCODING,
        HeaderValue::from_static("chunked"),
    );

    if wants_icy {
        let icy_headers = build_icy_headers(
            &state.config.station.name,
            "Radio",
            state.config.station.default_bitrate_kbps,
            metaint,
        );
        for (k, v) in icy_headers {
            response_headers.insert(k, v);
        }
    } else if let Ok(station_val) = HeaderValue::from_str(&state.config.station.name) {
        response_headers.insert(HeaderName::from_static("icy-name"), station_val);
    }

    (StatusCode::OK, response_headers, body).into_response()
}

pub async fn handle_metadata_json(State(state): State<AppState>) -> Json<CurrentMetadata> {
    let mut meta = state.current_meta.read().await.clone();
    meta.listeners = state.metrics.active_listeners();
    Json(meta)
}

pub async fn handle_metadata_sse(State(state): State<AppState>) -> Response {
    let mut rx = state.meta_tx.subscribe();
    let metrics = Arc::clone(&state.metrics);
    let initial = state.current_meta.read().await.clone();

    let stream = async_stream::stream! {
        let mut first = initial;
        first.listeners = metrics.active_listeners();
        let json_str = serde_json::to_string(&first).unwrap_or_default();
        yield Ok::<String, axum::Error>(format!("data: {}\n\n", json_str));

        loop {
            match rx.recv().await {
                Ok(mut meta) => {
                    meta.listeners = metrics.active_listeners();
                    let json_str = serde_json::to_string(&meta).unwrap_or_default();
                    yield Ok::<String, axum::Error>(format!("data: {}\n\n", json_str));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));

    (headers, Body::from_stream(stream)).into_response()
}
