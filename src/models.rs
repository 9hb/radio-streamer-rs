use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrackInfo {
    pub id: usize,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub duration_secs: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurrentMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub duration_secs: u64,
    pub elapsed_secs: u64,
    pub listeners: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PlayNowReq {
    pub id: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReorderReq {
    pub from: usize,
    pub to: usize,
}
