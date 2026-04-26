# FeTorrent

A modern BitTorrent client and engine built in Rust, with a daemon backend, CLI, and web UI.

## What is in this repo

FeTorrent is a workspace with four Rust crates and one frontend app:

- crates/core: BitTorrent engine (peer sessions, piece management, trackers, magnet metadata)
- crates/api: Axum HTTP/WebSocket API used by CLI and UI
- crates/daemon: Long-running process hosting the engine and API
- crates/cli: Command-line client for adding, listing, and controlling torrents
- ui: React + Vite dashboard

## Features

- Magnet link support
- .torrent file support
- Real-time updates over WebSocket
- CLI and web UI controls
- Configurable download directory and limits
- Pause, resume, remove torrent lifecycle actions

## Quick Start

### Prerequisites

- Rust toolchain (stable)
- Node.js 18+ (for UI development)

### Build

```bash
cargo build --release
```

### Run daemon

```bash
./target/release/fetorrent-daemon --port 6977
```

or via CLI:

```bash
./target/release/fetorrent run --port 6977
```

### Add a torrent (CLI)

```bash
./target/release/fetorrent add "magnet:?xt=urn:btih:..."
```

You can also pass a download folder directly:

```bash
./target/release/fetorrent add "magnet:?xt=urn:btih:..." --dir "/absolute/path"
```

### List torrents

```bash
./target/release/fetorrent list
```

### Web UI

When daemon is running on port 6977, open:

- http://127.0.0.1:6977

## Download Location Selection

FeTorrent supports choosing download location in both interfaces:

- CLI: add command asks for location when --dir is not provided
- UI: Add Torrent dialog and Settings both support selecting or entering directory

Note: In some desktop/browser setups, native folder picker may not be available from the daemon process. UI falls back to manual path input automatically.

## Configuration

Configuration is stored in your OS config directory under fetorrent/config.toml.

Key fields:

- server.bind
- server.port
- downloads.directory
- downloads.max_peers
- limits.download_kbps
- limits.upload_kbps

## Project Structure

```text
crates/
  core/
  api/
  cli/
  daemon/
ui/
```

## Development

### Rust

```bash
cargo check
cargo test
```

### UI

```bash
cd ui
npm install
npm run dev
```

Production build:

```bash
cd ui
npm run build
```

## Current Status

This project is actively evolving. Core downloading, metadata exchange, and control flows are implemented and used by both CLI and UI.

## License

MIT
