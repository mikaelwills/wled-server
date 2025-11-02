# Loopy Pro OSC Integration Documentation

## Overview

This document describes how the WLED Rust server integrates with Loopy Pro via OSC (Open Sound Control) to enable synchronized control of audio tracks and LED lighting.

## Architecture

```
Frontend (SvelteKit)
    ↓ HTTP POST /osc
Rust Server (Proxy)
    ↓ UDP OSC
Loopy Pro (iPad)
    → Plays/stops audio tracks
```

## Loopy Pro OSC Configuration

**Location:** Loopy Pro Settings → Control Settings → OSC

**Server Settings:**
- **Protocol:** UDP (default)
- **Port:** 9595
- **IP Address:** 192.168.1.242 (iPad IP on local network)
- **Mode:** Server (receives OSC messages)

**Important Notes:**
- Loopy Pro can **receive** OSC messages to control playback
- Loopy Pro cannot **send** arbitrary OSC messages (only feedback)
- For bidirectional communication with control surfaces, use TCP Client + Framing OSC 1.1

## OSC Message Format

### PlayStop Command

**Address:** `/PlayStop/[track_number]`

**Arguments:** None (empty)

**Effect:** Toggles play/stop state of the specified track

**Examples:**
- `/PlayStop/01` - Toggle track 1
- `/PlayStop/06` - Toggle track 6
- `/PlayStop/10` - Toggle track 10

### Track Numbers

- Zero-padded two digits (01-99)
- Track numbering starts at 01 (not 00)
- Must match your Loopy Pro track configuration

## Implementation Details

### Rust Server (Backend)

**File:** `src/main.rs`

**Dependencies:**
```toml
rosc = "0.10"  # OSC encoding/decoding
```

**Endpoint:** `POST /osc`

**Request Body:**
```json
{
  "address": "/PlayStop/06"
}
```

**Response:** `200 OK` on success

**Code:**
```rust
#[derive(Deserialize)]
struct OscRequest {
    address: String,
}

async fn send_osc(Json(payload): Json<OscRequest>) -> Result<StatusCode, StatusCode> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let packet = rosc::encoder::encode(&rosc::OscPacket::Message(rosc::OscMessage {
        addr: payload.address,
        args: vec![],
    }))
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    socket
        .send_to(&packet, "192.168.1.242:9595")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}
```

**Why UDP from Rust?**
- Browsers cannot send raw UDP/TCP packets
- Rust server acts as proxy between HTTP (frontend) and UDP (Loopy Pro)
- Simple, minimal implementation for one-way commands

### Frontend (SvelteKit)

**File:** `frontend/src/lib/osc.ts`

**Environment Variables:**
```env
PUBLIC_API_URL=http://192.168.1.111:3000
```

**OSC Client:**
```typescript
import { PUBLIC_API_URL } from '$env/static/public';

export async function sendOSC(address: string) {
    await fetch(`${PUBLIC_API_URL}/osc`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ address })
    });
}
```

**Usage Example:**
```typescript
// Play/stop track 6 and set LED to purple
function triggerLoopyAndLED(boardId: string, oscAddress: string, r: number, g: number, b: number) {
    // Send OSC to Loopy Pro
    sendOSC(oscAddress);

    // Set LED color
    setColor(boardId, r, g, b);
}

// In component
<button on:click={() => triggerLoopyAndLED('mikaels-bed', '/PlayStop/06', 255, 0, 255)}>
    ▶ Play Track 6 + Purple Lights
</button>
```

## Network Configuration

### Required IP Addresses

| Device | IP Address | Port | Protocol |
|--------|-----------|------|----------|
| iPad (Loopy Pro) | 192.168.1.242 | 9595 | UDP (OSC Server) |
| Rust Server | 192.168.1.111 | 3000 | HTTP |
| Frontend Dev Server | 192.168.1.111 | 5173 | HTTP |

### Firewall Considerations

