use crate::config::GenreClusteringConfig;
use crate::models::TrackInfo;
use rand::Rng;
use rand::prelude::IndexedRandom;
use std::collections::{HashSet, VecDeque};

pub fn select_next_track(
    tracks: &[TrackInfo],
    history: &VecDeque<usize>,
    current_genre: &mut Option<String>,
    cluster_remaining: &mut usize,
    clustering_cfg: &GenreClusteringConfig,
) -> usize {
    if tracks.is_empty() {
        return 0;
    }

    let mut rng = rand::rng();
    let history_set: HashSet<usize> = history.iter().cloned().collect();

    let mut eligible: Vec<usize> = (0..tracks.len())
        .filter(|&idx| !history_set.contains(&tracks[idx].id))
        .collect();

    if eligible.is_empty() {
        let recent_half: HashSet<usize> = history
            .iter()
            .rev()
            .take(history.len() / 2)
            .cloned()
            .collect();
        eligible = (0..tracks.len())
            .filter(|&idx| !recent_half.contains(&tracks[idx].id))
            .collect();

        if eligible.is_empty() {
            eligible = (0..tracks.len()).collect();
        }
    }

    if !clustering_cfg.enabled {
        return *eligible.choose(&mut rng).unwrap_or(&0);
    }

    let cluster_min = clustering_cfg.min.max(1);
    let cluster_max = clustering_cfg.max.max(cluster_min);

    if *cluster_remaining == 0 || current_genre.is_none() {
        let chosen_idx = *eligible.choose(&mut rng).unwrap_or(&0);
        *current_genre = Some(tracks[chosen_idx].genre.clone());
        *cluster_remaining = rng.random_range(cluster_min..=cluster_max);
        chosen_idx
    } else {
        let genre_name = current_genre.as_ref().unwrap();
        let same_genre_candidates: Vec<usize> = eligible
            .iter()
            .cloned()
            .filter(|&idx| &tracks[idx].genre == genre_name && tracks[idx].genre != "Unknown Genre")
            .collect();

        if !same_genre_candidates.is_empty() {
            *cluster_remaining = cluster_remaining.saturating_sub(1);
            *same_genre_candidates.choose(&mut rng).unwrap()
        } else {
            let chosen_idx = *eligible.choose(&mut rng).unwrap_or(&0);
            *current_genre = Some(tracks[chosen_idx].genre.clone());
            *cluster_remaining = rng.random_range(cluster_min..=cluster_max);
            chosen_idx
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_test_tracks() -> Vec<TrackInfo> {
        vec![
            TrackInfo {
                id: 1,
                path: PathBuf::from("/music/rock1.mp3"),
                title: "Rock 1".into(),
                artist: "Artist A".into(),
                album: "Album 1".into(),
                genre: "Rock".into(),
                duration_secs: 180,
            },
            TrackInfo {
                id: 2,
                path: PathBuf::from("/music/rock2.mp3"),
                title: "Rock 2".into(),
                artist: "Artist B".into(),
                album: "Album 2".into(),
                genre: "Rock".into(),
                duration_secs: 200,
            },
            TrackInfo {
                id: 3,
                path: PathBuf::from("/music/jazz1.mp3"),
                title: "Jazz 1".into(),
                artist: "Artist C".into(),
                album: "Album 3".into(),
                genre: "Jazz".into(),
                duration_secs: 240,
            },
            TrackInfo {
                id: 4,
                path: PathBuf::from("/music/jazz2.mp3"),
                title: "Jazz 2".into(),
                artist: "Artist D".into(),
                album: "Album 4".into(),
                genre: "Jazz".into(),
                duration_secs: 210,
            },
            TrackInfo {
                id: 5,
                path: PathBuf::from("/music/ambient1.mp3"),
                title: "Ambient 1".into(),
                artist: "Artist E".into(),
                album: "Album 5".into(),
                genre: "Ambient".into(),
                duration_secs: 300,
            },
        ]
    }

    #[test]
    fn test_select_empty_tracks() {
        let history = VecDeque::new();
        let mut current_genre = None;
        let mut cluster_remaining = 0;
        let cfg = GenreClusteringConfig::default();

        let idx = select_next_track(
            &[],
            &history,
            &mut current_genre,
            &mut cluster_remaining,
            &cfg,
        );
        assert_eq!(idx, 0);
    }

    #[test]
    fn test_history_exclusion() {
        let tracks = make_test_tracks();
        let mut history = VecDeque::new();
        // Mark track 1, 2, 3, 4 as played
        history.push_back(1);
        history.push_back(2);
        history.push_back(3);
        history.push_back(4);

        let mut current_genre = None;
        let mut cluster_remaining = 0;
        let cfg = GenreClusteringConfig {
            enabled: false,
            min: 1,
            max: 1,
        };

        // Track 5 should be the only eligible track
        let idx = select_next_track(
            &tracks,
            &history,
            &mut current_genre,
            &mut cluster_remaining,
            &cfg,
        );
        assert_eq!(idx, 4);
        assert_eq!(tracks[idx].id, 5);
    }

    #[test]
    fn test_all_history_fallback() {
        let tracks = make_test_tracks();
        let mut history = VecDeque::new();
        for t in &tracks {
            history.push_back(t.id);
        }

        let mut current_genre = None;
        let mut cluster_remaining = 0;
        let cfg = GenreClusteringConfig {
            enabled: false,
            min: 1,
            max: 1,
        };

        let idx = select_next_track(
            &tracks,
            &history,
            &mut current_genre,
            &mut cluster_remaining,
            &cfg,
        );
        assert!(idx < tracks.len());
    }

    #[test]
    fn test_genre_clustering_continuity() {
        let tracks = make_test_tracks();
        let mut history = VecDeque::new();
        history.push_back(1); // Rock 1 played

        let mut current_genre = Some("Rock".to_string());
        let mut cluster_remaining = 2;
        let cfg = GenreClusteringConfig {
            enabled: true,
            min: 2,
            max: 4,
        };

        // Rock 2 is track index 1, which matches the current genre
        let idx = select_next_track(
            &tracks,
            &history,
            &mut current_genre,
            &mut cluster_remaining,
            &cfg,
        );
        assert_eq!(tracks[idx].genre, "Rock");
        assert_eq!(idx, 1);
        assert_eq!(cluster_remaining, 1);
    }

    #[test]
    fn test_clustering_switches_when_no_more_genre_candidates() {
        let tracks = make_test_tracks();
        let mut history = VecDeque::new();
        history.push_back(1);
        history.push_back(2); // Both rock tracks played

        let mut current_genre = Some("Rock".to_string());
        let mut cluster_remaining = 2;
        let cfg = GenreClusteringConfig {
            enabled: true,
            min: 2,
            max: 4,
        };

        let idx = select_next_track(
            &tracks,
            &history,
            &mut current_genre,
            &mut cluster_remaining,
            &cfg,
        );
        // Should switch genre because no more rock tracks are eligible
        assert_ne!(tracks[idx].genre, "Rock");
        assert_eq!(current_genre, Some(tracks[idx].genre.clone()));
    }
}
