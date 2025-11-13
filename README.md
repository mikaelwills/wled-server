# WLED Server

A Rust-based intermediary server for controlling WLED devices with light sequencing capabilities, designed for live performance and Loopy Pro integration.

## Overview

This server provides a centralized control system for managing multiple WLED boards with timeline-based light programming and real-time performance playback.

## Key Features

**Board Management**
- Multi-board control via WebSocket connections
- Automatic reconnection and connection monitoring
- Board grouping for synchronized control
- Real-time status updates via Server-Sent Events

**Programming Interface**
- Timeline-based light sequencer with audio waveform visualization
- Drag-and-drop audio file support with MP3 compression
- Click-to-create cue markers with visual editing
- Per-board preset management
- Audio playback preview in browser

**Performance Mode**
- Large, touch-friendly button interface for live performance
- One-tap play/stop controls
- Electric border effects for visual feedback during playback
- Smart grid layout adapts to number of programs

**Light Control**
- E1.31 (sACN) protocol support for direct LED control
- WebSocket fallback for board-level commands
- Group-based lighting control
- Preset system for quick scene changes

**Technical Features**
- Actor-based architecture for reliable board management
- Audio compression (MP3 at 128kbps) for efficient storage
- OSC integration for external control (Loopy Pro)
- RESTful API for all operations
- CORS-enabled for cross-origin access


## Quick Start with Docker

The easiest way to run WLED Server is using Docker:

```bash
docker run -d \
  --name wled-server \
  -p 3010:3010 \
  -p 3011:3011 \
  -v ./wled-data:/app/data \
  -v ./wled-programs:/app/programs \
  -v ./wled-audio:/app/audio \
  -v ./wled-presets:/app/presets \
  --restart unless-stopped \
  YOUR_DOCKER_USERNAME/wled-server:latest
```

Then open your browser to `http://localhost:3011` and click "Add Board" to configure your WLED devices.

### Using Docker Compose

Create a `docker-compose.yml`:

```yaml
version: '3.8'
services:
  wled-server:
    image: YOUR_DOCKER_USERNAME/wled-server:latest
    container_name: wled-server
    ports:
      - "3010:3010"
      - "3011:3011"
    volumes:
      - ./wled-data:/app/data
      - ./wled-programs:/app/programs
      - ./wled-audio:/app/audio
      - ./wled-presets:/app/presets
    restart: unless-stopped
```

Run with:
```bash
docker-compose up -d
```

## Other Installation Methods

See `DEPLOYMENT.md` for native installation and deployment to embedded devices.
