# WLED Rust Server

## TODO
- Make per-board random patterns for effects (currently all boards show identical patterns):
  - Lightning: Each board should have independent random flash positions/timing
  - Sparkle: Each board should spawn sparks at different random positions
  - Puddles: Each board should have independent puddle spawning
  - Bursts: Each board should have independent burst positions

## ⚠️ IMPORTANT: DO NOT RESTART THE SERVER ⚠️
**Run `cargo build` to check compilation, but NEVER restart/kill the server process. The user will deploy with their deploy script.**

A Rust-based intermediary server for controlling WLED devices with light sequencing for Loopy Pro integration.

## Documentation
When the user refers to "documentation in SpaceNotes", use the spacenotes-mcp to access: `Development/WLED Server/WLED-rust-server documentation`

## Two Operating Modes

**Home Use** (WebSocket/HTTP API)
- Individual board control via WLED's native API
- WLED presets stored on each board
- Effects run on WLED itself (smooth transitions, ambient effects)
- Preset sync features for managing board presets
- User picks presets from what's available on each board

**Performance Mode** (E1.31/Effects Engine)
- Server-side effect rendering at 60fps
- Direct LED control via E1.31, bypasses WLED's effect engine
- Frame-perfect BPM sync for strobes
- Effect presets defined in `boards.toml` (not on WLED boards)
- Group-based, multi-board synchronized control
- API: `POST /effects/start` with `{ preset, bpm, target }`

These are separate systems serving different needs. Home use features (preset management, per-board control) must coexist with performance features (effects engine, E1.31 transport).


**Target:** Beryl router at `192.168.8.1`

**Beryl File Structure:**
```
/etc/wled-server/
├── data/
│   └── boards.toml          # Board config, groups, effect presets
├── presets/
│   └── *.json               # Home-use WLED presets
├── programs/                # Empty - programs stored on USB
├── audio/                   # Empty - audio stored on USB
└── frontend/build/          # Static frontend files

/tmp/mountd/disk1_part1/wled-server/   # USB STORAGE (persistent)
├── programs/                # Light sequencer programs (*.json)
└── audio/                   # Compressed audio files (*.mp3)
```

**Why USB?** Router's internal storage is limited. Programs and audio are stored on USB drive mounted at `/tmp/mountd/disk1_part1/`.

**⚠️ CRITICAL: Local vs Beryl boards.toml are DIFFERENT configs!**
- **Local** (`data/boards.toml`): Home boards (Desk, Squiiiiiiish, etc.)
- **Beryl** (`/etc/wled-server/data/boards.toml`): Performance boards (ONE-NINE)

**NEVER scp local boards.toml to Beryl** - it will overwrite the 9 performance boards!

**To edit Beryl config directly:**
```bash
ssh root@192.168.8.1 "vi /etc/wled-server/data/boards.toml"
ssh root@192.168.8.1 "/etc/init.d/wled-server restart"
```

**Presets can be synced (same on both):**
- `presets/` → Beryl: `/etc/wled-server/presets/`

**Environment Variables (set in init script):**
- `WLED_PROGRAMS_PATH=/tmp/mountd/disk1_part1/wled-server/programs`
- `WLED_AUDIO_PATH=/tmp/mountd/disk1_part1/wled-server/audio`

The server reads `boards.toml` at startup (no hot-reload). After updating effect presets, sync to Beryl and restart the server. Frontend fetches presets on page load.

## Development Workflow

**STRICT BUILD POLICY:**
- **ALWAYS BUILD LOCALLY** - Never build on target devices (NAS, router, etc.)
- Cross-compile Rust backend on development machine for target architecture
- Build frontend with Bun on development machine
- Only deploy pre-built binaries and assets to target devices
- Target devices (especially routers) cannot handle compilation workload

## Reference Resources

**WLED Source Code:** `~/Productivity/Development/esp32/WLED/`
- `wled00/FX.cpp` - All effect implementations with speed/timing formulas
- `wled00/FX.h` - Effect IDs and metadata
- Use this for researching effect timing formulas for BPM sync features



## Architecture Overview

The server uses the **Actor Pattern** for managing WLED boards:
- Each board is represented by a `BoardActor` that runs in its own tokio task
- Actors communicate via message passing using `mpsc` channels
- HTTP handlers send `BoardCommand` messages to actors
- Actors respond using oneshot channels for state queries
- Each actor maintains its own WebSocket connection and auto-reconnects on failure


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

### Loopy Pro Audio Muting Mechanism:

**Problem:** When triggering Loopy Pro via OSC, audio comes from Loopy (high-quality, uncompressed). Playing the same audio from the browser creates double playback and uses compressed versions.

**Solution:** Conditional audio playback with simulated progress tracking.

**Implementation Details:**

1. **Settings Storage** (`src/config.rs`):
   - `LoopyProConfig` struct includes `mute_audio: bool` field
   - Persisted to `config.toml` on disk
   - API endpoints: `GET/PUT /api/settings/loopy-pro`

2. **Playback Logic** (`frontend/src/routes/performance/+page.svelte:84-211`):
   ```javascript
   // Fetch mute setting from backend
   const settings = await fetch('/api/settings/loopy-pro');
   const muteAudio = settings.mute_audio || false;

   // Conditional playback
   if (!muteAudio) {
     await audio.play();  // Browser plays audio
   } else {
     // Silent mode - Loopy Pro plays audio via OSC
   }
   ```

3. **Progress Tracking** (lines 184-206):
   - **Normal mode:** Uses `audio.currentTime / audio.duration` from Audio element
   - **Muted mode:** Simulates progress using `performance.now()` and stored `program.audioDuration`
   - **Chain trigger:** Manually calls `audio.onended()` when simulated time reaches duration
   - Both modes use `requestAnimationFrame` for smooth 60fps progress bar updates

4. **Chain Auto-Play Compatibility:**
   - Chain mechanism relies on `audio.onended` event
   - When muted: Timer-based simulation triggers `onended` manually
   - Chains work identically in both playback modes

**User Flow:**
1. Settings page: Toggle "Mute App Audio" checkbox
2. Backend persists `mute_audio: true` to config.toml
3. Performance page: Fetches setting before each program play
4. If muted: OSC triggers Loopy Pro track, LED cues fire, progress simulated, chains work
5. If unmuted: Browser plays audio, LED cues fire, progress tracked, chains work

**Why This Design:**
- Allows preview/standalone use without Loopy Pro (browser audio)
- Avoids double playback when using Loopy Pro's superior audio
- Maintains chain functionality regardless of audio source
- Progress bars work identically in both modes

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

### Settings Management
- `GET /api/settings/loopy-pro` - Get Loopy Pro settings (IP, port, mute_audio)
- `PUT /api/settings/loopy-pro` - Update Loopy Pro settings (JSON: `{"ip": "192.168.1.100", "port": 7000, "mute_audio": true}`)

