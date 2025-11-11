# Music-Precision Light Synchronization Issue

## Problem Statement

WLED boards become unresponsive approximately halfway through a strobe sequence in the light sequencer, requiring a power cycle to recover. This occurs when using rapid on/off cues synchronized to music (drum hits).

**Symptoms:**
- Boards respond to first ~8-10 rapid cues
- Mid-sequence failure (around 50% through strobe pattern)
- Complete unresponsiveness requiring power cycle
- Affects all 3 boards in GROUP simultaneously

## Root Cause Analysis

### Command Flooding

**Current Load:**
- **17 rapid on/off cues** over ~6.7 seconds
- **3 boards** in "GROUP" (One, Two, Three)
- Each cue triggers 3 parallel HTTP→WebSocket commands
- **Total: 51 commands in 6.7 seconds = ~7.6 requests/second sustained**

**Critical Timing:**
- Fastest interval: **148ms between cues**
- Most intervals: 240-600ms
- Each interval triggers 3 simultaneous board commands

### WLED's Documented Limits

From WLED documentation and community feedback:

> "For JSON/WebSockets... an update rate of 1/second is recommended, and I'd say about **10/second is the maximum** it will handle for longer periods of time."

> "Processing the 'i' request is relatively slow – a few request per second is probably the max possible without making WLED unstable."

**WebSocket Client Limit:**
- Maximum **4 concurrent WebSocket clients** per board
- 5th client causes existing connection to be dropped

### Why Boards Fail Mid-Sequence

**Buffer Overflow Mechanism:**
1. Preset command loads at cue 2 (heavy operation)
2. Before boards fully process preset, rapid on/off commands start firing
3. WLED's internal command buffer fills up
4. ESP8266/ESP32 can't process commands faster than ~10 Hz
5. After 8-10 accumulated commands, buffers overflow
6. WebSocket connection hangs or firmware becomes unresponsive
7. Only power cycle clears the stuck state

**Accumulation Effect:**
- First few commands work (buffer has space)
- Middle commands start backing up (buffer filling)
- By command 8-10, ESP chip can't keep up
- System locks up

## Current Architecture

### Playback System
```
Audio (Browser) → WaveSurfer.js Cues →
setTimeout Scheduler → Fetch API →
Rust HTTP Server → BoardActor →
WebSocket → WLED Board
```

**Timing Precision:**
- Uses `performance.now()` for microsecond accuracy
- Drift compensation on each cue
- Fire-and-forget for minimal latency
- Pre-scheduled via setTimeout (all cues queued at playback start)

### Cue Example Sequence
```json
Cue 2:  27.428s - preset 26 (strobe white)
Cue 3:  27.733s - off (305ms later)
Cue 4:  28.325s - on  (592ms later)
Cue 5:  28.628s - off (303ms later)
Cue 6:  29.173s - on  (545ms later)
Cue 7:  29.463s - off (290ms later)
Cue 8:  29.611s - on  (148ms later) ← Fastest interval
Cue 9:  30.186s - off (575ms later)
...17 cues total
```

### Current Message Format
**Power On:** `{"on":true,"tt":0}` (18 bytes)
**Power Off:** `{"on":false,"tt":0}` (19 bytes)
**Preset:** `{"ps":26,"tt":0}` (16 bytes)

## Solutions Evaluated

### ❌ Option 1: Backend Command Queue with Rate Limiting
**Approach:** Queue commands in Rust BoardActor, enforce 100ms minimum spacing

**Pros:**
- Guarantees WLED never receives >10 commands/sec
- Preserves all commands (nothing dropped)
- Boards stay responsive

**Cons:**
- **Adds latency accumulation** (0-100ms delay per command)
- 6.7s sequence could become 8-9s
- **Destroys precise drum-hit synchronization** ← FATAL FLAW

**Verdict:** REJECTED - Timing precision is non-negotiable for music sync

---

### ✅ Option 2: WebSocket Message Optimization
**Approach:** Minimize JSON payload size to reduce buffer pressure

**Changes:**
- Remove transition time field (`"tt":0`) - defaults to 0
- Use hex colors instead of RGB arrays (future optimization)

