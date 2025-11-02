# WLED Server API Documentation

Base URL: `http://0.0.0.0:3000` (or `http://192.168.1.111:3000` on your network)

## Overview

This server acts as an intermediary between your frontend and multiple WLED devices. It manages WebSocket connections to each board, provides REST endpoints for control, and broadcasts real-time state updates via Server-Sent Events (SSE).

---

## Endpoints

### 1. Health Check

**GET /**

Check if server is running.

**Response:**
```
200 OK
"WLED Server Running"
```

---

### 2. List All Boards

**GET /boards**

Retrieve the current state of all registered WLED boards.

**Response:**
```json
200 OK
[
  {
    "id": "mikaels-bed",
    "ip": "192.168.1.172",
    "on": true,
    "brightness": 128,
    "color": [255, 128, 0],
    "effect": 0
  },
  {
    "id": "living-room",
    "ip": "192.168.1.175",
    "on": false,
    "brightness": 200,
    "color": [0, 255, 0],
    "effect": 5
  }
]
```

**Notes:**
- Returns an array of board states
- Each board queries its actor via oneshot channel
- May return 500 if actor communication fails

---

### 3. Register New Board

**POST /boards**

Dynamically add a new WLED board to the server.

**Request Body:**
```json
{
  "id": "living-room",
  "ip": "192.168.1.175"
}
```

**Response:**
```
201 CREATED
```

**Error Responses:**
- `409 CONFLICT` - Board with this ID already exists
- `500 INTERNAL_SERVER_ERROR` - Failed to acquire lock

**Notes:**
- Board actor is spawned immediately
- Starts WebSocket connection to WLED device
- Board persists until server restart (not saved to config)

---

### 4. Delete Board

**DELETE /boards/:id**

Remove a board from the server.

**Example:**
```
DELETE /boards/living-room
```

**Response:**
```
204 NO_CONTENT
```

**Error Responses:**
- `404 NOT_FOUND` - Board ID does not exist
- `500 INTERNAL_SERVER_ERROR` - Failed to acquire lock

**Notes:**
- Board actor stops automatically when channel is dropped
- WebSocket connection closes gracefully

---

### 5. Toggle Power

**POST /board/:id/toggle**

Toggle the on/off state of a specific board.

**Example:**
```
POST /board/mikaels-bed/toggle
```

**Request Body:** None (empty body)

**Response:**
```
200 OK
```

**Error Responses:**
- `404 NOT_FOUND` - Board ID does not exist
- `500 INTERNAL_SERVER_ERROR` - Failed to send command

**Notes:**
- Sends `{"on": true/false}` to WLED
- State change broadcasts via SSE

---

### 6. Set Brightness

**POST /board/:id/brightness**

Set the brightness level (0-255).

**Request Body:**
```json
{
  "brightness": 180
}
```

**Response:**
```
200 OK
```

**Error Responses:**
- `404 NOT_FOUND` - Board ID does not exist
- `500 INTERNAL_SERVER_ERROR` - Failed to send command

**Notes:**
- Value clamped to u8 (0-255)
- Sends `{"bri": 180}` to WLED
- State change broadcasts via SSE

---

### 7. Set Color

**POST /board/:id/color**

Set the RGB color.

**Request Body:**
```json
{
  "r": 255,
  "g": 128,
  "b": 0
}
```

**Response:**
```
200 OK
```

**Error Responses:**
- `404 NOT_FOUND` - Board ID does not exist
- `500 INTERNAL_SERVER_ERROR` - Failed to send command

**Notes:**
- Each value clamped to u8 (0-255)
- Sends `{"seg":[{"col":[[r,g,b]]}]}` to WLED
- Applies to first segment only
- State change broadcasts via SSE

---

### 8. Set Effect

**POST /board/:id/effect**

Set the WLED effect by ID.

**Request Body:**
```json
{
  "effect": 12
}
```

**Response:**
```
200 OK
```

**Error Responses:**
- `404 NOT_FOUND` - Board ID does not exist
- `500 INTERNAL_SERVER_ERROR` - Failed to send command

**Notes:**
- Effect ID is WLED-specific (0-based index)
- Sends `{"seg":[{"fx": 12}]}` to WLED
- Applies to first segment only
- State change broadcasts via SSE

---

### 9. Set Preset

**POST /board/:id/preset**

Apply a saved WLED preset.

**Request Body:**
```json
{
  "preset": 3
}
```

**Response:**
```
200 OK
```

**Error Responses:**
- `404 NOT_FOUND` - Board ID does not exist
- `500 INTERNAL_SERVER_ERROR` - Failed to send command

**Notes:**
- Preset ID is 1-based (matches WLED UI)
- Sends `{"ps": 3}` to WLED
- Presets can change multiple settings at once (brightness, color, effect, etc.)
- Configure presets in WLED web interface
- State change broadcasts via SSE

---

### 10. Server-Sent Events (SSE)

**GET /events**

Subscribe to real-time state updates from all boards.

**Response:**
```
200 OK
Content-Type: text/event-stream

data: {"type":"connected","message":"Connected to WLED server"}

data: {"type":"state_update","board_id":"mikaels-bed","state":{"id":"mikaels-bed","ip":"192.168.1.172","on":true,"brightness":128,"color":[255,128,0],"effect":0}}

data: {"type":"state_update","board_id":"living-room","state":{"id":"living-room","ip":"192.168.1.175","on":false,"brightness":200,"color":[0,255,0],"effect":5}}
```

**Event Types:**

1. **connected**
   - Sent immediately on connection
   - Confirms SSE stream is active

2. **state_update**
   - Sent whenever a board's state changes
   - Includes full board state
   - Triggered by:
     - API commands (POST endpoints)
     - WLED device changes (buttons, app, etc.)
     - WebSocket messages from WLED

**Notes:**
- Keep connection open to receive updates
- Server sends keep-alive pings automatically
- Reconnect if connection drops

---

## Frontend Integration Guide

### Initial Setup

1. **Connect to SSE stream first:**
```javascript
const eventSource = new EventSource('http://127.0.0.1:3000/events');

eventSource.addEventListener('message', (event) => {
  const data = JSON.parse(event.data);

  if (data.type === 'connected') {
    console.log('Connected to WLED server');
  } else if (data.type === 'state_update') {
    updateBoardUI(data.board_id, data.state);
  }
});
```

2. **Fetch initial board list:**
```javascript
const response = await fetch('http://127.0.0.1:3000/boards');
const boards = await response.json();
boards.forEach(board => renderBoard(board));
```

### Board Management

**Add a new board:**
```javascript
async function addBoard(id, ip) {
  const response = await fetch('http://127.0.0.1:3000/boards', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id, ip })
  });

  if (response.status === 201) {
    console.log('Board added successfully');
  } else if (response.status === 409) {
    alert('Board already exists');
  }
}
```

**Remove a board:**
```javascript
async function deleteBoard(id) {
  const response = await fetch(`http://127.0.0.1:3000/boards/${id}`, {
    method: 'DELETE'
  });

  if (response.status === 204) {
    console.log('Board deleted');
  }
}
```

### Control Operations

**Toggle power:**
```javascript
async function togglePower(boardId) {
  await fetch(`http://127.0.0.1:3000/board/${boardId}/toggle`, {
    method: 'POST'
  });
  // State update will arrive via SSE
}
```

**Set brightness:**
```javascript
async function setBrightness(boardId, brightness) {
  await fetch(`http://127.0.0.1:3000/board/${boardId}/brightness`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ brightness })
  });
}
```

**Set color:**
```javascript
async function setColor(boardId, r, g, b) {
  await fetch(`http://127.0.0.1:3000/board/${boardId}/color`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ r, g, b })
  });
}
```

**Set effect:**
```javascript
async function setEffect(boardId, effect) {
  await fetch(`http://127.0.0.1:3000/board/${boardId}/effect`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ effect })
  });
}
```

---

## Architecture Notes

### Actor Pattern
- Each board has a dedicated actor (tokio task)
- Actors manage WebSocket connections to WLED devices
- Auto-reconnects on connection loss (5-second delay)

### State Management
- Server maintains in-memory state for each board
- State updates from WLED devices broadcast to all SSE clients
- No persistent storage (boards from config loaded on startup)

### Concurrency
- Thread-safe state via `Arc<RwLock<HashMap>>`
- Message passing via `mpsc` channels to actors
- Broadcast channel for SSE events

---

## Error Handling

The server handles errors gracefully:
- Missing config file → starts with no boards
- Port in use → exits with error message
- Lock poisoning → returns 500 to client
- Board not found → returns 404
- Duplicate board → returns 409

---

## Configuration

`boards.toml` (optional, loads on startup):
```toml
[[boards]]
id = "mikaels-bed"
ip = "192.168.1.172"

[[boards]]
id = "living-room"
ip = "192.168.1.175"
```

---

## Frontend

A SvelteKit-based PWA frontend is available in `/frontend`.

**Features:**
- Dark mode interface
- WLED-style circular HSV color picker
- Brightness control (0-255)
- All 186 WLED effects (alphabetically sorted)
- Toggle switches for power control
- Add/delete boards dynamically
- Collapsible board cards
- Touch-friendly for mobile devices
- Network-accessible (works on any device on your local network)

**Running the frontend:**
```bash
cd frontend
bun install
bun run dev -- --host
```

Access at: `http://<your-ip>:5173`

**Building for production:**
```bash
cd frontend
bun run build
```

Static files output to `/frontend/build`

## Future Enhancements

- Server-Sent Events integration in frontend (real-time updates without refresh)
- PWA manifest and service worker (install as app)
- Docker + nginx deployment
- Scenes (multi-board synchronized control)
- Persistent storage for dynamically added boards
- Authentication/authorization
- HTTPS support
- Board discovery via mDNS
