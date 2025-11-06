#!/bin/bash

# Startup script for Docker container
# Runs both Rust backend and lighttpd frontend

echo "Starting WLED Server in Docker..."

# Start backend in background
echo "Starting Rust backend on port 3010..."
/app/backend &
BACKEND_PID=$!

# Start frontend in background (lighttpd serving static files with SPA support)
echo "Starting lighttpd frontend on port 3011..."
lighttpd -D -f /etc/lighttpd/lighttpd.conf &
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
