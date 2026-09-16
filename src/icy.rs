use axum::http::{HeaderMap, HeaderName, HeaderValue};
use bytes::{BufMut, Bytes, BytesMut};

pub const DEFAULT_ICY_METAINT: usize = 16384;

pub fn client_requests_icy_metadata(headers: &HeaderMap) -> bool {
    for (name, val) in headers {
        if name.as_str().eq_ignore_ascii_case("icy-metadata")
            && let Ok(str_val) = val.to_str()
            && str_val.trim() == "1"
        {
            return true;
        }
    }
    false
}

pub fn build_icy_headers(
    station_name: &str,
    station_genre: &str,
    bitrate_kbps: u64,
    metaint: usize,
) -> Vec<(HeaderName, HeaderValue)> {
    let mut h = Vec::new();

    if let Ok(v) = HeaderValue::from_str(&metaint.to_string()) {
        h.push((HeaderName::from_static("icy-metaint"), v));
    }
    if let Ok(v) = HeaderValue::from_str(station_name) {
        h.push((HeaderName::from_static("icy-name"), v));
    }
    if let Ok(v) = HeaderValue::from_str(station_genre) {
        h.push((HeaderName::from_static("icy-genre"), v));
    }
    if let Ok(v) = HeaderValue::from_str(&bitrate_kbps.to_string()) {
        h.push((HeaderName::from_static("icy-br"), v));
    }
    h.push((
        HeaderName::from_static("icy-pub"),
        HeaderValue::from_static("1"),
    ));
    h.push((
        HeaderName::from_static("icy-description"),
        HeaderValue::from_static("Rust High-Fidelity Radio Stream"),
    ));
    h
}

pub fn format_icy_metadata(artist: &str, title: &str) -> Vec<u8> {
    // Format: StreamTitle='Artist - Title';
    let clean_artist = artist.replace('\'', " ");
    let clean_title = title.replace('\'', " ");
    let meta_str = format!(
        "StreamTitle='{} - {}';",
        clean_artist.trim(),
        clean_title.trim()
    );
    let meta_bytes = meta_str.as_bytes();
    let len = meta_bytes.len();

    // The length byte represents the number of 16-byte blocks
    let num_blocks = len.div_ceil(16);
    if num_blocks > 255 {
        // Max 255 blocks (4080 bytes)
        return vec![0];
    }

    let padded_len = num_blocks * 16;
    let mut out = Vec::with_capacity(1 + padded_len);
    out.push(num_blocks as u8);
    out.extend_from_slice(meta_bytes);
    out.resize(1 + padded_len, 0u8);
    out
}

pub struct IcyInterleaver {
    metaint: usize,
    remaining_until_meta: usize,
    current_meta_frame: Vec<u8>,
    meta_dirty: bool,
}

impl IcyInterleaver {
    pub fn new(metaint: usize) -> Self {
        let actual_metaint = if metaint == 0 {
            DEFAULT_ICY_METAINT
        } else {
            metaint
        };
        Self {
            metaint: actual_metaint,
            remaining_until_meta: actual_metaint,
            current_meta_frame: vec![0u8],
            meta_dirty: false,
        }
    }

    pub fn set_metadata(&mut self, artist: &str, title: &str) {
        let frame = format_icy_metadata(artist, title);
        if self.current_meta_frame != frame {
            self.current_meta_frame = frame;
            self.meta_dirty = true;
        }
    }

    pub fn process_audio_chunk(&mut self, audio: &[u8]) -> Bytes {
        let mut out = BytesMut::with_capacity(audio.len() + 64);
        let mut offset = 0;

        while offset < audio.len() {
            let available = audio.len() - offset;
            let take = available.min(self.remaining_until_meta);

            out.put_slice(&audio[offset..offset + take]);
            offset += take;
            self.remaining_until_meta -= take;

            if self.remaining_until_meta == 0 {
                if self.meta_dirty {
                    out.put_slice(&self.current_meta_frame);
                    self.meta_dirty = false;
                } else {
                    out.put_u8(0);
                }
                self.remaining_until_meta = self.metaint;
            }
        }

        out.freeze()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_requests_icy() {
        let mut headers = HeaderMap::new();
        assert!(!client_requests_icy_metadata(&headers));

        headers.insert("icy-metadata", HeaderValue::from_static("1"));
        assert!(client_requests_icy_metadata(&headers));

        headers.insert("Icy-MetaData", HeaderValue::from_static(" 1 "));
        assert!(client_requests_icy_metadata(&headers));

        headers.insert("icy-metadata", HeaderValue::from_static("0"));
        assert!(!client_requests_icy_metadata(&headers));
    }

    #[test]
    fn test_build_icy_headers() {
        let headers = build_icy_headers("My Station", "Synthwave", 320, 8192);
        let find_header = |name: &str| {
            headers
                .iter()
                .find(|(k, _)| k.as_str() == name)
                .map(|(_, v)| v.to_str().unwrap().to_string())
        };

        assert_eq!(find_header("icy-metaint"), Some("8192".into()));
        assert_eq!(find_header("icy-name"), Some("My Station".into()));
        assert_eq!(find_header("icy-genre"), Some("Synthwave".into()));
        assert_eq!(find_header("icy-br"), Some("320".into()));
        assert_eq!(find_header("icy-pub"), Some("1".into()));
    }

    #[test]
    fn test_format_icy_metadata() {
        let meta = format_icy_metadata("Daft Punk", "Get Lucky");
        assert!(!meta.is_empty());

        let num_blocks = meta[0] as usize;
        assert!(num_blocks > 0);
        assert_eq!(meta.len(), 1 + num_blocks * 16);

        let text = std::str::from_utf8(&meta[1..1 + "StreamTitle='Daft Punk - Get Lucky';".len()])
            .unwrap();
        assert_eq!(text, "StreamTitle='Daft Punk - Get Lucky';");

        // The remaining bytes must be null padding
        for &byte in &meta[1 + text.len()..] {
            assert_eq!(byte, 0);
        }
    }

    #[test]
    fn test_interleaving_audio() {
        let metaint = 10;
        let mut interleaver = IcyInterleaver::new(metaint);
        interleaver.set_metadata("Artist", "Track");

        // Audio of 10 bytes -> should append metadata block
        let audio1 = vec![0xAA; 10];
        let out1 = interleaver.process_audio_chunk(&audio1);
        assert_eq!(&out1[0..10], &audio1[..]);
        assert!(out1.len() > 10);
        assert_eq!(out1[10], interleaver.current_meta_frame[0]);

        // Next 10 bytes of audio with no metadata change -> should append single 0 byte
        let audio2 = vec![0xBB; 10];
        let out2 = interleaver.process_audio_chunk(&audio2);
        assert_eq!(out2.len(), 11);
        assert_eq!(&out2[0..10], &audio2[..]);
        assert_eq!(out2[10], 0x00);
    }
}
