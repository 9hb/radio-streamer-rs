use crate::models::TrackInfo;
use id3::{Tag, TagLike};
use std::{fs::File, path::Path};
use symphonia::core::codecs::CODEC_TYPE_NULL;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use walkdir::WalkDir;

pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "wav", "flac", "ogg", "aac", "m4a"];

pub fn is_supported_audio_extension(ext: &str) -> bool {
    let lower = ext.to_lowercase();
    SUPPORTED_EXTENSIONS.contains(&lower.as_str())
}

pub fn get_exact_duration_secs(path: &Path) -> u64 {
    let Ok(file) = File::open(path) else {
        return 0;
    };

    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }

    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();

    if let Ok(probed) = symphonia::default::get_probe().format(&hint, mss, &fmt_opts, &meta_opts) {
        let format = probed.format;
        if let Some(track) = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        {
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
    0
}

pub fn scan_music_dir(dir: &Path) -> Vec<TrackInfo> {
    let mut tracks = Vec::new();
    let mut id_counter = 0;

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };

        if !is_supported_audio_extension(ext) {
            continue;
        }

        let ext_lower = ext.to_lowercase();
        let mut title = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown Track".to_string());
        let mut artist = "Unknown Artist".to_string();
        let mut album = "Unknown Album".to_string();
        let mut genre = "Unknown Genre".to_string();

        let duration_secs = get_exact_duration_secs(path);

        if ext_lower == "mp3"
            && let Ok(tag) = Tag::read_from_path(path)
        {
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

    tracks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_extensions() {
        assert!(is_supported_audio_extension("mp3"));
        assert!(is_supported_audio_extension("MP3"));
        assert!(is_supported_audio_extension("wav"));
        assert!(is_supported_audio_extension("flac"));
        assert!(is_supported_audio_extension("ogg"));
        assert!(is_supported_audio_extension("aac"));
        assert!(is_supported_audio_extension("m4a"));
        assert!(!is_supported_audio_extension("txt"));
        assert!(!is_supported_audio_extension("exe"));
    }

    #[test]
    fn test_scan_empty_directory() {
        let temp_dir = std::env::temp_dir().join("radio_test_empty_scan");
        let _ = std::fs::create_dir_all(&temp_dir);
        let tracks = scan_music_dir(&temp_dir);
        assert!(tracks.is_empty());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_scan_unsupported_files() {
        let temp_dir = std::env::temp_dir().join("radio_test_unsupported_scan");
        let _ = std::fs::create_dir_all(&temp_dir);
        let file_path = temp_dir.join("test.txt");
        let _ = std::fs::write(&file_path, "hello world");

        let tracks = scan_music_dir(&temp_dir);
        assert!(tracks.is_empty());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
