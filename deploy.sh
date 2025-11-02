#!/bin/bash

# Deployment script for remote machine (.161)
# Usage: ./deploy.sh

set -e

echo "🚀 Deploying WLED Server..."
echo ""

# Pull latest changes
echo "📥 Pulling latest code from git..."
git pull
echo ""

# Stop existing containers
echo "⏹️  Stopping existing containers..."
docker-compose down
echo ""

# Rebuild and start containers
echo "🔨 Building and starting containers..."
docker-compose up -d --build
echo ""

# Wait a moment for services to start
sleep 3

# Show status
echo "✅ Deployment complete!"
echo ""
echo "📊 Container status:"
docker-compose ps
echo ""

echo "📡 Services should be available at:"
echo "  Backend:  http://$(hostname -I | awk '{print $1}'):3000"
echo "  Frontend: http://$(hostname -I | awk '{print $1}'):3001"
echo ""

echo "📜 To view logs:"
echo "  docker-compose logs -f"
echo ""