- UDP port 9595 must be open on iPad
- Devices must be on same local network
- No internet connection required

## Limitations

### What Loopy Pro Cannot Do

❌ **Send OSC messages on track events** - Loopy Pro cannot automatically send OSC when tracks start/stop
❌ **HTTP requests** - No built-in HTTP client
❌ **URL schemes** - Cannot trigger iOS shortcuts directly
❌ **MIDI Designer alternative needed** - For automatic triggering based on Loopy Pro events

### Current Solution

✅ **Manual triggering** - User clicks button in frontend to trigger both Loopy Pro and LEDs simultaneously
✅ **Synchronized control** - Single button controls both audio and lighting
✅ **Network accessible** - Works from any device on local network

## Alternative Approaches Investigated

### 1. MIDI Designer Bridge (Not Used)
```
Loopy Pro → MIDI → MIDI Designer → URL Scheme → iOS Shortcut → HTTP → Rust
```
- Too complex for manual triggering use case
- Would enable automatic LED changes based on Loopy Pro events
- Requires third-party app (MIDI Designer)

### 2. Network MIDI (Not Used)
```
Loopy Pro → Network MIDI → rtpmidid on router → Rust MIDI handler
```
- Apple-proprietary RTP-MIDI protocol
- Requires daemon on router
- Not robust for GL.iNet router environment

### 3. OSC WebSocket (Attempted, Failed)
```
Frontend → osc-js (WebSocket) → Loopy Pro
```
- Loopy Pro doesn't support WebSocket OSC connections
- Only supports native UDP/TCP OSC
- Browsers cannot send raw UDP packets

### 4. Current HTTP → UDP Proxy (✅ Implemented)
```
Frontend → HTTP → Rust Server → UDP OSC → Loopy Pro
```
- Simple and minimal
- Works reliably
- No additional dependencies

## Testing

### Test OSC Connection

From command line (requires `oscsend` tool):
```bash
oscsend 192.168.1.242 9595 /PlayStop/06
```

### Test HTTP Endpoint

```bash
curl -X POST http://192.168.1.111:3000/osc \
  -H "Content-Type: application/json" \
  -d '{"address": "/PlayStop/06"}'
```

### Expected Behavior

1. Track 6 in Loopy Pro should start playing (if stopped)
2. Or stop playing (if already playing)
3. No error messages in Rust server console

## Troubleshooting

### OSC messages not received by Loopy Pro

1. **Check Loopy Pro OSC server is enabled**
   - Settings → Control Settings → OSC → Toggle ON

2. **Verify IP address**
   - iPad Settings → Wi-Fi → Check IP address
   - Update `src/main.rs` if IP changed

3. **Confirm port 9595**
   - Default OSC port in Loopy Pro
   - Check Loopy Pro settings if using custom port

4. **Network connectivity**
   - Devices must be on same network
   - Try pinging iPad from Rust server machine

### Rust server errors

**"Address already in use"**
- Another process is using port 3000
- Find and kill: `lsof -ti:3000 | xargs kill -9`

**"Failed to bind UDP socket"**
- Rare, usually indicates OS-level network issue
- Restart Rust server

## Future Enhancements

- [ ] Add TCP OSC support (for bidirectional feedback)
- [ ] Support custom OSC arguments (not just empty messages)
- [ ] Add OSC message queue for reliability
- [ ] Support multiple Loopy Pro instances
- [ ] Add Loopy Pro discovery via mDNS
- [ ] Create preset mappings (save track → LED combinations)

## References

- [Loopy Pro Manual - Control Settings](https://loopypro.com/manual/#control-settings-section)
- [OSC Specification](http://opensoundcontrol.org/spec-1_0)
- [rosc Rust Crate Documentation](https://docs.rs/rosc/)
- [Loopy Pro Forum - OSC Discussions](https://forum.loopypro.com/categories/osc)

## Last Updated

2025-11-01 - Initial documentation of OSC integration
