#!/bin/bash

# Deploy to Robert (Intel NUC10i3FNK) via Tailscale
# Syncs source to Robert, builds natively there (ALSA/audio needs Linux headers)
# Frontend built locally with Bun, synced as static files
# NEVER overwrites runtime data (presets, boards.toml, programs, audio, history)
#
# Prerequisites (one-time): run robert-sudo-setup.sh on Robert to:
#   - Make mikael own /opt/wled-server (no sudo for file ops)
#   - Allow passwordless sudo for systemctl wled-server commands
#   - Create wled system user with audio group
#   - Configure RT scheduling limits

set -e

ROBERT_USER="mikael"
ROBERT_IP="100.126.128.13"
DEPLOY_PATH="/opt/wled-server"
BUILD_PATH="/home/mikael/Productivity/wled-server"

echo "Checking connection to Robert..."
if ! ping -c 1 -W 2 ${ROBERT_IP} &> /dev/null; then
    echo "Cannot reach Robert at ${ROBERT_IP}"
    echo "Please connect to the correct network"
    exit 1
fi
echo "Robert is reachable"
echo ""

echo "Deploying WLED Server to Robert (Intel NUC)..."
echo ""

echo "Building frontend locally..."
cd frontend
bun install
bun run build
cd ..
echo ""

echo "Syncing Rust source to Robert..."
rsync -az \
    src/ ${ROBERT_USER}@${ROBERT_IP}:${BUILD_PATH}/src/
rsync -az \
    Cargo.toml Cargo.lock ${ROBERT_USER}@${ROBERT_IP}:${BUILD_PATH}/
rsync -az \
    .cargo/ ${ROBERT_USER}@${ROBERT_IP}:${BUILD_PATH}/.cargo/
echo "Source synced"
echo ""

echo "Building Rust binary on Robert..."
ssh ${ROBERT_USER}@${ROBERT_IP} << 'ENDSSH'
set -e
if ! command -v cargo &>/dev/null; then
    echo "Rust not installed. Installing..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
source "$HOME/.cargo/env"
cd ~/Productivity/wled-server
cargo build --release
echo "Build complete"
ENDSSH
echo ""

echo "Deploying to Robert..."
ssh ${ROBERT_USER}@${ROBERT_IP} "sudo systemctl stop wled-server 2>/dev/null || true"

ssh ${ROBERT_USER}@${ROBERT_IP} "cp ~/Productivity/wled-server/target/release/rust-wled-server /opt/wled-server/ && chmod +x /opt/wled-server/rust-wled-server"

rsync -az --delete \
    frontend/build/ \
    ${ROBERT_USER}@${ROBERT_IP}:/opt/wled-server/frontend/build/

scp -q wled-server.service ${ROBERT_USER}@${ROBERT_IP}:/tmp/wled-server.service
ssh ${ROBERT_USER}@${ROBERT_IP} "sudo cp /tmp/wled-server.service /etc/systemd/system/wled-server.service && rm /tmp/wled-server.service"

ssh ${ROBERT_USER}@${ROBERT_IP} "sudo systemctl daemon-reload && sudo systemctl enable wled-server && sudo systemctl start wled-server"

sleep 3

ssh ${ROBERT_USER}@${ROBERT_IP} << 'ENDSSH'
if systemctl is-active --quiet wled-server; then
    echo ""
    echo "WLED Server is running on Robert!"
    echo ""
    echo "  Frontend: http://100.126.128.13:3010"
    echo "  API:      http://100.126.128.13:3010/api/health"
    echo ""
    echo "Useful commands:"
    echo "  systemctl status wled-server"
    echo "  journalctl -u wled-server -f"
else
    echo ""
    echo "Service failed to start!"
    echo "Check logs: journalctl -u wled-server -n 50"
    exit 1
fi
ENDSSH

echo ""
echo "Deployment complete!"
echo "Access at: http://${ROBERT_IP}:3010"
