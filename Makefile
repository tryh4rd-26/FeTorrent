.PHONY: all dev build build-ui test dist

all: build-ui build

# Run the daemon in development mode (API on localhost:6977)
dev:
	cargo run -p fetorrent-daemon

# Build the Rust workspace in release mode
build:
	cargo build --release

# Build the React frontend
build-ui:
	cd ui && npm install && npm run build

# Run unit and integration tests
test:
	cargo test --workspace
	cargo clippy --workspace -- -D warnings

# Build the final distribution payload locally
dist: build-ui build
	mkdir -p dist
	cp target/release/fetorrent dist/
	cp target/release/fetorrent-daemon dist/
	cp config.example.toml dist/
	cp -r ui/dist dist/ui
	@echo "Distribution ready in ./dist"

# Install globally to user space
install: build-ui build
	mkdir -p ~/.cargo/bin
	install -m 755 target/release/fetorrent ~/.cargo/bin/
	install -m 755 target/release/fetorrent-daemon ~/.cargo/bin/
	mkdir -p ~/.local/share/fetorrent/ui
	cp -r ui/dist/* ~/.local/share/fetorrent/ui/
	@echo "FeTorrent installed successfully!"
	@echo "Binaries are in ~/.cargo/bin"
	@echo "UI is served from ~/.local/share/fetorrent/ui"
