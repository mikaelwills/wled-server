Detailed Implementation Plan for Claude: Refactoring the Rust Light Server for SpacetimeDB
Objective:
Refactor the existing Rust server from a stateless HTTP-to-UDP proxy into a stateful, real-time "renderer client." This client will subscribe to a shared state managed by SpacetimeDB and continuously stream E1.31 (sACN) UDP packets to WLED boards based on that state. This new architecture must be resilient to crashes and support live updates to the lighting program.
Core Architectural Principles:
Database as the Single Source of Truth: The SpacetimeDB instance will hold the entire desired state of the lighting rig (e.g., which effect is running, what color, etc.). The Rust application will no longer hold any authoritative state in its own memory; it will only maintain a local, in-memory cache of the database state.
Decoupling of Control and Rendering: The UI (e.g., a Svelte app) will modify the state by writing directly to SpacetimeDB. The Rust app will be a "dumb" renderer that subscribes to this state and translates it into UDP packets.
Continuous Render Loop: The Rust app will not be event-driven. It will run a high-frequency loop that constantly reads from its local state cache and asserts this state onto the WLED boards via UDP. This ensures smooth animations and self-healing from dropped packets or board reboots.
Step 1: Define the SpacetimeDB Schema
This is the foundation. You will create a new SpacetimeDB module with the following tables. This schema supports both direct control and sequenced shows.
code
Rust
// file: src/lib.rs (in your SpacetimeDB module)

use spacetimedb::{spacetimedb, Identity, ReducerContext, Timestamp};

// Table for direct, real-time control of individual boards.
#[spacetimedb(table)]
pub struct WledBoard {
    #[primarykey]
    #[autoinc]
    board_id: u64,
    #[unique]
    name: String, // A human-readable name like "Left Stage Bar"
    ip_address: String, // The board's IP for UDP streaming
    
    // High-level state parameters
    effect_name: String, // "SOLID", "RAINBOW", "PULSE_CUSTOM"
    primary_color: [u8; 3], // [R, G, B]
    intensity: u8, // 0-255
    speed: u8, // 0-255
}

// Table to store the cues for a pre-programmed, sequenced show.
#[spacetimedb(table)]
pub struct Cue {
    #[primarykey]
    #[autoinc]
    cue_id: u64,
    show_id: String, // e.g., "song_1_final_version"
    timestamp_ms: u64, // The time in ms from the start of the show this cue fires
    
    // State to apply when this cue fires
    target_board_name: String,
    effect_name: String,
    primary_color: [u8; 3],
    intensity: u8,
    speed: u8,
}

// A singleton table to manage the master playback of a sequenced show.
#[spacetimedb(table, singleton)]
pub struct ShowControl {
    active_show_id: String,
    is_playing: bool,
    start_time_unix_ms: u64, // The wall-clock time the play button was hit
}

// Example reducers for the UI to call
#[spacetimedb(reducer)]
pub fn update_board_state(ctx: ReducerContext, board_name: String, effect: String, color: [u8; 3]) {
    // ... logic to find and update a WledBoard row ...
}

#[spacetimedb(reducer)]
pub fn set_show_playback(ctx: ReducerContext, playing: bool) {
    let mut control = ShowControl::get().unwrap_or_default();
    control.is_playing = playing;
    if playing {
        control.start_time_unix_ms = ctx.timestamp.as_micros() / 1000;
    }
    ShowControl::update(control);
}
Step 2: Rust Project Setup (Cargo.toml)
Your Rust renderer client will need the following dependencies.
code
Toml
[dependencies]
spacetimedb = "0.7" # Check for the latest version
tokio = { version = "1", features = ["full"] }
sacn = "0.5" # Or your preferred E1.31/sACN library
chrono = "0.4"
# Add other necessary crates like serde for config if needed
Step 3: State Management in Rust
Define structs in your Rust client to hold the local cache of the database state. This state needs to be shareable between the network thread and the render loop thread.
code
Rust
// file: src/state.rs

use std::sync::{Arc, Mutex};
use crate::spacetimedb_generated::*; // Assumes you've run `spacetimedb generate`

// Structs to hold the data from SpacetimeDB tables
// These will be generated for you by `spacetimedb generate`
// e.g., WledBoard, Cue, ShowControl

// This struct holds the entire application's state.
#[derive(Default, Clone)]
pub struct AppState {
    pub boards: Vec<wled_board::WledBoard>,
    pub cues: Vec<cue::Cue>,
    pub show_control: Option<show_control::ShowControl>,
    pub next_cue_index: usize,
    pub data_changed: bool, // A flag to signal the render loop to re-evaluate
}

// A thread-safe handle to our application state.
pub type AppStateHandle = Arc<Mutex<AppState>>;
Step 4: The Main Application Logic
The main.rs file will initialize the connection, register callbacks, and spawn the main render loop.
code
Rust
// file: src/main.rs

mod state;
// ... other modules ...

use spacetimedb::{connect, on_subscription_applied, SpacetimeDBClient};
use state::{AppState, AppStateHandle};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

const FRAME_INTERVAL_MS: u64 = 25; // for 40 FPS

