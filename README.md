# FeTorrent: Technical Specification and Reference Manual

FeTorrent is a high-performance, asynchronous BitTorrent engine implemented in Rust. It utilizes a distributed architecture consisting of a headless daemon, a REST/WebSocket API layer, and multiple interface controllers (CLI and Web Dashboard).

## Technical Architecture

The system is partitioned into five primary workspace members:

- **fetorrent-core**: The protocol implementation layer. Handles piece management, swarm orchestration, and peer communication logic. Implements BEP 3 (BitTorrent Core), BEP 9 (Metadata Discovery), and BEP 10 (Extensions).
- **fetorrent-api**: The communication gateway. Utilizes the Axum framework to provide a stateless REST API for lifecycle management and a bi-directional WebSocket interface for real-time telemetry.
- **fetorrent-daemon**: The central supervisor process. Manages persistent storage, background trackers, and the overall lifecycle of active torrent tasks.
- **fetorrent-cli**: The primary administrative interface. Provides granular control over the daemon and high-density performance visualization.
- **fetorrent-ui**: A centralized monitoring dashboard built with React and Tailwind CSS, providing a real-time reactive view of the engine's state.

## Core Protocols and Compliance

- **BEP 0003**: Standard BitTorrent 1.0 protocol.
- **BEP 0009**: Extension for handling metadata files (Magnet link support).
- **BEP 0010**: Extension Protocol for peer-level capability negotiation.
- **Persistence**: State and configuration are maintained in standard TOML format within the host OS's designated configuration directory (e.g., `~/.config/fetorrent/`).

## Installation

Ensure the Rust toolchain (v1.75+) is available on the system path.

```bash
# Clone the repository and install all binary components
git clone https://github.com/tryhard/FeTorrent
cd FeTorrent
make install
```

## Command-Line Interface Reference

All commands support the `--json` flag for machine integration and can be configured via the `--url` flag to target remote daemons.

### Lifecycle Management

#### Starting the Daemon
Initializes the background engine. By default, the server binds to `127.0.0.1:6977`.

```bash
# Start in the background (Daemonized)
fetorrent start

# Start in the foreground for debugging
fetorrent run --foreground
```

#### Stopping the Daemon
Terminates the background process by targeting its specific listening port.

```bash
fetorrent kill
```

### Transfer Operations

#### Adding Torrents
Supports magnet URIs and local `.torrent` file paths.

```bash
# Add via Magnet link
fetorrent add "magnet:?xt=urn:btih:..."

# Add via local file
fetorrent add /path/to/linux.torrent --dir ~/Downloads
```

#### Controlling Transfers
Manage individual task states by ID.

```bash
# Pause a specific download
fetorrent pause 1

# Resume a paused download
fetorrent resume 1

# Remove a torrent from the queue (does not delete downloaded data)
fetorrent remove 1
```

### Monitoring and Telemetry

#### Listing Active Tasks
Displays a real-time dashboard of all active swarm metrics.

```bash
# Standard table view
fetorrent list

# Live-updating watch mode (1s interval)
fetorrent list --watch
```

#### Detailed Inspection
Provides exhaustive metadata about a specific torrent task.

```bash
fetorrent info 1
```

#### Viewing Logs
Direct access to the daemon's internal event log.

```bash
# Show last 100 lines
fetorrent log --lines 100

# Follow logs in real-time (Tail)
fetorrent log --live
```

#### System Statistics
High-level overview of global network performance and throughput.

```bash
fetorrent stats
```

## Security and Process Boundary

The FeTorrent daemon binds to the loopback interface by default to prevent unauthorized network surface exposure. All inter-process communication (IPC) is handled via HTTP/WebSocket with serialized JSON payloads to maintain a strict boundary between the engine core and its consumers.
