# WLED Rust Server

A Rust-based intermediary server for controlling WLED devices with light sequencing for Loopy Pro integration.

## NEXT STEPS - Code Cleanup Required

**E1.31 Implementation Cleanup:**
1. ✅ E1.31 raw implementation working (`src/transport/e131_raw.rs`)
2. ❌ **Remove legacy E1.31 sacn library code** (`src/transport/e131.rs`) - no longer used, incompatible with WLED
3. ❌ **Remove sacn dependency** from `Cargo.toml` - not needed, using raw implementation
4. ❌ **Clean up per-board E1.31** in `src/actor.rs` - currently only groups use E1.31, per-board code is unused
5. ❌ **Remove E1.31 config** from `src/config.rs` - `E131Config` struct and board-level fields not used
6. ❌ **Update `boards.toml`** - Remove `e131_enabled` and `e131_universe` fields (not used)
7. ❌ **Remove WebSocket fallback** from `src/group.rs` - already removed, verify no references remain
8. ❌ **Test group commands** after cleanup to ensure nothing breaks

**Other Cleanup:**
- Review and remove any dead code warnings from compiler
- Update documentation to reflect E1.31-only group control
- Consider making group board IPs configurable instead of hardcoded in main.rs

## Development Workflow

**STRICT BUILD POLICY:**
- **ALWAYS BUILD LOCALLY** - Never build on target devices (NAS, router, etc.)
- Cross-compile Rust backend on development machine for target architecture
- Build frontend with Bun on development machine
- Only deploy pre-built binaries and assets to target devices
- Target devices (especially routers) cannot handle compilation workload



## Architecture Overview

The server uses the **Actor Pattern** for managing WLED boards:
- Each board is represented by a `BoardActor` that runs in its own tokio task
- Actors communicate via message passing using `mpsc` channels
- HTTP handlers send `BoardCommand` messages to actors
- Actors respond using oneshot channels for state queries
- Each actor maintains its own WebSocket connection and auto-reconnects on failure

## Implementation Status

### Core Modules:
- **actor.rs**: BoardActor implementation with async run loop
- **board.rs**: BoardState and BoardCommand definitions
- **config.rs**: TOML configuration loading (defined but not yet used)
- **main.rs**: Axum HTTP server with route handlers

### Implemented Features:
- ✓ Actor-based architecture with message passing
- ✓ WebSocket connection management per board
- ✓ Auto-reconnection logic (5-second delay)
- ✓ WebSocket keepalive (5-second ping interval, 5-10s disconnect detection)
- ✓ Toggle power endpoint (returns full BoardState after toggle)
- ✓ Brightness control endpoint
- ✓ Color control endpoint (RGB)
- ✓ Effect selection endpoint
- ✓ List boards endpoint (queries actor state via oneshot)
- ✓ Server-Sent Events (SSE) for real-time updates
- ✓ Multi-board configuration from boards.toml
- ✓ Dynamic board registration/deletion endpoints with persistence
- ✓ Board edit/rename functionality (PUT endpoint)
- ✓ CORS enabled for cross-origin requests (including DELETE)
- ✓ Network-accessible server (0.0.0.0 binding)
- ✓ Graceful fallback for unreachable boards in list_boards
- ✓ **Per-Board Presets** - Board-specific preset management
  - GET /board/:id/presets endpoint fetches actual presets from each WLED board
  - Frontend shows only presets that exist on that specific board
  - Clear indication when boards need syncing ("No presets - click Sync")
  - Prominent "Sync Presets" button for boards without presets
  - Automatic preset fetching when boards connect
  - Cached preset lists per board in frontend state
- ✓ SvelteKit frontend with static adapter
- ✓ WLED-style HSV color wheel component with working selector circle
- ✓ Complete effects list (186 effects, alphabetically sorted)
- ✓ Touch-friendly mobile interface
- ✓ Dark mode UI theme
- ✓ **Board Groups** - Frontend-only feature for controlling multiple boards simultaneously
  - Create groups with multiple member boards
  - Group controls (power, color, brightness, effect) affect all members
  - Groups persist in localStorage
  - Toggle switch reflects group's actual LED color
  - Group state derived from member board states (all ON = group ON)
- ✓ **Light Sequencer** - Timeline-based light programming with Loopy Pro integration
  - WaveSurfer.js integration with drag-and-drop WAV loading
  - Click-to-create cue markers on waveform with draggable regions
  - Full cue configuration (boards/groups, preset, color, effect, brightness)
  - Audio compression (WebM/Opus at 64kbps, ~95% size reduction)
  - Program metadata (song name, Loopy Pro track number)
  - Save/load programs to/from localStorage
  - Delete programs with confirmation dialog
  - Browser audio preview playback
  - Program playback with OSC/LED cue triggering

## Technical Details:

### Actor Pattern Implementation:
```rust
// Communication flow:
HTTP Request → Route Handler → mpsc::Sender<BoardCommand>
  → BoardActor.run() → WebSocket → WLED Device

// State query flow:
HTTP Request → BoardCommand::GetState(oneshot::Sender)
  → BoardActor → oneshot::Receiver → JSON Response
```

### Dependencies:

**Backend (Rust):**
- **tokio**: Async runtime with full features
- **axum**: Web framework (v0.7)
- **tokio-tungstenite**: WebSocket client (v0.21)
- **serde/serde_json**: Serialization/deserialization
- **toml**: Configuration file parsing
- **tower-http**: CORS middleware
- **tokio-stream**: SSE stream support

**Frontend (SvelteKit):**
- **@sveltejs/kit**: Framework
- **@sveltejs/adapter-static**: Static site generation
- **svelte**: Component framework
- **vite**: Build tool
- **typescript**: Type safety
- **bun**: JavaScript runtime and package manager

### Frontend Architecture:
```
frontend/
├── src/
│   ├── routes/
│   │   ├── +page.svelte           # Main UI (board controls)
│   │   ├── sequencer/
│   │   │   └── +page.svelte       # Light sequencer editor
│   │   └── +layout.ts             # Prerender config
│   └── lib/
│       └── ColorWheel.svelte      # HSV color picker component
├── .env                           # API URL configuration
└── svelte.config.js              # Static adapter config
```

### Connection Monitoring:

**WebSocket Keepalive Implementation:**
- **Ping Interval:** 5 seconds
- **Detection Window:** 5-10 seconds (ping fails on next attempt after disconnect)
- **Implementation:** `tokio::time::interval` with `Message::Ping(vec![])` frames
- **Failure Handling:** Marks board as disconnected, broadcasts status via SSE, triggers reconnection
- **Location:** src/actor.rs:138-140

**Tradeoff Analysis:**
- **5s interval:** 12 pings/minute/board = minimal overhead, 5-10s detection
- **Passive monitoring:** No overhead but only detects on next read (could be minutes)
- **1s interval:** 60 pings/minute/board = instant detection but 5x overhead

**Why Active Monitoring:**
TCP connections don't immediately close when devices power off. Without active pings, the actor won't detect the disconnection until it tries to read/write, which could take a long time if the board is idle.

## API Endpoints

### Board Management
- `GET /api/boards` - List all boards and groups with current state
- `POST /api/boards` - Register new board
- `PUT /api/boards/:id` - Update board (ID or IP)
- `DELETE /api/boards/:id` - Delete board

### Board Control
- `POST /api/board/:id/power` - Toggle power (JSON: `{"on": true, "transition": 0}`)
- `POST /api/board/:id/brightness` - Set brightness (JSON: `{"brightness": 128, "transition": 0}`)
- `POST /api/board/:id/color` - Set RGB color (JSON: `{"r": 255, "g": 255, "b": 255, "transition": 0}`)
- `POST /api/board/:id/effect` - Set effect (JSON: `{"effect": 0, "transition": 0}`)
- `POST /api/board/:id/speed` - Set effect speed (JSON: `{"speed": 128, "transition": 0}`)
- `POST /api/board/:id/intensity` - Set effect intensity (JSON: `{"intensity": 128, "transition": 0}`)
- `POST /api/board/:id/preset` - Apply preset (JSON: `{"preset": 1, "transition": 0}`)
- `POST /api/board/:id/led-count` - Set LED count (JSON: `{"led_count": 30}`)
- `POST /api/board/:id/reset-segment` - Reset segment to defaults
- `GET /api/board/:id/presets` - Get actual presets from WLED board
- `POST /api/board/:id/presets/sync` - Sync global presets to board

### Group Management
- `POST /api/groups` - Create group
- `PUT /api/groups/:id` - Update group
- `DELETE /api/groups/:id` - Delete group
- `POST /api/group/:id/power` - Set group power
- `POST /api/group/:id/brightness` - Set group brightness
- `POST /api/group/:id/color` - Set group color
- `POST /api/group/:id/effect` - Set group effect
- `POST /api/group/:id/preset` - Apply preset to group

### Preset Management
- `GET /api/presets` - List all global presets
- `POST /api/presets` - Save new preset
- `GET /api/presets/:id` - Get specific preset
- `DELETE /api/presets/:id` - Delete preset

### Program Management (Sequencer)
- `GET /api/programs` - List all programs
- `POST /api/programs` - Save new program
- `GET /api/programs/:id` - Get specific program
- `PUT /api/programs/:id` - Update program
- `DELETE /api/programs/:id` - Delete program

### Real-time & Utilities
- `GET /api/events` - Server-Sent Events stream for real-time updates
- `POST /api/osc` - Send OSC message
- `GET /api/health` - Health check

### Known Limitations:
- Sequencer programs stored in localStorage only (not synced across devices)
- SSE available on backend but frontend uses polling
- No PWA manifest/service worker
- No graceful shutdown handling
- No logging/tracing infrastructure
