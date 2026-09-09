# radio-streamer-rs

> Headless continuous audio streamer in Rust.

`radio-streamer-rs` continuously scans a local music library, extracts audio metadata, and broadcasts a real-time chunked audio stream (`audio/mpeg`) over HTTP. Any standard media player (VLC, mpv, ffmpeg, browser) can connect to the continuous stream URL with zero configuration.

```text
┌─────────────────────────┐
│ Local Music Repository  │
│      (MP3 / WAV)        │
└────────────┬────────────┘
             │ scan & metadata parsing
             ▼
┌─────────────────────────┐        broadcast chunk
│   radio-streamer-rs     ├───────────────────────────> ┌────────────────────┐
│  (Tokio & Symphonia)    │                             │  Browser / Player  │
└────────────┬────────────┘                             │ (VLC / mpv / web)  │
             │ SSE & metadata updates                   └────────────────────┘
             ▼
┌─────────────────────────┐
│    /api/metadata        │
│    /now-playing         │
└─────────────────────────┘
```

## Features

- Continuous 24/7 audio streaming with chunked transfer encoding (`audio/mpeg`)
- Native Icecast-compatible protocol headers (`icy-name`)
- Real-time metadata extraction using `id3` and `symphonia`
- Anti-repetition playback history window (250 tracks) and intelligent genre transition clustering
- Server-Sent Events (`/api/metadata/sse`) and live JSON endpoint (`/now-playing`)
- Minimalist embedded web player styled with monospace aesthetic
- Audio visualizer with mirror frequency spectrum analysis
- Low latency admin dashboard for real-time queue management and instant track skipping

## Architecture

- **Engine**: Built with Tokio asynchronous runtime and Axum HTTP framework.
- **Audio Decoding**: Powered by Symphonia for accurate track duration, bitrates, and demuxing.
- **Broadcasting**: Zero-allocation broadcast channels dispatching raw audio chunks across multiple concurrent listeners without disk buffering.
- **Zero Assets Overhead**: Front-end interfaces (`index.html`, `admin.html`) are embedded directly into the binary at compile time.

## Quick Start

### Prerequisites

- Rust 1.85+ (Edition 2024 compatible)
- Linux / macOS (tested on Debian x86_64)

### Build & Run

```bash
# Clone the repository
git clone https://github.com/9hb/radio-streamer-rs.git
cd radio-streamer-rs

# Build release binary
cargo build --release

# Run the server (reads config.toml by default)
./target/release/radio-streamer
```

## Configuration

Settings are configured via `config.toml` in the working directory (or overridden using the `CONFIG_PATH` environment variable). Environment variables (`PORT`, `BIND_ADDRESS`, `MUSIC_DIR`, `STATION_NAME`, `HISTORY_SIZE`) take precedence when set.

```toml
[server]
port = 9191
bind_address = "0.0.0.0"

[station]
name = "01337000 Radio"
music_dir = "./music"
chunk_size = 1024
default_bitrate_kbps = 320

[playback]
history_size = 250
queue_size = 10

[playback.genre_clustering]
enabled = true
min = 3
max = 5

[admin]
allowed_ips = ["127.0.0.1"]
```

### Endpoints

| Route | Description |
| :--- | :--- |
| `/` | Embedded web interface with player, visualizer, and stream details |
| `/stream` | Direct raw audio stream (`audio/mpeg`) for VLC, mpv, or browsers |
| `/now-playing` | Live track metadata in JSON format |
| `/api/metadata` | Metadata endpoint reporting duration, elapsed time, and listeners |
| `/api/metadata/sse` | Real-time Server-Sent Events stream for track changes |
| `/admin` | Admin dashboard (protected by IP allowlist) |
