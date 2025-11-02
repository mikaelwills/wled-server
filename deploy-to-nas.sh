#!/bin/bash

# Deploy to Synology NAS at 192.168.1.161
# Builds frontend locally with Bun (fast)
# Rust backend builds inside Docker on NAS (correct platform)

set -e

NAS_USER="mikael"
NAS_IP="192.168.1.161"
NAS_PATH="/volume1/docker/wled-server"

echo "🚀 Deploying WLED Server to NAS..."
echo ""

# Build Rust backend for Linux (cross-compile on Mac)
echo "🔨 Cross-compiling Rust backend for Linux..."
cargo build --release --target x86_64-unknown-linux-gnu
echo ""

# Build frontend locally with Bun (fast)
echo "🎨 Building frontend..."
cd frontend
bun install
bun run build
cd ..
echo ""

# Create directory on NAS if it doesn't exist
echo "📁 Ensuring directory exists on NAS..."
ssh ${NAS_USER}@${NAS_IP} "mkdir -p ${NAS_PATH}"
echo ""

# Check if boards.toml exists on NAS
echo "📋 Checking for existing boards.toml on NAS..."
if ssh ${NAS_USER}@${NAS_IP} "test -f ${NAS_PATH}/data/boards.toml"; then
    echo "   Found existing boards.toml - preserving it"
    EXCLUDE_BOARDS="--exclude=data/boards.toml"
else
    echo "   No boards.toml found - will copy local version"
    EXCLUDE_BOARDS=""
fi
echo ""

# Copy cross-compiled binary + pre-built frontend to NAS
echo "📤 Copying binaries and runtime files to NAS..."
COPYFILE_DISABLE=1 tar czf - \
  $EXCLUDE_BOARDS \
  target/x86_64-unknown-linux-gnu/release/rust-wled-server \
  frontend/build \
  frontend/package.json \
  Dockerfile \
  .dockerignore \
  docker-compose.yml \
  docker-start.sh \
  data \
  | ssh ${NAS_USER}@${NAS_IP} "cd ${NAS_PATH} && tar xzf - 2>/dev/null"
echo "✅ Copy complete"
echo ""

# Restart Docker containers on NAS
echo "🔄 Restarting Docker containers on NAS..."
ssh ${NAS_USER}@${NAS_IP} << 'ENDSSH'
cd /volume1/docker/wled-server
docker-compose down
docker-compose up -d --build
echo ""
echo "✅ Deployment complete!"
echo ""
echo "📊 Container status:"
docker-compose ps
echo ""
echo "📡 Services available at:"
echo "  Backend:  http://192.168.1.161:3010"
echo "  Frontend: http://192.168.1.161:3011"
echo ""
echo "📜 To view logs:"
echo "  ssh mikael@192.168.1.161"
echo "  cd /volume1/docker/wled-server"
echo "  docker-compose logs -f"
ENDSSH

echo ""
echo "🎉 Done!"