**Message Size Reduction:**
| Command | Current | Optimized | Savings |
|---------|---------|-----------|---------|
| Power | 18-19 bytes | 12-13 bytes | **33%** |
| Preset | 16 bytes | 10 bytes | **38%** |
| Effect | 27 bytes | 21 bytes | **22%** |

**Pros:**
- 25-30% bandwidth reduction
- Faster parsing on ESP chip
- Zero latency added
- Easy to implement

**Cons:**
- May not be sufficient alone
- Doesn't address fundamental rate limiting

**Verdict:** IMPLEMENTING - Low-hanging fruit, provides foundation

---

### ❌ Option 3: WLED Playlists
**Approach:** Pre-program entire sequence as WLED playlist

**WLED Playlist Structure:**
```json
{
  "playlist": {
    "ps": [26, 0, 26, 0],      // Preset IDs
    "dur": [3, 5, 3, 5],        // Duration in 0.1s units
    "transition": [0, 0, 0, 0],
    "repeat": 1
  }
}
```

**Critical Limitation:**
- **Timing precision: 100ms minimum** (tenths of seconds)
- Internal timer-based (no external audio sync)
- Max 100 playlist entries

**Example Impact:**
```
Current:  592ms interval → Playlist: 600ms (drift: +8ms)
Current:  303ms interval → Playlist: 300ms (drift: -3ms)
Over 17 cues: 100-300ms cumulative drift
```

**Pros:**
- Offloads timing to WLED firmware
- No network traffic during playback

**Cons:**
- **100ms precision insufficient** for drum-hit sync ← FATAL FLAW
- No audio synchronization capability
- Would require custom firmware for <100ms timing

**Verdict:** REJECTED - Designed for slow ambient effects, not musical timing

---

### 🔮 Option 4: Multiple WebSocket Connections Per Board
**Approach:** Open 2-3 WebSocket connections per board, distribute rapid commands

**Status:** Not yet explored

**Potential:**
- WLED supports up to 4 concurrent clients
- Could distribute burst loads across connections
- No latency added

**Risks:**
- More complex connection management
- Uses limited client slots
- May not solve ESP processing bottleneck

---

### 🔮 Option 5: Switch to UDP Protocol (DDP/Art-Net)
**Approach:** Use WLED's UDP realtime protocol instead of WebSocket/JSON

**Status:** Not yet explored

**Potential:**
- Designed for rapid DMX-like updates (44 Hz typical)
- True fire-and-forget (no acks, no buffers)
- Much lighter weight than JSON parsing

**Risks:**
- Major architecture change
- Different API (no simple on/off, uses pixel data)
- Requires implementing binary protocols

## Current Status

### Implemented
- ✅ Millisecond-accurate audio synchronization
- ✅ Fire-and-forget architecture (minimal latency)
- ✅ Per-board WebSocket connections with auto-reconnect

### In Progress
- 🔄 **WebSocket message optimization** (Option 2)

### Next Steps if Optimization Insufficient
1. Investigate UDP protocols (DDP for simple on/off)
2. Test multiple WebSocket connections
3. Consider hybrid approach (UDP for rapid sequences, WebSocket for configuration)

## Technical Constraints

### Non-Negotiable Requirements
- ✅ Millisecond timing precision (drum-hit accuracy)
- ✅ Fire-and-forget architecture (no latency)
- ✅ Current cue spacing must work unchanged
- ✅ No command dropping or batching

### WLED Hardware Limits
- ESP8266: Not recommended (limited processing power)
- ESP32: Better but still ~10 commands/sec max via WebSocket
- WiFi latency: 2.4GHz typical, subject to RF interference

## Notes

The current architecture (audio → cue triggers → WebSocket) is **already optimal** for precision musical timing. WLED's WebSocket/JSON API was not designed for rapid sequential updates at musical tempos. The solution may require moving to WLED's UDP protocols, which are designed for high-frequency lighting control.

---

**Last Updated:** 2025-11-09
**Status:** Active investigation - Testing message optimization
