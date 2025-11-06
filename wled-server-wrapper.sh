#!/bin/sh

# Wrapper script for WLED Rust Server
# Ensures environment variables are properly set before starting the binary

# Change to the correct directory so boards.toml is found
cd /etc/wled-server

# Set log level
RUST_LOG=info
export RUST_LOG

# Set storage paths for programs and audio on USB storage
# USB storage is auto-mounted at /tmp/mountd/disk1_part1 by the router
WLED_PROGRAMS_PATH=/tmp/mountd/disk1_part1/wled-server/programs
WLED_AUDIO_PATH=/tmp/mountd/disk1_part1/wled-server/audio
export WLED_PROGRAMS_PATH
export WLED_AUDIO_PATH

# Start the actual server
exec /usr/bin/rust-wled-server "$@"
