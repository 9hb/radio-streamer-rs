# radio-streamer-rs

> Minimalist headless audio radio streamer and Icecast-compatible broadcast engine written in Rust.

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

# Run the server
PORT=9191 MUSIC_DIR=/path/to/music ./target/release/radio-streamer
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
