#!/bin/bash

# Deploy to GL.iNet Beryl Router at 192.168.8.1
# New Architecture: 
# - lighttpd serves frontend on port 3011
# - rust-wled-server serves API on port 3010

set -e

ROUTER_USER="root"
ROUTER_IP="192.168.8.1"
ROUTER_PATH="/etc/wled-server"

echo "🚀 Deploying WLED Server to Beryl Router (Lighttpd + API Backend)..."
echo ""

# Build Rust backend for ARM64 (Beryl is aarch64)
echo "🔨 Cross-compiling Rust backend for ARM64..."
rustup target add aarch64-unknown-linux-musl 2>/dev/null || true
cargo build --release --target aarch64-unknown-linux-musl
echo ""

# Build frontend locally with Bun (fast)
echo "🎨 Building frontend..."
cd frontend
bun install
bun run build
cd ..
echo ""

# Create directory on router if it doesn't exist
echo "📁 Ensuring directories exist on router..."
ssh ${ROUTER_USER}@${ROUTER_IP} "mkdir -p ${ROUTER_PATH}/frontend/build && mkdir -p /var/log/lighttpd"
echo ""

# Copy files to router
echo "📤 Copying binaries and runtime files to router..."
scp \
  target/aarch64-unknown-linux-musl/release/rust-wled-server \
  lighttpd.conf \
  presets.json \
  wled-server.init \
  wled-server-wrapper.sh \
  ${ROUTER_USER}@${ROUTER_IP}:${ROUTER_PATH}/

# Copy frontend build files
scp -r frontend/build/* ${ROUTER_USER}@${ROUTER_IP}:${ROUTER_PATH}/frontend/build/
echo "✅ Copy complete"
echo ""

# Deploy and configure services on router
echo "🔄 Configuring services on router..."
ssh ${ROUTER_USER}@${ROUTER_IP} << 'ENDSSH'
set -e

# 0. Create USB storage directories for programs and audio
echo "Setting up USB storage directories..."
mkdir -p /tmp/mountd/disk1_part1/wled-server/programs
mkdir -p /tmp/mountd/disk1_part1/wled-server/audio
chmod 755 /tmp/mountd/disk1_part1/wled-server
chmod 755 /tmp/mountd/disk1_part1/wled-server/programs
chmod 755 /tmp/mountd/disk1_part1/wled-server/audio
echo "USB storage directories ready"

# 1. Install and configure lighttpd
echo "Checking lighttpd installation..."
if ! opkg list-installed | grep -q "lighttpd"; then
    echo "Installing lighttpd..."
    opkg update
    opkg install lighttpd lighttpd-mod-rewrite lighttpd-mod-proxy
else
    echo "lighttpd already installed, skipping package installation"
fi

# Stop existing services if running
echo "Stopping existing services..."
/etc/init.d/lighttpd stop 2>/dev/null || true
/etc/init.d/wled-server stop 2>/dev/null || true

# Copy lighttpd config
echo "Configuring lighttpd..."
cp /etc/wled-server/lighttpd.conf /etc/lighttpd/lighttpd.conf

# 2. Configure Rust backend service
echo "Installing Rust backend service..."
cp /etc/wled-server/wled-server.init /etc/init.d/wled-server
chmod +x /etc/init.d/wled-server
cp /etc/wled-server/rust-wled-server /usr/bin/rust-wled-server
chmod +x /usr/bin/rust-wled-server

# Enable and start services
echo "Enabling and starting services..."
/etc/init.d/lighttpd enable
/etc/init.d/lighttpd start
/etc/init.d/wled-server enable
/etc/init.d/wled-server start

sleep 3

# Check if services are running
if pgrep -f "lighttpd" > /dev/null && pgrep -f "rust-wled-server" > /dev/null; then
    echo ""
    echo "✅ Both Lighttpd and WLED Server are running!"
    echo ""
    echo "📡 Services available at:"
    echo "  Frontend: http://192.168.8.1:3011"
    echo "  API:      http://192.168.8.1:3010/api/health"
else
    echo ""
    echo "❌ Failed to start one or more services"
    echo "   Check logs with: logread | grep -e lighttpd -e wled"
    exit 1
fi
ENDSSH

echo ""
echo "🎉 Deployment complete!"
echo ""
echo "Access your WLED server at: http://192.168.8.1:3011"
