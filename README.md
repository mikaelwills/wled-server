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


## Getting Started

See `DEPLOYMENT.md` for installation and deployment instructions.
