# E1.31 Universe Configuration

## Overview

The WLED Rust Server automatically configures E1.31 universes on startup to ensure proper group isolation. Each group gets its own unique universe number, preventing cross-group interference when sending E1.31 commands.

## Architecture

### Per-Group Universe Isolation

- Each group has its own E1.31 transport with a unique universe number
- Groups are assigned universes sequentially starting from 1
- Universe assignments are logged during initialization
- Boards within a group only respond to packets on their configured universe

### Universe Assignment

```
Group Index → Universe Number
0 → Universe 1
1 → Universe 2
2 → Universe 3
...
```

**Example from boards.toml:**
```toml
[[groups]]
id = "Desk Group"
members = ["Desk", "Desk Ceiling"]
# → Assigned Universe 1

[[groups]]
id = "GROUP"
members = ["Two", "Three"]
# → Assigned Universe 2
```

## Automatic Board Configuration

### Configuration Process

On server startup, the system:

1. **Loads Configuration** - Reads `boards.toml` to determine group memberships
2. **Initializes E1.31 Transports** - Creates per-group transports with unique universes
3. **Configures Boards in Parallel** - Spawns async tasks for each board
4. **Sets E1.31 Parameters** - Configures universe, mode, and DMX address
5. **Reboots Boards** - Restarts boards to apply E1.31 settings
6. **Times Out After 10s** - Non-blocking with global timeout

### Configuration Parameters

Each board is configured with:

- **Universe (`uni`)** - Group's unique universe number (1, 2, 3, ...)
- **Mode (`mode`)** - Set to 10 (preset mode for E1.31 control)
- **DMX Address (`addr`)** - Set to 1 (standard DMX start address)

### API Endpoint Used

**POST** `http://{board_ip}/json/cfg`

**Payload:**
```json
{
  "if": {
    "live": {
      "dmx": {
        "uni": 2,
        "mode": 10,
        "addr": 1
      }
    }
  }
}
```

**Response:**
```json
{"success": true}
```

## Implementation Details

### Code Location

**File:** `src/main.rs`

**Function:** `configure_board_universe(board_ip: &str, universe: u16)`

**Key Features:**
- Uses `/json/cfg` endpoint (not `/settings/sync` which doesn't work reliably)
- Correct JSON path: `if.live.dmx` (not `if.sync.live.dmx`)
- 2-second wait after config POST for WLED to persist to flash
- Reboots board via `/json/state` with `{"rb": true}`
- 5-second timeout per HTTP request
- Returns error if configuration fails

### Parallel Execution

```rust
// Spawn parallel configuration tasks
for (universe_index, group) in loaded_config.groups.iter().enumerate() {
    let universe = (universe_index + 1) as u16;

    for member_id in &group.members {
        let task = tokio::spawn(async move {
            configure_board_universe(board_ip, universe).await
        });
        tasks.push(task);
    }
}

// Wait with global timeout
tokio::time::timeout(
    Duration::from_secs(10),
    futures::future::join_all(tasks)
).await;
```

### Logging

**Startup Logs:**
```
INFO rust_wled_server: Initializing E1.31 transport for group: ["192.168.1.67", "192.168.1.171"]
  group_id=Desk Group universe=1 board_count=2
INFO rust_wled_server::transport::e131_raw: E1.31 raw transport initialized (unicast to 2 boards)
  universe=1 board_count=2
INFO rust_wled_server: E1.31 transport initialized group_id=Desk Group universe=1

INFO rust_wled_server: Configuring board universe board_id=Desk group_id=Desk Group universe=1
INFO rust_wled_server: Successfully configured universe board_id=Desk universe=1
```

**Runtime Logs (when sending commands):**
```
INFO rust_wled_server::group: E1.31 group command - sending to universe 1 with 2 member board(s)
  group_id=Desk Group universe=1 targets=["192.168.1.67:5568", "192.168.1.171:5568"] member_count=2
INFO rust_wled_server::group: Sending power command group_id=Desk Group on=true preset=1 universe=1
```

## Troubleshooting

### Board Not Responding to E1.31 Commands

**Verify Configuration:**
```bash
curl -s http://{board_ip}/cfg.json | jq '.if.live.dmx'
```

**Expected Output:**
```json
{
  "uni": 2,           // Should match group's universe
  "seqskip": true,
  "e131prio": 0,
  "addr": 1,          // Should be 1
  "dss": 0,
  "mode": 10          // Should be 10 (preset mode)
}
```

### Manual Configuration

If automatic configuration fails, manually configure via WLED web UI:

1. Navigate to `http://{board_ip}/settings/sync`
2. Under **Realtime DMX (E1.31/Art-Net/sACN)**:
   - **Universe:** Set to group's universe number
   - **Mode:** Select "Preset"
   - **Start Address:** 1
3. Click **Save** at bottom of page
4. Board will reboot automatically

### Common Issues

**Issue:** Board configuration times out during startup
**Cause:** Board not reachable on network or already rebooting
**Solution:** Board will be configured on next server restart when reachable

**Issue:** Configuration succeeds but board doesn't respond
**Cause:** Board didn't reboot or settings didn't persist
**Solution:** Manually reboot board or use manual configuration

**Issue:** Wrong JSON path used
**Cause:** Using `if.sync.live.dmx` instead of `if.live.dmx`
**Solution:** Use correct path: `if.live.dmx`

**Issue:** Form POST to `/settings/sync` doesn't work
**Cause:** WLED form endpoint is unreliable for E1.31 settings
**Solution:** Use JSON POST to `/json/cfg` endpoint instead

## Verification

### Check Server Logs

```bash
# Look for successful configuration
grep "Successfully configured universe" /path/to/logs

# Look for E1.31 transport initialization
grep "E1.31 transport initialized" /path/to/logs
```

### Check Board Configuration

```bash
# Check single board
curl -s http://192.168.1.67/cfg.json | jq '.if.live.dmx'

# Check all boards in a group
for ip in 192.168.8.118 192.168.8.210; do
  echo "Board $ip:"
  curl -s http://$ip/cfg.json | jq '.if.live.dmx'
done
```

### Test E1.31 Commands

```bash
# Toggle group power via API
curl -X POST http://localhost:3010/api/group/GROUP/power \
  -H "Content-Type: application/json" \
  -d '{"on": true, "transition": 0}'

# Check logs for E1.31 transmission
# Should see: "E1.31 group command - sending to universe X"
```

## Performance Notes

- **Non-Blocking:** All board configurations run in parallel
- **Timeout:** 10-second global timeout prevents startup hangs
- **Failure Handling:** Failed configurations logged but don't block server startup
- **Reboot Impact:** Boards unavailable for ~3-5 seconds during reboot
- **Network Traffic:** Minimal - 2 HTTP requests per board on startup

## References

- **E1.31 (sACN) Specification:** ANSI E1.31-2018
- **WLED API Docs:** https://kno.wled.ge/interfaces/json-api/
- **Code Implementation:** `src/main.rs:44-99`, `src/group.rs:26-102`
