use serde::Deserialize;
use std::path::Path;
use tracing::{info, warn};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_bind")]
    pub bind_address: String,
}

fn default_port() -> u16 {
    9191
}
fn default_bind() -> String {
    "0.0.0.0".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            bind_address: default_bind(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct StationConfig {
    #[serde(default = "default_station_name")]
    pub name: String,
    #[serde(default = "default_music_dir")]
    pub music_dir: String,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
    #[serde(default = "default_bitrate")]
    pub default_bitrate_kbps: u64,
}

fn default_station_name() -> String {
    "01337000 Radio".to_string()
}
fn default_music_dir() -> String {
    "./music".to_string()
}
fn default_chunk_size() -> usize {
    1024
}
fn default_bitrate() -> u64 {
    320
}

impl Default for StationConfig {
    fn default() -> Self {
        Self {
            name: default_station_name(),
            music_dir: default_music_dir(),
            chunk_size: default_chunk_size(),
            default_bitrate_kbps: default_bitrate(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct GenreClusteringConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_cluster_min")]
    pub min: usize,
    #[serde(default = "default_cluster_max")]
    pub max: usize,
}

fn default_true() -> bool {
    true
}
fn default_cluster_min() -> usize {
    3
}
fn default_cluster_max() -> usize {
    5
}

impl Default for GenreClusteringConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            min: default_cluster_min(),
            max: default_cluster_max(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PlaybackConfig {
    #[serde(default = "default_history_size")]
    pub history_size: usize,
    #[serde(default = "default_queue_size")]
    pub queue_size: usize,
    #[serde(default)]
    pub genre_clustering: GenreClusteringConfig,
}

fn default_history_size() -> usize {
    250
}
fn default_queue_size() -> usize {
    10
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            history_size: default_history_size(),
            queue_size: default_queue_size(),
            genre_clustering: GenreClusteringConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AdminConfig {
    #[serde(default = "default_allowed_ips")]
    pub allowed_ips: Vec<String>,
}

fn default_allowed_ips() -> Vec<String> {
    Vec::new()
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            allowed_ips: default_allowed_ips(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RateLimitConfig {
    #[serde(default = "default_rate_limit_enabled")]
    pub enabled: bool,
    #[serde(default = "default_requests_per_minute")]
    pub requests_per_minute: u32,
    #[serde(default = "default_burst_size")]
    pub burst_size: u32,
    #[serde(default = "default_max_connections_per_ip")]
    pub max_connections_per_ip: usize,
}

fn default_rate_limit_enabled() -> bool {
    true
}
fn default_requests_per_minute() -> u32 {
    60
}
fn default_burst_size() -> u32 {
    30
}
fn default_max_connections_per_ip() -> usize {
    10
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: default_rate_limit_enabled(),
            requests_per_minute: default_requests_per_minute(),
            burst_size: default_burst_size(),
            max_connections_per_ip: default_max_connections_per_ip(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct IcyConfig {
    #[serde(default = "default_icy_enabled")]
    pub enabled: bool,
    #[serde(default = "default_icy_metaint")]
    pub metaint: usize,
}

fn default_icy_enabled() -> bool {
    true
}
fn default_icy_metaint() -> usize {
    16384
}

impl Default for IcyConfig {
    fn default() -> Self {
        Self {
            enabled: default_icy_enabled(),
            metaint: default_icy_metaint(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AudioConfig {
    #[serde(default = "default_reencode_enabled")]
    pub reencode: bool,
    #[serde(default = "default_audio_bitrate")]
    pub target_bitrate_kbps: u64,
    #[serde(default = "default_sample_rate")]
    pub target_sample_rate: u32,
    #[serde(default = "default_channels")]
    pub channels: u16,
}

fn default_reencode_enabled() -> bool {
    true
}
fn default_audio_bitrate() -> u64 {
    320
}
fn default_sample_rate() -> u32 {
    44100
}
fn default_channels() -> u16 {
    2
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            reencode: default_reencode_enabled(),
            target_bitrate_kbps: default_audio_bitrate(),
            target_sample_rate: default_sample_rate(),
            channels: default_channels(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub station: StationConfig,
    #[serde(default)]
    pub playback: PlaybackConfig,
    #[serde(default)]
    pub admin: AdminConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub icy: IcyConfig,
    #[serde(default)]
    pub audio: AudioConfig,
}

impl AppConfig {
    pub fn parse(toml_str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Self {
        let p = path.as_ref();
        if let Ok(contents) = std::fs::read_to_string(p) {
            match Self::parse(&contents) {
                Ok(cfg) => {
                    info!("Loaded configuration from '{}'", p.display());
                    cfg
                }
                Err(e) => {
                    warn!("Failed to parse '{}': {}. Using defaults.", p.display(), e);
                    Self::default()
                }
            }
        } else {
            info!(
                "Config file '{}' not found, using defaults / env vars.",
                p.display()
            );
            Self::default()
        }
    }

    pub fn load() -> Self {
        let config_path =
            std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
        let mut cfg = Self::load_from_file(&config_path);

        if let Ok(p) = std::env::var("PORT")
            .and_then(|v| v.parse::<u16>().map_err(|_| std::env::VarError::NotPresent))
        {
            cfg.server.port = p;
        }
        if let Ok(b) = std::env::var("BIND_ADDRESS") {
            cfg.server.bind_address = b;
        }
        if let Ok(m) = std::env::var("MUSIC_DIR") {
            cfg.station.music_dir = m;
        }
        if let Ok(n) = std::env::var("STATION_NAME") {
            cfg.station.name = n;
        }

        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.server.port, 9191);
        assert_eq!(cfg.server.bind_address, "0.0.0.0");
        assert_eq!(cfg.station.name, "01337000 Radio");
        assert_eq!(cfg.station.chunk_size, 1024);
        assert_eq!(cfg.playback.history_size, 250);
        assert_eq!(cfg.playback.queue_size, 10);
        assert!(cfg.playback.genre_clustering.enabled);
        assert_eq!(cfg.playback.genre_clustering.min, 3);
        assert_eq!(cfg.playback.genre_clustering.max, 5);
        assert!(cfg.rate_limit.enabled);
        assert_eq!(cfg.rate_limit.requests_per_minute, 60);
        assert_eq!(cfg.rate_limit.max_connections_per_ip, 10);
        assert!(cfg.icy.enabled);
        assert_eq!(cfg.icy.metaint, 16384);
        assert!(cfg.audio.reencode);
        assert_eq!(cfg.audio.target_bitrate_kbps, 320);
    }

    #[test]
    fn test_parse_custom_toml() {
        let toml_str = r#"
            [server]
            port = 8080
            bind_address = "127.0.0.1"

            [station]
            name = "Custom Station"
            music_dir = "/media/music"
            chunk_size = 2048
            default_bitrate_kbps = 192

            [playback]
            history_size = 50
            queue_size = 5

            [playback.genre_clustering]
            enabled = false
            min = 1
            max = 2

            [admin]
            allowed_ips = ["127.0.0.1", "10.0.0.1"]

            [rate_limit]
            enabled = true
            requests_per_minute = 120
            burst_size = 40
            max_connections_per_ip = 5

            [icy]
            enabled = true
            metaint = 8192

            [audio]
            reencode = false
            target_bitrate_kbps = 192
            target_sample_rate = 48000
            channels = 2
        "#;

        let cfg = AppConfig::parse(toml_str).expect("Valid TOML should parse");
        assert_eq!(cfg.server.port, 8080);
        assert_eq!(cfg.server.bind_address, "127.0.0.1");
        assert_eq!(cfg.station.name, "Custom Station");
        assert_eq!(cfg.station.chunk_size, 2048);
        assert_eq!(cfg.playback.history_size, 50);
        assert!(!cfg.playback.genre_clustering.enabled);
        assert_eq!(cfg.admin.allowed_ips, vec!["127.0.0.1", "10.0.0.1"]);
        assert_eq!(cfg.rate_limit.requests_per_minute, 120);
        assert_eq!(cfg.icy.metaint, 8192);
        assert!(!cfg.audio.reencode);
        assert_eq!(cfg.audio.target_bitrate_kbps, 192);
        assert_eq!(cfg.audio.target_sample_rate, 48000);
    }

    #[test]
    fn test_parse_partial_toml() {
        let toml_str = r#"
            [server]
            port = 7000
        "#;
        let cfg = AppConfig::parse(toml_str).expect("Partial TOML should parse with defaults");
        assert_eq!(cfg.server.port, 7000);
        assert_eq!(cfg.server.bind_address, "0.0.0.0");
        assert_eq!(cfg.station.name, "01337000 Radio");
        assert_eq!(cfg.rate_limit.max_connections_per_ip, 10);
    }

    #[test]
    fn test_parse_invalid_toml() {
        let toml_str = "server = not a valid toml :(";
        assert!(AppConfig::parse(toml_str).is_err());
    }
}
