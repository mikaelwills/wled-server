#!/bin/sh

# Wrapper script for WLED Rust Server
# Ensures environment variables are properly set before starting the binary

# Change to the correct directory so boards.toml is found
cd /etc/wled-server

# Set log level
RUST_LOG=info
export RUST_LOG

# Start the actual server
exec /usr/bin/rust-wled-server "$@"
