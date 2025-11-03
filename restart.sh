#!/bin/bash

# WLED Server Restart Script
# Restarts both backend (Rust) and frontend (SvelteKit)

echo "🔄 Restarting WLED Server..."

# Kill existing processes
echo "⏹️  Stopping existing processes..."
pkill -f rust-wled-server
pkill -f "vite dev"

# Wait a moment for processes to clean up
sleep 1

# Start backend and frontend in parallel
echo "🚀 Starting Rust backend..."
cargo run 2>&1 &
BACKEND_PID=$!
echo "   Backend started (PID: $BACKEND_PID)"

echo "🎨 Starting frontend..."
cd frontend
~/.bun/bin/bun run dev 2>&1 &
FRONTEND_PID=$!
echo "   Frontend started (PID: $FRONTEND_PID)"
cd ..

# Wait a moment for both to initialize
sleep 2

echo ""
echo "✅ WLED Server is running!"
echo ""
echo "📡 Backend:  http://0.0.0.0:3010"
echo "🌐 Frontend: http://localhost:3011"
echo ""
echo "To view logs:"
echo "  Backend:  tail -f /tmp/wled-backend.log"
echo "  Frontend: tail -f /tmp/wled-frontend.log"
echo ""
