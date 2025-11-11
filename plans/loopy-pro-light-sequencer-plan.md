# Loopy Pro Light Sequencer - Implementation Plan

## Overview

Create a visual timeline editor for choreographing WLED lighting changes synchronized with Loopy Pro audio tracks. Users will drag-and-drop WAV files, scrub through a waveform, and place light cue markers at specific timestamps to create synchronized lighting programs.

## User Workflow

1. **Navigate to Sequencer Page** - New route in frontend `/sequencer`
2. **Upload Audio** - Drag-and-drop WAV file (3-5 minutes, ~50-80 MB)
3. **Visualize Waveform** - WaveSurfer.js renders interactive waveform
4. **Create Cues** - Click on waveform to create light cue markers
5. **Configure Cues** - Click marker to set color, effect, brightness, target boards
6. **Save Program** - Store light program JSON with cue list
7. **Trigger Performance** - Button sends OSC to Loopy Pro + starts light sequence timer

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Frontend (SvelteKit)                     │
├─────────────────────────────────────────────────────────────┤
│  Sequencer Page                                             │
│  ├── WaveSurfer.js (waveform + regions plugin)             │
│  ├── Cue Editor Panel (color, effect, brightness, boards)  │
│  ├── Cue List View (edit/delete cues)                      │
│  └── Save/Load light programs                              │
├─────────────────────────────────────────────────────────────┤
│  Performance Page                                           │
│  └── Song buttons → trigger OSC + light sequence           │
└─────────────────────────────────────────────────────────────┘
                              ↓ HTTP
┌─────────────────────────────────────────────────────────────┐
│                    Rust Backend                             │
├─────────────────────────────────────────────────────────────┤
│  Endpoints:                                                 │
│  ├── POST /osc (existing - triggers Loopy Pro)            │
│  ├── POST /light-programs (save program JSON)             │
│  ├── GET  /light-programs (list saved programs)           │
│  ├── GET  /light-programs/:id (load specific program)     │
│  └── DELETE /light-programs/:id (delete program)          │
└─────────────────────────────────────────────────────────────┘
                              ↓ UDP OSC
┌─────────────────────────────────────────────────────────────┐
│                    Loopy Pro (iPad)                         │
│                    192.168.1.242:9595                       │
└─────────────────────────────────────────────────────────────┘
```

## Technical Stack

### Frontend
- **WaveSurfer.js** - Waveform rendering, audio playback, scrubbing
- **WaveSurfer Regions Plugin** - Draggable/resizable timeline markers
- **Svelte Components** - Cue editor UI, cue list, song buttons
- **Web Audio API** - Client-side WAV decoding (via WaveSurfer)
- **localStorage or Server Storage** - Light program persistence

### Backend (Rust)
- **Existing OSC endpoint** - Already implemented for Loopy Pro
- **New CRUD endpoints** - Light program storage/retrieval
- **File storage or DB** - JSON files for light programs
- **No WAV storage needed** - Audio only lives in browser during editing

### Libraries
- `wavesurfer.js` - v7.x (latest)
- `wavesurfer.js/dist/plugins/regions.esm.js` - Regions plugin
- Existing Rust dependencies (rosc for OSC)

## Data Model

### Light Program JSON Structure

```json
{
  "id": "song-123",
  "name": "My Awesome Song",
  "duration": 245.5,
  "loopyProTrack": "06",
  "createdAt": "2025-11-02T18:30:00Z",
  "cues": [
    {
      "id": "cue-1",
      "time": 0,
      "label": "Intro - Purple",
      "boards": ["group-1"],
      "color": [128, 0, 255],
      "effect": 0,
      "brightness": 150,
      "transition": 0
    },
    {
      "id": "cue-2",
      "time": 32.5,
      "label": "Verse - Blue pulse",
      "boards": ["group-1", "mikaels-bed"],
      "color": [0, 100, 255],
      "effect": 45,
      "brightness": 200,
      "transition": 1000
    },
    {
      "id": "cue-3",
      "time": 65.0,
      "label": "Chorus - Red strobe",
      "boards": ["group-1"],
      "color": [255, 0, 0],
      "effect": 12,
      "brightness": 255,
      "transition": 0
    }
  ]
}
```

### Cue Properties
- **id** - Unique identifier for the cue
- **time** - Timestamp in seconds (float)
- **label** - User-defined description
- **boards** - Array of board IDs or group IDs to control
- **color** - RGB array [r, g, b] (0-255 each)
- **effect** - WLED effect number (0-185)
- **brightness** - 0-255
- **transition** - Transition duration in milliseconds (optional, WLED supports this)

## Implementation Phases

### Phase 1: Sequencer Page - Waveform Editor

**Goal:** Load WAV file and render interactive waveform with playback controls

**Tasks:**
1. Create new route: `/sequencer` in SvelteKit
2. Install WaveSurfer.js: `bun add wavesurfer.js`
3. Create `Sequencer.svelte` component
4. Implement drag-and-drop file upload zone
5. Initialize WaveSurfer with Regions plugin
6. Load WAV file and render waveform
7. Add playback controls (play/pause, scrub)
8. Add zoom slider

**Files:**
- `frontend/src/routes/sequencer/+page.svelte`
- `frontend/src/lib/Sequencer.svelte`

**Code Snippet:**
```javascript
import WaveSurfer from 'wavesurfer.js'
import RegionsPlugin from 'wavesurfer.js/dist/plugins/regions.esm.js'

