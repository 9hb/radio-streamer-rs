# radio-streamer-rs

> High-performance, continuous multi-format audio broadcast engine in Rust.

`radio-streamer-rs` is a headless audio radio station written in Rust. It continuously scans your music library, decodes any input audio format (**MP3, FLAC, WAV, OGG/Vorbis, AAC, M4A**) into PCM, and live-re-encodes into a broadcast-grade constant bitrate stream (**CBR 320 / 192 kbps**) via LAME.

It features native **SHOUTcast/Icecast ICY metadata interleaving** (`Icy-MetaData: 1`), an embedded web player with real-time spectrum visualization and keyboard controls, an authenticated administrative dashboard with interactive listener trend charts, IP rate limiting against abuse, and a Prometheus `/metrics` monitoring endpoint.

```text
┌────────────────────────────────────────────────────────┐
│               Local Music Repository                   │
│         (FLAC / WAV / OGG / AAC / M4A / MP3)           │
└───────────────────────────┬────────────────────────────┘
                            │ scan & duration probing
                            ▼
┌────────────────────────────────────────────────────────┐
│                   radio-streamer-rs                    │
│   ┌─────────────────────┐       ┌──────────────────┐   │
│   │  Symphonia Decoder  │ ────> │   LAME Encoder   │   │
│   │    (Multi-format)   │  PCM  │     (CBR MP3)    │   │
│   └─────────────────────┘       └────────┬─────────┘   │
│                                          │             │
│                                 broadcast channel      │
└───────────────────────────┬──────────────┼─────────────┘
                            │              │
      ICY Interleaving & SSE│              │ raw / ICY audio chunk
                            ▼              ▼
           ┌─────────────────────┐    ┌──────────────────────────┐
           │   Metadata & API    │    │     Media Listeners      │
           │  • /api/metadata    │    │ • VLC / mpv / Foobar2000 │
           │  • /now-playing     │    │ • Embedded Web Player    │
           │  • /metrics (Prom)  │    │ • Mobile & Car Headunits │
           │  • /api/admin/stats │    └──────────────────────────┘
           └─────────────────────┘
```

---

## Key Features

* **Multi-Format Decoding & Re-encoding**: Powered by **Symphonia** and **LAME** (`mp3lame-sys`). Decodes FLAC, WAV, OGG, AAC, M4A, and MP3 into clean PCM and streams a uniform, constant bitrate MP3 feed so clients never glitch across format transitions.
* **Native ICY Metadata Protocol (`Icy-MetaData: 1`)**: Full support for classic SHOUTcast/Icecast metadata interleaving (`StreamTitle='Artist - Title';` at dynamic byte intervals). VLC, mpv, Foobar2000, Winamp, and hardware audio receivers display track titles immediately.
* **Intelligent Track Selection & Genre Clustering**:
  * Configurable anti-repetition window (default 250 tracks) to avoid repeats.
  * Natural genre clustering keeping coherent musical sets (min 3–max 5 tracks) before transitioning.
* **Stream Rate Limiting & Abuse Prevention**: Configurable requests-per-minute window and per-IP active concurrent connection limits to defend against DoS and bandwidth hogging.
* **Prometheus `/metrics` Endpoint**: Standard Prometheus metrics exporting:
  * `radio_listeners_active`: Real-time gauge of active listeners.
  * `radio_streamed_bytes_total` / `radio_streamed_gigabytes_total`: Total data transferred.
  * `radio_client_user_agent_total`: Listener breakdown by client (VLC, mpv, Chrome, Firefox, CLI, etc.).
  * `radio_buffer_underruns_total`: Slow-consumer lag counters.
* **Admin Dashboard & Listener Analytics**:
  * Real-time Canvas graph plotting listener counts over time.
  * Top played artists and top played genres with frequency counters.
  * Drag-and-drop queue reordering, instant skip (`⚡ SKIP`), and live library search.
* **Minimalist Web Player**:
  * Monospace aesthetic styled player with HTML5 Canvas spectrum visualizer.
  * **Keyboard Shortcuts**: `Space` for Instant Play / Mute toggle, `Arrow Up` / `Arrow Down` for fine-grained volume adjustment.
  * Server-Sent Events (`/api/metadata/sse`) for instant UI track updates without polling.