fn main() {
    // 1. Initialize the shared application state
    let app_state_handle: AppStateHandle = Arc::new(Mutex::new(AppState::default()));

    // 2. Register callbacks to handle incoming data from SpacetimeDB
    register_callbacks(app_state_handle.clone());
    
    // 3. Spawn the render loop as a separate asynchronous task
    let render_loop_handle = app_state_handle.clone();
    tokio::spawn(async move {
        render_loop(render_loop_handle).await;
    });

    // 4. Connect to SpacetimeDB. This blocks the main thread.
    connect(
        "wss://your_spacetimedb_uri",
        "your_database_name",
        None, // Use credentials file or env var
    ).unwrap();
}

// Placeholder for the render loop
async fn render_loop(state_handle: AppStateHandle) {
    // ... To be implemented in Step 5 ...
}

// Placeholder for callbacks
fn register_callbacks(state_handle: AppStateHandle) {
    // ... To be implemented in Step 6 ...
}
Step 5: Implementing the Render Loop and E1.31 Output
This is the core "muscle" of the application.
code
Rust
// Add to src/main.rs

async fn render_loop(state_handle: AppStateHandle) {
    // Initialize your sACN/E1.31 source
    let mut sacn_source = sacn::SacnSource::new("My Rust Renderer").unwrap();

    loop {
        // --- READ STATE ---
        // Acquire a lock on the state. This is a very fast operation.
        let mut app_state = state_handle.lock().unwrap();
        
        // --- RENDER LOGIC ---
        // This is where you would implement the "smarter player" for sequenced shows,
        // or the logic for rendering direct-control `WledBoard` state.
        // For now, let's just use the direct control state.
        for board in &app_state.boards {
            // Placeholder: Calculate the LED data based on the board's state.
            // This is where your custom effect logic ("RAINBOW", etc.) would live.
            let led_data = calculate_led_frame(&board);

            // Send the E1.31 packet
            // You will need to map board IP/name to DMX universes.
            let universe_id = get_universe_for_board(&board.name);
            sacn_source.send(universe_id, &led_data).unwrap();
        }

        // --- SLEEP ---
        // Release the lock by letting `app_state` go out of scope.
        drop(app_state);
        sleep(Duration::from_millis(FRAME_INTERVAL_MS)).await;
    }
}

// Placeholder for your effect generation logic
fn calculate_led_frame(board: &wled_board::WledBoard) -> Vec<u8> {
    // TODO: Implement your rendering logic here.
    // e.g., if board.effect_name == "SOLID", return a Vec of `board.primary_color`.
    // if board.effect_name == "RAINBOW", calculate the rainbow frame based on current time.
    vec![0; 512] // Return a full DMX frame
}
Step 6: Implementing the SpacetimeDB Callbacks
This is how the application state gets updated from the database.
code
Rust
// Add to src/main.rs

use spacetimedb::{on_event, spacetimedb_lib, ReducerEvent};

fn register_callbacks(state_handle: AppStateHandle) {
    // This callback is triggered once the client has received all the data
    // for its initial subscriptions.
    on_subscription_applied(|| {
        println!("Initial subscription data received.");
        // We can now query the local database cache.
        let mut app_state = state_handle.lock().unwrap();
        
        app_state.boards = wled_board::WledBoard::iter().collect();
        app_state.cues = cue::Cue::iter().collect();
        app_state.show_control = show_control::ShowControl::get();
        
        // Signal that the data has changed so the renderer can re-evaluate.
        app_state.data_changed = true;
    });

    // This callback is triggered for every transactional change from the server.
    on_event(|event: &ReducerEvent| {
        if event.table_name == "WledBoard" || event.table_name == "Cue" {
            let mut app_state = state_handle.lock().unwrap();

            // Re-query the tables to get the latest state.
            // The iter() function is extremely fast as it reads from an in-memory cache.
            app_state.boards = wled_board::WledBoard::iter().collect();
            app_state.cues = cue::Cue::iter().collect();

            // Signal the change.
            app_state.data_changed = true;
            println!("State updated for table: {}", event.table_name);
        }
    });
}
Step 7: Implementing the "Smarter Player" Logic (Handling Live Edits)
Modify the render_loop to handle sequenced shows and live edits, as discussed previously.
code
Rust
// A more advanced render_loop in src/main.rs

async fn render_loop(state_handle: AppStateHandle) {
    // ... sACN setup ...
    loop {
        let mut app_state = state_handle.lock().unwrap();
        
        if let Some(control) = &app_state.show_control {
            if control.is_playing {
                // Calculate current show time
                let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                let current_show_time = now_ms - control.start_time_unix_ms;

                // If the script data has changed, re-find our place.
                if app_state.data_changed {
                    // Use a binary search to efficiently find the next cue.
                    match app_state.cues.binary_search_by_key(&current_show_time, |c| c.timestamp_ms) {
                        Ok(idx) => app_state.next_cue_index = idx + 1,
                        Err(idx) => app_state.next_cue_index = idx,
                    }
                    app_state.data_changed = false; // Reset the flag
                }
                
                // Fire any cues that are due
                while app_state.next_cue_index < app_state.cues.len() &&
                      current_show_time >= app_state.cues[app_state.next_cue_index].timestamp_ms {
                    
                    let cue_to_fire = &app_state.cues[app_state.next_cue_index];
                    // TODO: Find the board and construct/send the E1.31 UDP packet from the cue data.
                    println!("FIRING CUE at {}", cue_to_fire.timestamp_ms);

                    app_state.next_cue_index += 1;
                }
            }
        }
        
        // ... rest of the loop (direct control rendering, sleep, etc.) ...
        drop(app_state);
        sleep(Duration::from_millis(FRAME_INTERVAL_MS)).await;
    }
}
