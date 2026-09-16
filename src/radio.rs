use crate::audio::{AudioDecoder, Mp3Encoder};
use crate::clustering::select_next_track;
use crate::models::TrackInfo;
use crate::scanner::scan_music_dir;
use crate::state::AppState;
use bytes::Bytes;
use std::collections::VecDeque;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{error, info, warn};

pub async fn run_radio_loop(state: Arc<AppState>, music_path: PathBuf) {
    let history_capacity = state.config.playback.history_size;
    let mut history: VecDeque<usize> = VecDeque::with_capacity(history_capacity + 1);
    let mut current_genre: Option<String> = None;
    let mut cluster_remaining = 0;

    let mut tracks = scan_music_dir(&music_path);
    {
        let mut all = state.all_tracks.write().await;
        *all = tracks.clone();
    }

    loop {
        if tracks.is_empty() {
            tracks = scan_music_dir(&music_path);
            {
                let mut all = state.all_tracks.write().await;
                *all = tracks.clone();
            }

            if tracks.is_empty() {
                warn!("No audio tracks found in music directory. Retrying in 5 seconds...");
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        }

        let track = {
            let mut q = state.queue.write().await;
            if !q.is_empty() {
                q.remove(0)
            } else {
                let selected_index = select_next_track(
                    &tracks,
                    &history,
                    &mut current_genre,
                    &mut cluster_remaining,
                    &state.config.playback.genre_clustering,
                );
                tracks[selected_index].clone()
            }
        };

        history.push_back(track.id);
        if history.len() > history_capacity {
            history.pop_front();
        }

        // Add to history state
        {
            let mut hist = state.history.write().await;
            hist.push_back(track.clone());
            if hist.len() > history_capacity {
                hist.pop_front();
            }
        }

        // Refill queue
        {
            let mut q = state.queue.write().await;
            let target_queue_len = state.config.playback.queue_size;
            if q.len() < target_queue_len {
                let mut temp_history = history.clone();
                let mut temp_genre = current_genre.clone();
                let mut temp_cluster = cluster_remaining;

                while q.len() < target_queue_len {
                    let next_idx = select_next_track(
                        &tracks,
                        &temp_history,
                        &mut temp_genre,
                        &mut temp_cluster,
                        &state.config.playback.genre_clustering,
                    );
                    let next_track = tracks[next_idx].clone();
                    temp_history.push_back(next_track.id);
                    q.push(next_track);
                }
            }
        }

        state.stats.record_track_play(&track);

        info!(
            "Now playing [{}/{}]: '{}' by '{}' (Genre: '{}', Dur: {}s)",
            track.id,
            tracks.len(),
            track.title,
            track.artist,
            track.genre,
            track.duration_secs
        );

        let streamed_ok = if state.config.audio.reencode {
            stream_reencoded_track(&state, &track).await
        } else {
            false
        };

        if !streamed_ok {
            stream_raw_track(&state, &track).await;
        }
    }
}

async fn stream_reencoded_track(state: &Arc<AppState>, track: &TrackInfo) -> bool {
    let mut decoder = match AudioDecoder::open(&track.path) {
        Ok(d) => d,
        Err(e) => {
            warn!(
                "Could not decode track {:?}: {}. Falling back to raw.",
                track.path, e
            );
            return false;
        }
    };

    let bitrate = state.config.audio.target_bitrate_kbps;
    let mut encoder = match Mp3Encoder::new(decoder.sample_rate(), decoder.channels(), bitrate) {
        Ok(enc) => enc,
        Err(e) => {
            warn!("Could not init MP3 encoder: {}. Falling back to raw.", e);
            return false;
        }
    };

    let track_dur = if track.duration_secs > 0 {
        track.duration_secs
    } else {
        300
    };

    let track_start_instant = Instant::now();

    {
        let mut meta = state.current_meta.write().await;
        meta.title = track.title.clone();
        meta.artist = track.artist.clone();
        meta.album = track.album.clone();
        meta.genre = track.genre.clone();
        meta.duration_secs = track_dur;
        meta.elapsed_secs = 0;
        let _ = state.meta_tx.send(meta.clone());
    }

    let sample_rate = decoder.sample_rate() as f64;
    let mut total_samples_sent = 0u64;

    loop {
        tokio::select! {
            _ = state.skip_notify.notified() => {
                info!("Track skip requested via admin panel!");
                break;
            }
            _ = tokio::task::yield_now() => {}
        }

        match decoder.next_interleaved_pcm() {
            Ok(Some(pcm)) => {
                let samples_in_packet = (pcm.len() / 2) as u64;
                total_samples_sent += samples_in_packet;

                let mp3_bytes = encoder.encode_interleaved_pcm(&pcm);
                if !mp3_bytes.is_empty() {
                    let _ = state.tx.send(Bytes::from(mp3_bytes));
                }

                let elapsed_real_secs = track_start_instant.elapsed().as_secs();
                let current_elapsed_secs = elapsed_real_secs.min(track_dur);
                {
                    let mut meta = state.current_meta.write().await;
                    if meta.elapsed_secs != current_elapsed_secs {
                        meta.elapsed_secs = current_elapsed_secs;
                        let _ = state.meta_tx.send(meta.clone());
                    }
                }

                let expected_real_secs = total_samples_sent as f64 / sample_rate;
                let actual_real_secs = track_start_instant.elapsed().as_secs_f64();
                if expected_real_secs > actual_real_secs {
                    let sleep_secs = expected_real_secs - actual_real_secs;
                    sleep(Duration::from_secs_f64(sleep_secs)).await;
                }
            }
            Ok(None) => {
                let flushed = encoder.flush();
                if !flushed.is_empty() {
                    let _ = state.tx.send(Bytes::from(flushed));
                }
                break;
            }
            Err(e) => {
                error!("Decoding error on {:?}: {}", track.path, e);
                break;
            }
        }
    }

    true
}

async fn stream_raw_track(state: &Arc<AppState>, track: &TrackInfo) {
    let mmap = match File::open(&track.path)
        .and_then(|f| unsafe { memmap2::MmapOptions::new().map(&f) })
    {
        Ok(m) => Arc::new(m),
        Err(e) => {
            error!("Failed to mmap track file {:?}: {}", track.path, e);
            sleep(Duration::from_secs(1)).await;
            return;
        }
    };

    let file_len = mmap.len() as u64;
    let track_dur = if track.duration_secs > 0 {
        track.duration_secs
    } else {
        (file_len * 8) / (state.config.station.default_bitrate_kbps * 1000)
    };

    let bytes_per_sec = if track_dur > 0 {
        file_len as f64 / track_dur as f64
    } else {
        40000.0
    };

    let track_start_instant = Instant::now();

    {
        let mut meta = state.current_meta.write().await;
        meta.title = track.title.clone();
        meta.artist = track.artist.clone();
        meta.album = track.album.clone();
        meta.genre = track.genre.clone();
        meta.duration_secs = track_dur;
        meta.elapsed_secs = 0;
        let _ = state.meta_tx.send(meta.clone());
    }

    let mut offset = 0usize;
    let mut total_bytes_sent = 0u64;
    let chunk_size = state.config.station.chunk_size;

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
            _ = state.skip_notify.notified() => {
                info!("Track skip requested via admin panel!");
                break;
            }
            _ = tokio::task::yield_now() => {}
        }

        let chunk_end = (offset + chunk_size).min(mmap.len());
        let chunk_bytes = Bytes::copy_from_slice(&mmap[offset..chunk_end]);
        let n = chunk_end - offset;
        offset = chunk_end;
        total_bytes_sent += n as u64;

        let _ = state.tx.send(chunk_bytes);

        let elapsed_real_secs = track_start_instant.elapsed().as_secs();
        let current_elapsed_secs = elapsed_real_secs.min(track_dur);
        {
            let mut meta = state.current_meta.write().await;
            if meta.elapsed_secs != current_elapsed_secs {
                meta.elapsed_secs = current_elapsed_secs;
                let _ = state.meta_tx.send(meta.clone());
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
