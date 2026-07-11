# yt-dlp

[![CI](https://github.com/joshfinnie/yt-dlp.rs/actions/workflows/ci.yml/badge.svg)](https://github.com/joshfinnie/yt-dlp.rs/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/joshfinnie/yt-dlp.rs/branch/main/graph/badge.svg)](https://codecov.io/gh/joshfinnie/yt-dlp.rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A fast video downloader written in Rust. Inspired by [yt-dlp](https://github.com/yt-dlp/yt-dlp).

Downloads YouTube videos with no Python dependency, a single static binary, and a familiar interface.

## Features

- **YouTube** — full format support via the InnerTube API (no PoToken required)
- **Format selection** — `bestvideo+bestaudio`, height/ext/bitrate filters, fallback chains
- **Progress bars** — per-stream download progress with speed and ETA
- **HLS** — concurrent segment download with configurable parallelism
- **Optional ffmpeg** — merge separate video+audio streams for 1080p+; degrades gracefully without it
- **Output templates** — `%(title)s`, `%(id)s`, `%(upload_date)s`, etc.
- **Subtitle download** — manual and auto-generated captions in VTT/TTML/SRV
- **Resume** — interrupted HTTP downloads continue from where they left off
- **Pipeable** — status messages on stderr, data (JSON) on stdout

## Installation

### Pre-built binaries

Download the latest release for your platform from the [Releases](https://github.com/joshfinnie/yt-dlp.rs/releases) page.

### From source

Requires Rust 1.75+:

```sh
cargo install --git https://github.com/joshfinnie/yt-dlp.rs
```

Or clone and build:

```sh
git clone https://github.com/joshfinnie/yt-dlp.rs
cd yt-dlp.rs
cargo build --release
# binary at ./target/release/yt-dlp
```

## Quick start

```sh
# Download at best available quality
yt-dlp 'https://youtu.be/dQw4w9WgXcQ'

# List all available formats
yt-dlp -F 'https://youtu.be/dQw4w9WgXcQ'

# Download best 1080p video + best audio, merge to MKV (requires ffmpeg)
yt-dlp -f 'bestvideo[height<=1080]+bestaudio' 'https://youtu.be/dQw4w9WgXcQ'

# Extract audio only
yt-dlp -x -f bestaudio 'https://youtu.be/dQw4w9WgXcQ'

# Print video info as JSON (scriptable)
yt-dlp -j 'https://youtu.be/dQw4w9WgXcQ' 2>/dev/null | jq .title
```

## Usage

```
yt-dlp [OPTIONS] <URL>...
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-f FORMAT` | Format selection expression | `bestvideo+bestaudio/best` |
| `-o TEMPLATE` | Output filename template | `%(title)s [%(id)s].%(ext)s` |
| `-F` | List available formats and exit | |
| `-x` | Extract audio after download | |
| `--audio-format FMT` | Audio format: `m4a`, `mp3`, `opus` | `m4a` |
| `-j` | Dump video info as JSON (stdout) | |
| `-s` / `--simulate` | Dry run — show what would happen | |
| `--skip-download` | Extract info but don't download | |
| `--write-subs` | Save subtitle files | |
| `--write-auto-subs` | Save auto-generated captions | |
| `--sub-langs LANGS` | Comma-separated language codes, or `all` | `en` |
| `-r RATE` | Bandwidth limit e.g. `500K`, `2M` | unlimited |
| `-R N` | Retry count on failure | `10` |
| `--concurrent-fragments N` | Parallel HLS segment downloads | `4` |
| `--merge-output-format FMT` | Container when merging (`mkv`, `mp4`) | `mkv` |
| `-k` | Keep intermediate video/audio files | |
| `--restrict-filenames` | ASCII-only filenames | |
| `--proxy URL` | HTTP/HTTPS/SOCKS5 proxy | |
| `--no-progress` | Suppress progress bars | |
| `-q` | Quiet mode | |

## Format selection

Format selectors follow yt-dlp syntax and can be composed:

```sh
# Single best combined stream (video+audio in one file)
-f best

# Merge best video + best audio (requires ffmpeg)
-f bestvideo+bestaudio

# Best video up to 1080p + best audio
-f 'bestvideo[height<=1080]+bestaudio'

# Best mp4 video + best m4a audio
-f 'bestvideo[ext=mp4]+bestaudio[ext=m4a]'

# Try merge, fall back to combined if merge unavailable
-f 'bestvideo+bestaudio/best'

# Download a specific format by ID
-f 137

# Worst quality (useful for testing)
-f worst
```

**Keywords:** `best`, `worst`, `bestvideo` (`bv`), `worstvideo` (`wv`), `bestaudio` (`ba`), `worstaudio` (`wa`)

**Filters:** `[height<=1080]`, `[ext=mp4]`, `[abr>=128]`, `[vcodec=avc1]`

**Operators:** `=`, `!=`, `<`, `<=`, `>`, `>=`

**Composition:** `+` (merge), `/` (fallback)

## Output templates

Output filenames are expanded from `%(field)s` placeholders:

| Placeholder | Value |
|-------------|-------|
| `%(id)s` | Video ID |
| `%(title)s` | Video title |
| `%(ext)s` | File extension |
| `%(uploader)s` | Channel name |
| `%(upload_date)s` | Date as `YYYYMMDD` |
| `%(duration)s` | Duration as `HH:MM:SS` |
| `%(view_count)s` | View count |
| `%(playlist_index)s` | Zero-padded index in playlist |

```sh
# Organise by uploader
yt-dlp -o '%(uploader)s/%(upload_date)s %(title)s.%(ext)s' URL

# Include view count
yt-dlp -o '%(title)s [%(view_count)s views].%(ext)s' URL
```

## ffmpeg

ffmpeg is **optional**. Without it, `yt-dlp` falls back to the best *combined* format (video+audio in a single file, typically up to 720p).

With ffmpeg, you get:
- Up to 4K/HDR by merging separate video and audio adaptive streams
- Audio extraction (`-x`)
- Container remuxing

Install via your package manager:

```sh
# macOS
brew install ffmpeg

# Ubuntu/Debian
sudo apt install ffmpeg

# Windows (via Scoop)
scoop install ffmpeg
```

## Supported sites

| Site | Status |
|------|--------|
| YouTube | Full (videos, Shorts, live streams) |
| More coming | PRs welcome |

## WebAssembly (WASM) Browser Version

A WebAssembly port of the downloader is available in the `www/` directory. It runs entirely in the browser using Rust compiled to WASM for extraction and downloading, a Node.js CORS proxy to bypass security blocks, and `ffmpeg.wasm` to merge streams or convert audio to MP3 locally.

### Features

- **No Server-Side Downloader** — video parsing and download packet assembly occur entirely inside the user's browser thread.
- **In-Browser FFmpeg** — merges separate HD video and audio streams (MKV), or converts audio directly to MP3.
- **Strict CORP/COEP Isolation** — fully configured local asset pipeline allowing Web Workers to run under strict browser cross-origin isolation policies.

### Building the WASM Module

To compile the Rust extractor and downloader library to WebAssembly:

```sh
# Requires wasm-pack installed (cargo install wasm-pack)
wasm-pack build --target web --out-dir www/pkg
```

This compiles the library and generates the WebAssembly wrapper `www/pkg/yt_dlp.js` along with the WASM binary.

### Running Locally

To host the browser client and start the CORS proxy:

1. **Start the CORS Proxy** (to route Google Video CDN requests and bypass CORS):
   ```sh
   node www/cors-proxy.js
   ```
   *Runs on `http://localhost:8080`*

2. **Start the Web App Server** (implements COOP/COEP headers required for `ffmpeg.wasm` shared buffers):
   ```sh
   node www/server.js
   ```
   *Runs on `http://localhost:3000`*

Open `http://localhost:3000` in your browser.

## Contributors

- **Josh Finnie** — Original Author & Maintainer
- **Iqbal Rifai** — Ported the downloader pipeline to WebAssembly, built the Node.js CORS proxy integration, and implemented the local browser-based `ffmpeg.wasm` multiplexing/MP3 conversion system.

## License

MIT — see [LICENSE](LICENSE).
