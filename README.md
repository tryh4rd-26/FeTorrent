# FeTorrent

FeTorrent is a BitTorrent engine and suite of client utilities implemented in the Rust programming language. It follows a modular architecture composed of a high-performance asynchronous core, a REST/WebSocket API, a background daemon, and both CLI and Web interfaces.

The project is designed for users who require a lightweight, robust, and extensible BitTorrent client that can be managed across diverse environments.

---

## Technical Architecture

FeTorrent is structured as a workspace of modular crates to ensure separation of concerns and optimal performance.

*   **`fetorrent-core`**: The foundational engine implementing the BitTorrent protocol, peer wire communication, and piece management using the `Tokio` asynchronous runtime.
*   **`fetorrent-api`**: An interface layer providing programmatic access to the engine via `Axum`, supporting both traditional REST endpoints and real-time WebSocket events.
*   **`fetorrent-daemon`**: A headless service that hosts the core engine and API, designed for continuous operation on servers or workstations.
*   **`fetorrent`**: A unified command-line interface for local and remote daemon management.

---

## Installation

### Cargo
FeTorrent can be installed from the crates.io registry:

```bash
cargo install fetorrent
```

### Manual Build
To build the project from the source repository:

1.  Clone the repository:
    ```bash
    git clone https://github.com/tryh4rd-26/FeTorrent
    ```
2.  Build and install all components:
    ```bash
    cd FeTorrent
    make install
    ```
    *Requirements: Rust stable toolchain, Node.js v18+, and npm.*

---

## Command Line Interface Usage

The `fetorrent` utility is the primary tool for interacting with the background daemon.

### Daemon Management
*   **`run`**: Launches the daemon process in the foreground.
*   **`kill`**: Terminates the running background daemon process.
*   **`log`**: Streams the internal daemon logs to the terminal for debugging and monitoring.
*   **`help`**: Displays the manual for the CLI or specific subcommands.

### Torrent Management
*   **`add [URI]`**: Ingests a new torrent into the session. Accepts Magnet URIs or file system paths to `.torrent` files.
*   **`list`**: Displays a summary table of all torrents in the current session, including transfer rates and status.
*   **`info [ID]`**: Provides detailed metadata and swarm statistics for a specific torrent.
*   **`pause [ID]`**: Halts all network activity for the specified torrent.
*   **`resume [ID]`**: Recommences network activity for a paused torrent.
*   **`remove [ID]`**: Deletes the torrent from the session. Use the `--delete` flag to remove the associated data from disk.
*   **`stats`**: Outputs global statistics for the daemon session, including aggregate speeds and total data transferred.

---

## Web Graphical User Interface

The Web GUI is served by the daemon and provides a visual dashboard for monitoring and configuration. It is accessible by default at `http://localhost:6977`.

*   **Real-time Monitoring**: Visual speed graphs and piece-map progress tracking.
*   **Activity Logging**: A diagnostic feed capturing piece-level verification events and tracker communications.
*   **Swarm Analysis**: Detailed views for file manifests, tracker health, and peer connectivity.
*   **Adaptive Theme**: Support for light and dark color schemes based on system preferences.

---

## Configuration

Configuration is managed via a `config.toml` file located in the standard user configuration directory (e.g., `~/.config/fetorrent/` on Unix-like systems). Key configurable parameters include network binding, default download directories, and global bandwidth limits.

---

## License

This project is licensed under the MIT License.
