#!/bin/bash

# Deploy locally on Robert itself
# Run this from ~/Productivity/wled-server/ on Robert
# Builds release binary, copies to /opt/wled-server, restarts service
# NEVER overwrites runtime data (presets, boards.toml, programs, audio, history)

set -e

DEPLOY_PATH="/opt/wled-server"
SOURCE_PATH="$(cd "$(dirname "$0")" && pwd)"

cd "$SOURCE_PATH"

echo "Building frontend..."
cd frontend
bun install --frozen-lockfile 2>/dev/null || bun install
bun run build
cd ..
echo ""

echo "Building Rust binary (release)..."
source "$HOME/.cargo/env" 2>/dev/null || true
cargo build --release
echo ""

echo "Stopping wled-server..."
sudo systemctl stop wled-server 2>/dev/null || true

echo "Deploying binary..."
cp target/release/rust-wled-server ${DEPLOY_PATH}/
chmod +x ${DEPLOY_PATH}/rust-wled-server

echo "Deploying frontend..."
rsync -a --delete frontend/build/ ${DEPLOY_PATH}/frontend/build/

echo "Updating systemd service..."
sudo cp wled-server.service /etc/systemd/system/wled-server.service
sudo systemctl daemon-reload
sudo systemctl enable wled-server

echo "Starting wled-server..."
sudo systemctl start wled-server

sleep 3

if systemctl is-active --quiet wled-server; then
    echo ""
    echo "WLED Server is running!"
    echo ""
    echo "  Frontend: http://$(hostname -I | awk '{print $1}'):3010"
    echo "  API:      http://$(hostname -I | awk '{print $1}'):3010/api/health"
    echo ""
else
    echo ""
    echo "Service failed to start!"
    echo "Check logs: journalctl -u wled-server -n 50"
    exit 1
fi

echo "Deployment complete!"
