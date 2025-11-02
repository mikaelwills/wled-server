#!/bin/bash

# Deploy to Synology NAS at 192.168.1.161
# Syncs code and restarts Docker containers

set -e

NAS_USER="mikael"
NAS_IP="192.168.1.161"
NAS_PATH="/volume1/docker/wled-server"

echo "🚀 Deploying WLED Server to NAS..."
echo ""

# Create directory on NAS if it doesn't exist
echo "📁 Ensuring directory exists on NAS..."
ssh ${NAS_USER}@${NAS_IP} "mkdir -p ${NAS_PATH}"
echo ""

# Sync files to NAS (excluding build artifacts and git)
echo "📦 Syncing files to NAS..."
rsync -avz --progress \
  --exclude 'target/' \
  --exclude 'node_modules/' \
  --exclude 'frontend/.svelte-kit/' \
  --exclude 'frontend/build/' \
  --exclude '.git/' \
  --exclude '*.log' \
  --exclude '.DS_Store' \
  --exclude '.claude/' \
  ./ ${NAS_USER}@${NAS_IP}:${NAS_PATH}/
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
echo "  Backend:  http://192.168.1.161:3000"
echo "  Frontend: http://192.168.1.161:3001"
echo ""
echo "📜 To view logs:"
echo "  ssh mikael@192.168.1.161"
echo "  cd /volume1/docker/wled-server"
echo "  docker-compose logs -f"
ENDSSH

echo ""
echo "🎉 Done!"