* **Enterprise Concurrency**: Validated with built-in benchmarks handling **5,000+ concurrent listeners** per broadcast channel with **100.0% delivery** and zero packet lag.

---

## Quick Start

### Prerequisites

* Rust 1.85+ (Edition 2024 compatible)
* C compiler (`gcc` or `clang`) for LAME C bindings

### Build & Run

```bash
# Clone repository
git clone https://github.com/9hb/radio-streamer-rs.git
cd radio-streamer-rs

# Run tests
cargo test

# Build release binary
cargo build --release

# Run the streamer
./target/release/radio-streamer
```

---

## Configuration

Settings are configured via `config.toml` or overridden using environment variables (`PORT`, `BIND_ADDRESS`, `MUSIC_DIR`, `STATION_NAME`, `CONFIG_PATH`).

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
allowed_ips = [] # Empty list allows all IPs; specify IPs to restrict admin panel

[rate_limit]
enabled = true
requests_per_minute = 60
burst_size = 30
max_connections_per_ip = 10

[icy]
enabled = true
metaint = 16384

[audio]
reencode = true
target_bitrate_kbps = 320
target_sample_rate = 44100
channels = 2
```

---

## Endpoints

| Route | Method | Description |
| :--- | :--- | :--- |
| `/` | `GET` | Embedded web interface with player, visualizer, and keyboard shortcuts |
| `/stream` | `GET` | Audio stream (`audio/mpeg`). Supports `Icy-MetaData: 1` interleaving |
| `/now-playing` | `GET` | Current track metadata in JSON format |
| `/metrics` | `GET` | Prometheus metrics for listeners, bandwidth, user-agents, and buffer |
| `/admin` | `GET` | Admin control panel (queue reorder, skip, listener history chart) |
| `/api/metadata` | `GET` | JSON endpoint reporting duration, elapsed time, and listener count |
| `/api/metadata/sse` | `GET` | Server-Sent Events (SSE) stream for real-time player updates |
| `/api/admin/stats` | `GET` | Historical listener time series and top played artists/genres |
| `/api/admin/queue` | `GET` | Upcoming track queue |
| `/api/admin/history` | `GET` | Recently played tracks history |
| `/api/admin/genres` | `GET` | List of all discovered music genres in the library |
| `/api/admin/search` | `GET` | Search track catalog by title, artist, or album |
| `/api/admin/skip` | `POST` | Immediately skip currently playing track |
| `/api/admin/play-now` | `POST` | Preempt queue and play requested track immediately |
| `/api/admin/queue/reorder` | `POST` | Reorder upcoming queue via drag-and-drop |
| `/api/admin/queue/add` | `POST` | Add track to upcoming queue |

---

## Concurrency Benchmarks

The project includes an automated concurrency benchmark measuring tokio broadcast channel throughput across thousands of parallel listener tasks:

```bash
cargo bench
```

### Benchmark Results

* **Paced Streaming (Real-world Audio Delivery at 320 kbps)**:
  * **10 to 5,000 concurrent listeners**: **100.0% delivery rate**, **0 lagged/dropped packets**.
  * Throughput scales linearly up to **~600 MB/s** (~607,000 msgs/s) on standard multicore hardware.
* **Maximum Saturating Throughput (Zero-delay stress test)**:
  * Raw channel throughput exceeds **18,000 MB/s** (~18.6 million msgs/s).

---

## CI & Automated Releases

The repository is configured with production-grade GitHub Actions (`.github/workflows/ci.yml`) enforcing:
* Code formatting (`cargo fmt --check`)
* Strict linting (`cargo clippy --all-targets -- -D warnings`)
* Automated test suite execution (`cargo test`)
* Native `x86_64` and cross-compiled `aarch64` (ARM64) binary builds
* Automated release creation with checksum verification (`SHA256SUMS.txt`) upon version bump in `Cargo.toml`

---

## License

MIT / Apache 2.0