const regions = RegionsPlugin.create()

const ws = WaveSurfer.create({
  container: '#waveform',
  waveColor: 'rgb(200, 100, 255)',
  progressColor: 'rgb(100, 0, 200)',
  plugins: [regions],
})

// Handle file drop
function handleFileDrop(file) {
  const url = URL.createObjectURL(file)
  ws.load(url)
}
```

---

### Phase 2: Cue Marker Creation

**Goal:** Click on waveform to create light cue markers

**Tasks:**
1. Enable click-to-create markers on waveform
2. Create marker with default settings (white, no effect, mid brightness)
3. Display markers on timeline with labels
4. Store cue metadata in reactive Svelte store
5. Link region IDs to cue settings

**Implementation:**
```javascript
let cues = $state([])

ws.on('click', (relativeX) => {
  const clickTime = ws.getCurrentTime() * relativeX

  const newRegion = regions.addRegion({
    start: clickTime,
    content: 'New Cue',
    color: 'rgba(100, 150, 255, 0.7)',
  })

  cues.push({
    id: newRegion.id,
    time: clickTime,
    label: 'New Cue',
    boards: [],
    color: [255, 255, 255],
    effect: 0,
    brightness: 128,
    transition: 0
  })
})

// Update cue time when region is dragged
regions.on('region-updated', (region) => {
  const cue = cues.find(c => c.id === region.id)
  if (cue) cue.time = region.start
})
```

---

### Phase 3: Cue Editor Panel

**Goal:** Click marker to open editor panel for configuring light settings

**Tasks:**
1. Create `CueEditor.svelte` component
2. Show/hide panel on region click
3. Reuse existing UI components:
   - ColorWheel.svelte for color picker
   - Effects dropdown (186 effects)
   - Brightness slider
   - Board/group selector (checkboxes)
4. Update cue settings on changes
5. Add delete cue button
6. Add label/name input field

**Component Structure:**
```svelte
<script>
  import ColorWheel from '$lib/ColorWheel.svelte'

  let { cue, boards, groups, onUpdate, onDelete } = $props()
</script>

