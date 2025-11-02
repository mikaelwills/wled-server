# WLED Rust Server - Teaching Project

This is a RUST LEARNING EXERCISE. You are teaching the user Rust through building a WLED intermediary server.

## Development Workflow

**Testing Changes:** Use the restart script to quickly restart both frontend and backend:
```bash
./restart.sh
```
This will:
- Kill existing processes
- Start backend (Rust) on http://0.0.0.0:3000
- Start frontend (SvelteKit) on http://localhost:5173
- Both servers start in parallel for faster restarts

## Architecture Overview

The server uses the **Actor Pattern** for managing WLED boards:
- Each board is represented by a `BoardActor` that runs in its own tokio task
- Actors communicate via message passing using `mpsc` channels
- HTTP handlers send `BoardCommand` messages to actors
- Actors respond using oneshot channels for state queries
- Each actor maintains its own WebSocket connection and auto-reconnects on failure

## Teaching Structure:
1. Split implementation into **major phases**
2. Each phase contains **steps**
3. **Each step must be maximum 35 lines tall** (to fit on user's screen)
4. User will be in neovim in a terminal split, writing code themselves as you guide them

## Teaching Approach:
- **WAIT for user confirmation** before proceeding to next step
- Keep each instruction/code snippet within the 35-line limit
- Format: **Code snippet + brief explanation**
- User will ask about Rust concepts they don't understand before moving on
- Steps can build toward a testable milestone (don't need to be immediately testable)

## Current Implementation Status

### Core Modules:
- **actor.rs**: BoardActor implementation with async run loop
- **board.rs**: BoardState and BoardCommand definitions
- **config.rs**: TOML configuration loading (defined but not yet used)
- **main.rs**: Axum HTTP server with route handlers

### Implemented Features:
- ✓ Actor-based architecture with message passing
- ✓ WebSocket connection management per board
- ✓ Auto-reconnection logic (5-second delay)
- ✓ WebSocket keepalive (10-second ping interval, 10-20s disconnect detection)
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

### Key Rust Concepts Covered:
- **Ownership & Borrowing**: RwLock for shared state
- **Concurrency**: tokio async runtime, mpsc channels, oneshot channels
- **Pattern Matching**: Command handling, WebSocket message parsing
- **Type System**: Enums for commands, structs for state
- **Error Handling**: Result types, Box<dyn Error>
- **Traits**: Serialize/Deserialize with serde

## Phase Roadmap:

### Phase 1: Core Server Setup ✓ COMPLETED
- [x] Step 1: Initialize Rust project with Cargo
- [x] Step 2: Basic HTTP server with Axum
- [x] Step 3: Define BoardState struct
- [x] Step 4: Shared application state (Arc/RwLock/HashMap)
- [x] Step 5: Return JSON responses with test board
- [x] Step 6: Add POST endpoint for board control

### Phase 2: WebSocket Client to WLED ✓ COMPLETED
- [x] Step 7: Add WebSocket dependencies (tokio-tungstenite)
- [x] Step 8: Create WebSocket client connection to single board
- [x] Step 9: Parse incoming WLED state messages
- [x] Step 10: Update shared state from WebSocket messages
- [x] Step 11: Send control commands via WebSocket

### Phase 3: Actor Pattern Refactor ✓ COMPLETED
- [x] Step 12: Define BoardCommand enum with message types
- [x] Step 13: Create BoardActor struct with state encapsulation
- [x] Step 14: Implement actor run loop with tokio::select!
- [x] Step 15: Replace shared state with mpsc channels
- [x] Step 16: Handle reconnection logic within actor

### Phase 4: Advanced Features ✓ COMPLETED
- [x] Step 17: Add brightness control endpoint
- [x] Step 18: Add color control endpoint
- [x] Step 19: Add effect selection endpoint
- [x] Step 20: Implement GetState command with oneshot response

### Phase 5: Server-Sent Events (SSE) for Real-Time Updates ✓ COMPLETED
- [x] Step 21: Add SSE dependencies and create broadcast channel
- [x] Step 22: Extend BoardCommand with state broadcast events
- [x] Step 23: Implement SSE endpoint with Axum response stream
- [x] Step 24: Emit state changes from BoardActor to broadcast channel
- [x] Step 25: Handle client connections and disconnections

### Phase 6: Multi-Board & Configuration ✓ COMPLETED
- [x] Step 26: Load boards from boards.toml configuration
- [x] Step 27: Spawn multiple BoardActors from config
- [x] Step 28: Add board discovery/registration endpoint
- [ ] Step 29: Implement scenes (multi-board synchronized control) - SKIPPED FOR NOW
- [x] Step 30: Add error handling improvements (graceful config/binding failures)

### Phase 7: Web Interface (PWA) ✓ COMPLETED
- [x] Step 31: Create SvelteKit project with static adapter
- [x] Step 32: Configure CORS in Rust backend
- [x] Step 33: Network accessibility (0.0.0.0 binding, --host flag)
- [x] Step 34: Build board list UI with toggle switches
- [x] Step 35: Create WLED-style circular HSV color picker component
- [x] Step 36: Add brightness slider (0-255 range)
- [x] Step 37: Implement effects dropdown (186 effects, alphabetical)
- [x] Step 38: Add board registration form (fullscreen modal)
- [x] Step 39: Add board deletion with confirmation
- [x] Step 40: Dark mode theme and mobile-friendly layout
- [x] Step 41: Integrate SSE for real-time updates
- [x] Step 42: Board edit/rename functionality
- [x] Step 43: Board groups feature (frontend-only)
- [ ] Step 44: Add PWA manifest and service worker (PENDING)

### Phase 8: Light Sequencer - Loopy Pro Integration ✓ FRONTEND COMPLETE
- [x] Step 45: Create `/sequencer` route with WaveSurfer.js integration
- [x] Step 46: Implement drag-and-drop WAV file loading
- [x] Step 47: Add click-to-create marker functionality
- [x] Step 48: Build cue configuration controls (boards, preset, color, effect, brightness)
- [x] Step 49: Add program metadata inputs (song name, Loopy Pro track)
- [x] Step 50: Implement custom multi-select dropdown for boards/groups
- [x] Step 51: Save program to localStorage with validation
- [x] Step 52: Add cue label editing functionality
- [x] Step 53: Implement Clear Cues button with confirmation
- [x] Step 54: Implement audio compression (WebM/Opus at 64kbps, ~95% size reduction)
- [x] Step 55: Continuous program list view (all programs visible)
- [x] Step 56: Delete program functionality with event dispatcher fix
- [x] Step 57: Play button for browser audio preview
- [x] Step 58: UI refinements (dark mode buttons, compact spacing, inline cues header)
- [x] Step 59: Program playback with OSC/LED cue triggering (from parent page)

### Phase 9: Backend Program Storage (NEXT)
- [ ] Step 60: Add Program struct and JSON serialization in Rust
- [ ] Step 61: Implement file-based storage for programs (programs/*.json)
- [ ] Step 62: Add POST /programs endpoint (save program)
- [ ] Step 63: Add GET /programs endpoint (list all programs)
- [ ] Step 64: Add GET /programs/:id endpoint (get single program)
- [ ] Step 65: Add DELETE /programs/:id endpoint (delete program)
- [ ] Step 66: Add PUT /programs/:id endpoint (update program)
- [ ] Step 67: Update frontend to use API instead of localStorage
- [ ] Step 68: Add error handling and validation in backend

## Current Progress:
**Phase:** 8 - Light Sequencer ✓ FRONTEND COMPLETE | Phase 9 - Backend Storage (NEXT)
**Last Completed:** Full sequencer frontend with audio compression, program management, and playback
**Last Updated:** 2025-11-02
**Status:** Sequencer frontend complete with localStorage. Next: Rust backend API for program persistence.

### Light Sequencer - Complete Feature Set (Phase 8):
**✅ Core Functionality:**
- WaveSurfer.js integration with drag-and-drop WAV loading
- Click-to-create cue markers on waveform with draggable regions
- Full cue configuration (boards/groups, preset, color, effect, brightness)
- Audio compression (WebM/Opus at 64kbps, ~95% file size reduction)
- Program metadata (song name, Loopy Pro track number)
- Save/load programs to/from localStorage with validation
- Delete programs with confirmation dialog
- Continuous scrollable list view (all programs visible)
- Browser audio preview playback per program
- Program playback with OSC/LED cue triggering via parent page

**✅ UI/UX Improvements:**
- Dark mode interface with colored text buttons
- Compact, dense cue rows without dividers
- Inline cues dropdown header
- Loading indicator as program card during compression
- Play button controls WaveSurfer audio playback
- Balanced waveform padding
- Fixed delete button event handling

**📋 Next Phase: Backend Storage (Phase 9)**
- Rust REST API for program CRUD operations
- File-based or SQLite storage for programs
- Replace frontend localStorage with API calls
- Program JSON structure already defined and ready for backend

## Current Progress Summary:
**Main App:** Fully functional WLED control panel with groups support
**Sequencer:** Frontend complete with localStorage - ready for backend integration

### Recent Session (2025-11-02):
✅ **Light Sequencer - Frontend Complete:**

**Audio Compression Implementation:**
- Implemented MediaRecorder API to compress WAV to WebM/Opus at 64kbps
- Achieved ~95% file size reduction (e.g., 15MB → 0.74MB)
- Added loading spinner shown as program card during compression
- Existing programs remain visible while new file compresses
- Audio stored as base64 data URL in localStorage

**UI/UX Refinements:**
- Refactored to continuous program list view (removed card view)
- All programs visible simultaneously in scrollable list
- Dark mode buttons with colored text instead of colored backgrounds
- Reduced spacing in cue rows for more dense layout
- Removed divider lines between cue items
- Inline "Cues" dropdown header (removed redundant hint text)
- Fixed track input field width
- Balanced waveform top/bottom padding
- Reduced gaps between Save/Clear/Delete buttons

**Functionality Fixes:**
- Fixed Play button to play browser audio (WaveSurfer) instead of triggering Loopy Pro
- Changed from `playingProgramId === programId` to `isPlaying` state
- Fixed Delete button using proper Svelte event dispatcher (`on:delete` instead of `ondelete`)
- Delete now properly removes program from view and localStorage

**Program Data Structure (Ready for Backend):**
```json
{
  "id": "song-name-track-timestamp",
  "songName": "Song Title",
  "loopyProTrack": "06",
  "fileName": "audio.wav",
  "audioData": "data:audio/webm;codecs=opus;base64,...",
  "cues": [
    {
      "time": 18.26,
      "label": "Cue 1",
      "boards": ["board-id-1", "group-id-1"],
      "preset": 0,
      "color": "#ff0000",
      "effect": 0,
      "brightness": 255,
      "transition": 0
    }
  ],
  "createdAt": "2025-11-02T..."
}
```
- Programs stored as JSON array in localStorage (key: `light-programs`)
- Audio compressed to WebM/Opus at 64kbps (~95% size reduction)
- Validation: song name required, at least 1 cue, boards selection warned
- Unique ID format: `{sanitized-song-name}-{track}-{timestamp}`

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
│   │   ├── programs/              # TODO: Programs management (Phase 2)
│   │   ├── performance/           # TODO: Performance/trigger page (Phase 3)
│   │   └── +layout.ts             # Prerender config
│   └── lib/
│       └── ColorWheel.svelte      # HSV color picker component
├── .env                           # API URL configuration
└── svelte.config.js              # Static adapter config
```

### Connection Monitoring:

**WebSocket Keepalive Implementation:**
- **Ping Interval:** 10 seconds
- **Detection Window:** 10-20 seconds (ping fails on next attempt after disconnect)
- **Implementation:** `tokio::time::interval` with `Message::Ping(vec![])` frames
- **Failure Handling:** Marks board as disconnected, broadcasts status via SSE, triggers reconnection
- **Location:** src/actor.rs:93-106

**Tradeoff Analysis:**
- **10s interval:** 6 pings/minute/board = minimal overhead, 10-20s detection
- **Passive monitoring:** No overhead but only detects on next read (could be minutes)
- **1s interval:** 60 pings/minute/board = instant detection but 10x overhead

**Why Active Monitoring:**
TCP connections don't immediately close when devices power off. Without active pings, the actor won't detect the disconnection until it tries to read/write, which could take a long time if the board is idle.

### Current Limitations:
- **Sequencer Programs:** Currently localStorage-only (not synced across devices or persisted server-side)
- **Main App:** SSE not yet integrated in frontend (uses polling via fetchBoards)
- No PWA manifest/service worker (not installable as app)
- No graceful shutdown handling
- No logging/tracing infrastructure
- No Docker deployment setup
- No nginx reverse proxy configuration

### Phase 9 Backend Implementation Plan:

**Program Storage Structure:**
- File-based: Store each program as `programs/{program-id}.json`
- Or SQLite: Single database with programs table
- Audio data stored as base64 string (already compressed)

**Rust API Endpoints:**
```rust
POST   /programs         // Create new program
GET    /programs         // List all programs
GET    /programs/:id     // Get single program
PUT    /programs/:id     // Update program
DELETE /programs/:id     // Delete program
```

**Program Struct (Rust):**
```rust
#[derive(Serialize, Deserialize, Clone)]
struct Program {
    id: String,
    song_name: String,
    loopy_pro_track: String,
    file_name: String,
    audio_data: String,  // base64 WebM/Opus
    cues: Vec<Cue>,
    created_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct Cue {
    time: f64,
    label: String,
    boards: Vec<String>,
    preset: u8,
    color: String,
    effect: u8,
    brightness: u8,
    transition: u8,
}
```

**Frontend Migration:**
- Replace `localStorage.setItem()` with `fetch()` POST to `/programs`
- Replace `localStorage.getItem()` with `fetch()` GET from `/programs`
- Keep same JSON structure (already compatible)
