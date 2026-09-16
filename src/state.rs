use crate::config::AppConfig;
use crate::limiter::StreamRateLimiter;
use crate::metrics::MetricsTracker;
use crate::models::{CurrentMetadata, TrackInfo};
use crate::stats::StatsTracker;
use bytes::Bytes;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock, broadcast};

#[derive(Clone)]
pub struct AppState {
    pub tx: broadcast::Sender<Bytes>,
    pub meta_tx: broadcast::Sender<CurrentMetadata>,
    pub current_meta: Arc<RwLock<CurrentMetadata>>,
    pub queue: Arc<RwLock<Vec<TrackInfo>>>,
    pub history: Arc<RwLock<VecDeque<TrackInfo>>>,
    pub all_tracks: Arc<RwLock<Vec<TrackInfo>>>,
    pub skip_notify: Arc<Notify>,
    pub config: Arc<AppConfig>,
    pub metrics: Arc<MetricsTracker>,
    pub stats: Arc<StatsTracker>,
    pub limiter: Arc<StreamRateLimiter>,
}

impl AppState {
    pub fn new(
        tx: broadcast::Sender<Bytes>,
        meta_tx: broadcast::Sender<CurrentMetadata>,
        config: Arc<AppConfig>,
    ) -> Self {
        let limiter = Arc::new(StreamRateLimiter::new(config.rate_limit.clone()));
        let metrics = Arc::new(MetricsTracker::new());
        let stats = Arc::new(StatsTracker::new());

        let current_meta = Arc::new(RwLock::new(CurrentMetadata {
            title: "Waiting for stream...".to_string(),
            artist: "Radio Server".to_string(),
            album: "".to_string(),
            genre: "".to_string(),
            duration_secs: 0,
            elapsed_secs: 0,
            listeners: 0,
        }));

        Self {
            tx,
            meta_tx,
            current_meta,
            queue: Arc::new(RwLock::new(Vec::new())),
            history: Arc::new(RwLock::new(VecDeque::new())),
            all_tracks: Arc::new(RwLock::new(Vec::new())),
            skip_notify: Arc::new(Notify::new()),
            config,
            metrics,
            stats,
            limiter,
        }
    }
}