<div class="cue-editor-panel">
  <h3>Edit Cue at {cue.time.toFixed(2)}s</h3>

  <label>
    Label: <input bind:value={cue.label} on:input={onUpdate} />
  </label>

  <label>Boards:</label>
  {#each boards as board}
    <label>
      <input type="checkbox" value={board.id} bind:group={cue.boards} on:change={onUpdate} />
      {board.name}
    </label>
  {/each}

  <ColorWheel color={cue.color} on:change={(e) => { cue.color = e.detail; onUpdate() }} />

  <label>
    Effect:
    <select bind:value={cue.effect} on:change={onUpdate}>
      {#each effects as effect, i}
        <option value={i}>{effect}</option>
      {/each}
    </select>
  </label>

  <label>
    Brightness: <input type="range" min="0" max="255" bind:value={cue.brightness} on:input={onUpdate} />
  </label>

  <label>
    Transition (ms): <input type="number" bind:value={cue.transition} on:input={onUpdate} />
  </label>

  <button onclick={onDelete}>Delete Cue</button>
</div>
```

---

### Phase 4: Cue List View

**Goal:** Display all cues in a sortable list for quick editing

**Tasks:**
1. Create `CueList.svelte` component
2. Show table/list of all cues sorted by time
3. Display: timestamp, label, color preview dot, effect name
4. Click row to select and edit cue
5. Delete button for each cue
6. Sync selection with timeline (clicking list item seeks to cue)

**Component:**
```svelte
<div class="cue-list">
  <h3>Cues ({cues.length})</h3>
  <table>
    <thead>
      <tr>
        <th>Time</th>
        <th>Label</th>
        <th>Color</th>
        <th>Effect</th>
        <th>Actions</th>
      </tr>
    </thead>
    <tbody>
      {#each cues.sort((a, b) => a.time - b.time) as cue}
        <tr onclick={() => selectCue(cue)}>
          <td>{formatTime(cue.time)}</td>
          <td>{cue.label}</td>
          <td><div class="color-dot" style="background: rgb({cue.color.join(',')})"></div></td>
          <td>{effects[cue.effect]}</td>
          <td><button onclick={() => deleteCue(cue.id)}>Delete</button></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
```

---

### Phase 5: Save/Load Light Programs

**Goal:** Persist light programs to server for later use

**Tasks:**

**Frontend:**
1. Create save button that POST to `/light-programs`
2. Create load/browse page to GET `/light-programs`
3. Add program name input field
4. Associate program with Loopy Pro track number
5. Handle load: restore cues and create regions

**Backend:**
1. Create new endpoints in `src/main.rs`:
   - `POST /light-programs` - Save program JSON to file
   - `GET /light-programs` - List all saved programs
   - `GET /light-programs/:id` - Load specific program
   - `DELETE /light-programs/:id` - Delete program
2. Store programs in `light_programs/` directory as JSON files
3. Use program ID as filename: `light_programs/{id}.json`

**Rust Endpoint Example:**
```rust
#[derive(Deserialize, Serialize)]
struct LightProgram {
    id: String,
    name: String,
    duration: f64,
    loopy_pro_track: String,
    created_at: String,
    cues: Vec<Cue>,
}

#[derive(Deserialize, Serialize)]
struct Cue {
    id: String,
    time: f64,
    label: String,
    boards: Vec<String>,
    color: [u8; 3],
    effect: u8,
    brightness: u8,
    transition: u32,
}

async fn save_light_program(
    Json(program): Json<LightProgram>,
) -> Result<StatusCode, StatusCode> {
    let path = format!("light_programs/{}.json", program.id);
    let json = serde_json::to_string_pretty(&program)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tokio::fs::create_dir_all("light_programs").await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tokio::fs::write(&path, json).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::CREATED)
}

async fn list_light_programs() -> Result<Json<Vec<LightProgram>>, StatusCode> {
    // Read all JSON files from light_programs/ directory
    // Parse and return as JSON array
}
```

---

### Phase 6: Performance Page - Song Triggers

**Goal:** Create page with song buttons that trigger Loopy Pro + light sequence

**Tasks:**

**Frontend:**
1. Create new route: `/performance`
2. Display saved light programs as large buttons
3. On button click:
   - Send OSC to Loopy Pro (existing `/osc` endpoint)
   - Start timer-based light sequence executor
   - Highlight active song
4. Create light sequence executor:
   - Schedule all cues based on timestamps
   - Use `setTimeout` or interval checking
   - Send board commands at each cue time
5. Add stop button to halt sequence and Loopy Pro track

**Performance Page Component:**
```svelte
<script>
  import { onMount } from 'svelte'

  let programs = $state([])
  let activeProgram = $state(null)
  let scheduledTimeouts = []

  onMount(async () => {
    const res = await fetch(`${API_URL}/light-programs`)
    programs = await res.json()
  })

  async function triggerSong(program) {
    // 1. Send OSC to Loopy Pro
    await fetch(`${API_URL}/osc`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address: `/PlayStop/${program.loopy_pro_track}` })
    })

    // 2. Start light sequence
    activeProgram = program
    const startTime = Date.now()

    program.cues.forEach(cue => {
      const timeoutId = setTimeout(async () => {
        // Trigger light change for each board in cue
        for (const boardId of cue.boards) {
          await setColor(boardId, ...cue.color)
          await setEffect(boardId, cue.effect)
          await setBrightness(boardId, cue.brightness)
        }
      }, cue.time * 1000)

      scheduledTimeouts.push(timeoutId)
    })
  }

  function stopSequence() {
    scheduledTimeouts.forEach(clearTimeout)
    scheduledTimeouts = []
    activeProgram = null
  }
