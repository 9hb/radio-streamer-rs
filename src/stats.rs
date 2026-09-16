use crate::models::TrackInfo;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_LISTENER_HISTORY_POINTS: usize = 360; // e.g. 6 hours at 1/minute or recent snapshots

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListenerDataPoint {
    pub timestamp: u64,
    pub listeners: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatCount {
    pub name: String,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminStatsResponse {
    pub listener_history: Vec<ListenerDataPoint>,
    pub top_artists: Vec<StatCount>,
    pub top_genres: Vec<StatCount>,
    pub total_tracks_played: u64,
}

pub struct StatsTracker {
    listener_history: RwLock<VecDeque<ListenerDataPoint>>,
    artist_plays: RwLock<HashMap<String, u64>>,
    genre_plays: RwLock<HashMap<String, u64>>,
    total_plays: RwLock<u64>,
}

impl Default for StatsTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsTracker {
    pub fn new() -> Self {
        Self {
            listener_history: RwLock::new(VecDeque::with_capacity(MAX_LISTENER_HISTORY_POINTS)),
            artist_plays: RwLock::new(HashMap::new()),
            genre_plays: RwLock::new(HashMap::new()),
            total_plays: RwLock::new(0),
        }
    }

    pub fn record_listener_sample(&self, listeners: usize) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Ok(mut history) = self.listener_history.write() {
            if history.len() >= MAX_LISTENER_HISTORY_POINTS {
                history.pop_front();
            }
            history.push_back(ListenerDataPoint {
                timestamp: now,
                listeners,
            });
        }
    }

    pub fn record_track_play(&self, track: &TrackInfo) {
        if let Ok(mut count) = self.total_plays.write() {
            *count += 1;
        }

        if !track.artist.is_empty()
            && track.artist != "Unknown Artist"
            && let Ok(mut artists) = self.artist_plays.write()
        {
            *artists.entry(track.artist.clone()).or_insert(0) += 1;
        }

        if !track.genre.is_empty()
            && track.genre != "Unknown Genre"
            && let Ok(mut genres) = self.genre_plays.write()
        {
            *genres.entry(track.genre.clone()).or_insert(0) += 1;
        }
    }

    pub fn get_top_artists(&self, limit: usize) -> Vec<StatCount> {
        let Ok(artists) = self.artist_plays.read() else {
            return Vec::new();
        };

        let mut list: Vec<StatCount> = artists
            .iter()
            .map(|(k, v)| StatCount {
                name: k.clone(),
                count: *v,
            })
            .collect();

        list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
        list.truncate(limit);
        list
    }

    pub fn get_top_genres(&self, limit: usize) -> Vec<StatCount> {
        let Ok(genres) = self.genre_plays.read() else {
            return Vec::new();
        };

        let mut list: Vec<StatCount> = genres
            .iter()
            .map(|(k, v)| StatCount {
                name: k.clone(),
                count: *v,
            })
            .collect();

        list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
        list.truncate(limit);
        list
    }

    pub fn get_admin_stats(&self) -> AdminStatsResponse {
        let listener_history = self
            .listener_history
            .read()
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default();

        let top_artists = self.get_top_artists(10);
        let top_genres = self.get_top_genres(10);
        let total_tracks_played = self.total_plays.read().map(|v| *v).unwrap_or(0);

        AdminStatsResponse {
            listener_history,
            top_artists,
            top_genres,
            total_tracks_played,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_record_and_get_stats() {
        let stats = StatsTracker::new();
        stats.record_listener_sample(5);
        stats.record_listener_sample(8);

        let t1 = TrackInfo {
            id: 1,
            path: PathBuf::new(),
            title: "Song 1".into(),
            artist: "Artist A".into(),
            album: "Album 1".into(),
            genre: "Electronic".into(),
            duration_secs: 180,
        };
        let t2 = TrackInfo {
            id: 2,
            path: PathBuf::new(),
            title: "Song 2".into(),
            artist: "Artist A".into(),
            album: "Album 1".into(),
            genre: "Rock".into(),
            duration_secs: 200,
        };

        stats.record_track_play(&t1);
        stats.record_track_play(&t2);
        stats.record_track_play(&t1);

        let top_artists = stats.get_top_artists(5);
        assert_eq!(top_artists.len(), 1);
        assert_eq!(top_artists[0].name, "Artist A");
        assert_eq!(top_artists[0].count, 3);

        let top_genres = stats.get_top_genres(5);
        assert_eq!(top_genres.len(), 2);
        assert_eq!(top_genres[0].name, "Electronic");
        assert_eq!(top_genres[0].count, 2);
        assert_eq!(top_genres[1].name, "Rock");
        assert_eq!(top_genres[1].count, 1);

        let summary = stats.get_admin_stats();
        assert_eq!(summary.listener_history.len(), 2);
        assert_eq!(summary.total_tracks_played, 3);
    }
}
