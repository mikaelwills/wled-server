#!/bin/bash

# Startup script for Docker container
# Runs both Rust backend and SvelteKit frontend

echo "Starting WLED Server in Docker..."

# Start backend in background
echo "Starting Rust backend on port 3000..."
/app/backend &
BACKEND_PID=$!

# Start frontend in background (serving static files with SPA support)
echo "Starting SvelteKit frontend on port 3001..."
python3 /app/spa-server.py 3001 /app/frontend/build &
FRONTEND_PID=$!

# Function to handle shutdown
shutdown() {
    echo "Shutting down services..."
    kill $BACKEND_PID $FRONTEND_PID
    wait $BACKEND_PID $FRONTEND_PID
    exit 0
}

# Trap termination signals
trap shutdown SIGTERM SIGINT

echo "Both services started:"
echo "  Backend PID: $BACKEND_PID"
echo "  Frontend PID: $FRONTEND_PID"
echo "Ready!"

# Wait for both processes
wait $BACKEND_PID $FRONTEND_PID