</script>

<div class="performance-page">
  <h1>Songs</h1>

  {#if activeProgram}
    <div class="now-playing">
      Playing: {activeProgram.name}
      <button onclick={stopSequence}>Stop</button>
    </div>
  {/if}

  <div class="song-grid">
    {#each programs as program}
      <button
        class="song-button"
        class:active={activeProgram?.id === program.id}
        onclick={() => triggerSong(program)}
      >
        <div class="song-name">{program.name}</div>
        <div class="song-meta">Track {program.loopy_pro_track} • {formatDuration(program.duration)}</div>
        <div class="cue-count">{program.cues.length} cues</div>
      </button>
    {/each}
  </div>
</div>
```

---

### Phase 7: UI/UX Polish

**Goal:** Make the interface intuitive and visually appealing

**Tasks:**
1. Dark mode theme (match existing WLED UI)
2. Responsive layout (mobile-friendly)
3. Keyboard shortcuts (spacebar = play/pause, arrow keys = skip cues)
4. Visual feedback when cue is triggered during performance
5. Color-code regions by light color
6. Add tooltips and help text
7. Loading states and error handling
8. Confirmation dialogs for delete actions

---

## Advanced Features (Future Enhancements)

### Phase 8: Automation Curves
- Use Envelope plugin for smooth brightness/color transitions
- Draw curves instead of discrete cue points
- Interpolate between keyframes

### Phase 9: Multi-Track Support
- Multiple waveforms stacked (like DAW)
- Different boards on different tracks
- Solo/mute tracks

### Phase 10: MIDI Integration
- Send MIDI commands alongside OSC
- Sync with external hardware
- MIDI clock sync

### Phase 11: Live Mode
- Manual cue triggering (bypass timeline)
- Tap tempo for effects
- Crossfader between cues

### Phase 12: Templates & Presets
- Save cue configurations as presets
- Apply preset to multiple songs
- Copy/paste cues between programs

## Testing Strategy

### Manual Testing Checklist
- [ ] Upload WAV file (various sizes: 1MB, 10MB, 80MB)
- [ ] Waveform renders correctly
- [ ] Playback and scrubbing works smoothly
- [ ] Create cue markers by clicking
- [ ] Edit cue settings (color, effect, brightness, boards)
- [ ] Drag cue markers to different times
- [ ] Delete cue markers
- [ ] Save light program to server
- [ ] Load light program from server
- [ ] Trigger song from performance page
- [ ] Verify OSC sent to Loopy Pro (track plays)
- [ ] Verify light cues execute at correct times
- [ ] Stop sequence mid-performance
- [ ] Test with multiple boards and groups

### Performance Testing
- [ ] Large WAV files (80MB+) load without lag
- [ ] Waveform scrolling is smooth with 100+ cues
- [ ] Light sequence timing is accurate (±100ms tolerance)
- [ ] No memory leaks during long editing sessions

### Edge Cases
- [ ] No boards/groups selected in cue (should warn user)
- [ ] Overlapping cues at same timestamp
- [ ] Very short songs (<30s)
- [ ] Very long songs (>10 minutes)
- [ ] Invalid WAV files (handle gracefully)
- [ ] Network failure during save/load

## File Structure

```
rust-wled-server/
├── src/
│   └── main.rs (add light program CRUD endpoints)
├── light_programs/ (new directory)
│   ├── song-123.json
│   └── song-456.json
├── frontend/
│   └── src/
│       ├── routes/
│       │   ├── sequencer/
│       │   │   └── +page.svelte (waveform editor)
│       │   ├── performance/
│       │   │   └── +page.svelte (song triggers)
│       │   └── programs/
│       │       └── +page.svelte (browse/manage programs)
│       └── lib/
│           ├── Sequencer.svelte (main waveform component)
│           ├── CueEditor.svelte (cue settings panel)
│           ├── CueList.svelte (cue list table)
│           ├── ColorWheel.svelte (existing)
│           └── api/
│               └── lightPrograms.ts (API client functions)
└── loopy-pro-light-sequencer-plan.md (this file)
```

## Dependencies to Install

### Frontend
```bash
cd frontend
bun add wavesurfer.js
```

### Backend
No new dependencies needed (uses existing rosc, tokio, serde)

## Timeline Estimate

| Phase | Estimated Time | Complexity |
|-------|----------------|------------|
| Phase 1: Waveform Editor | 4-6 hours | Medium |
| Phase 2: Cue Creation | 2-3 hours | Easy |
| Phase 3: Cue Editor Panel | 3-4 hours | Medium |
| Phase 4: Cue List View | 2 hours | Easy |
| Phase 5: Save/Load Backend | 3-4 hours | Medium |
| Phase 6: Performance Page | 4-5 hours | Medium |
| Phase 7: UI/UX Polish | 3-4 hours | Easy-Medium |
| **Total** | **21-30 hours** | |

## Success Criteria

- [ ] User can load a WAV file and see the waveform
- [ ] User can create, edit, and delete cue markers on the timeline
- [ ] User can configure color, effect, brightness, and target boards for each cue
- [ ] User can save and load light programs from the server
- [ ] User can trigger a song from the performance page
- [ ] OSC message sent to Loopy Pro starts the audio track
- [ ] Light cues execute at the correct timestamps (within 100ms accuracy)
- [ ] UI is responsive and works on mobile devices
- [ ] No audio storage needed (WAV only used during editing)

## Open Questions

1. **WAV Storage:** Should we store WAV files on server for future edits, or require re-upload?
   - **Decision:** No server storage - user re-uploads if they want to edit later. Keeps things simple.

2. **Timing Accuracy:** Is setTimeout accurate enough, or do we need Web Audio API clock sync?
   - **Decision:** Start with setTimeout. If drift becomes an issue, upgrade to AudioContext.currentTime.

3. **Multiple Loopy Pro Instances:** Support for different iPads/instances?
   - **Decision:** Not for v1. Can add later with configurable IP/port per program.

4. **Sync Drift:** Over 5 minutes, will timing drift between Loopy Pro and light sequence?
   - **Decision:** Accept some drift for v1. If needed, add periodic sync points or manual nudge controls.

5. **Preview Mode:** Should sequencer have a "preview lights without audio" mode?
   - **Decision:** Yes - add a preview button that triggers lights without OSC. Good for testing.

## References

- [WaveSurfer.js Documentation](https://wavesurfer.xyz/)
- [WaveSurfer.js Regions Plugin](https://wavesurfer.xyz/docs/classes/plugins_regions.default)
- [Loopy Pro OSC Documentation](./loopy-pro-osc-docs.md)
- [WLED JSON API](https://kno.wled.ge/interfaces/json-api/)

## Last Updated

2025-11-02 - Initial plan created
