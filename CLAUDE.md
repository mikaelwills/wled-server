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
- ✓ Toggle power endpoint
- ✓ Brightness control endpoint
- ✓ Color control endpoint (RGB)
- ✓ Effect selection endpoint
- ✓ List boards endpoint (queries actor state via oneshot)
- ✓ Server-Sent Events (SSE) for real-time updates
- ✓ Multi-board configuration from boards.toml
- ✓ Dynamic board registration/deletion endpoints with persistence
- ✓ CORS enabled for cross-origin requests (including DELETE)
- ✓ Network-accessible server (0.0.0.0 binding)
- ✓ Graceful fallback for unreachable boards in list_boards
- ✓ SvelteKit frontend with static adapter
- ✓ WLED-style HSV color wheel component
- ✓ Complete effects list (186 effects, alphabetically sorted)
- ✓ Touch-friendly mobile interface
- ✓ Dark mode UI theme

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

### Phase 7: Web Interface (PWA) ✓ MOSTLY COMPLETED
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
- [ ] Step 41: Integrate SSE for real-time updates (PENDING)
- [ ] Step 42: Add PWA manifest and service worker (PENDING)

## Current Progress:
**Phase:** 7 - Web Interface (PWA) ✓ COMPLETED (with known issue)
**Last Completed:** SSE integration, disconnected board UI, timeout fixes
**Last Updated:** 2025-11-01
**Next:** Debug slow disconnection detection (still taking >1min despite 5s timeout)

### Recent Session (2025-11-01):
✅ **Completed:**
- Fixed SSE double-formatting bug (was prepending "data: " twice)
- Integrated SSE for real-time board status updates
- Added disabled state for disconnected boards (grayed out controls, hidden toggle)
- Fixed shared broadcast channel architecture (no more per-client channels)
- Added 5-second timeouts to all WebSocket write operations
- Removed debug console.log statements from SSE handler

⚠️ **Known Issue:**
- Disconnection detection still takes >1 minute despite 5-second timeout on ping writes
- Expected: 10s (ping interval) + 5s (timeout) = 15 seconds max
- Actual: Still ~100 seconds
- Hypothesis: TCP connection not actually "dead" until OS timeout, write doesn't fail even with timeout
- **TODO Tomorrow:** Investigate alternative detection methods (read timeout, TCP_USER_TIMEOUT socket option, or reduce ping interval to 5s)

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
│   │   ├── +page.svelte       # Main UI (board controls)
│   │   └── +layout.ts         # Prerender config
│   └── lib/
│       └── ColorWheel.svelte  # HSV color picker component
├── .env                       # API URL configuration
└── svelte.config.js          # Static adapter config
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
- SSE not yet integrated in frontend (uses polling via fetchBoards)
- No PWA manifest/service worker (not installable as app)
- No graceful shutdown handling
- No logging/tracing infrastructure
- No Docker deployment setup
- No nginx reverse proxy configuration
