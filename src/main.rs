use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{delete, get, post, put},
    Json, Router,
};
use futures::Stream;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_stream::StreamExt;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

mod actor;
mod audio;
mod board;
mod config;
mod group;
mod preset;
mod program;
mod sse;
mod types;

use actor::BoardActor;
use board::{BoardCommand, BoardState, GroupCommand};
use config::Config;
use sse::SseEvent;
use types::{
    AppState, BoardEntry, CreateGroupRequest, GroupBrightnessRequest, GroupColorRequest,
    GroupEffectRequest, GroupOperationResult, GroupPresetRequest, OscRequest, PowerRequest,
    PresetState, RegisterBoardRequest, SavePresetRequest, SharedState, UpdateBoardRequest,
    UpdateGroupRequest, UploadAudioRequest, UploadAudioResponse, WledPreset,
};

use http::Method;
use tower_http::cors::CorsLayer;

use std::net::UdpSocket;

#[tokio::main]
async fn main() {
    // Initialize tracing subscriber with environment-based configuration
    // Set log level with RUST_LOG environment variable (default: info)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Starting WLED Rust Server");

    // Initialize storage paths from environment or defaults
    let storage_paths = config::StoragePaths::default();
    if let Err(e) = storage_paths.init() {
        error!("Failed to initialize storage paths: {}", e);
        error!("Program storage will be unavailable");
    }

    // Create global broadcast channel for SSE events
    let (broadcast_tx, _) = broadcast::channel::<SseEvent>(100);

    let state: SharedState = Arc::new(AppState {
        boards: Arc::new(RwLock::new(HashMap::new())),
        broadcast_tx: Arc::new(broadcast_tx),
        storage_paths: Arc::new(storage_paths),
    });

    // Load boards from config if available
    match Config::load() {
        Ok(config) => {
            info!("Loaded {} board(s) from boards.toml", config.boards.len());
            for board_config in config.boards {
                let (tx, rx) = mpsc::channel(100);
                {
                    let mut senders = state.boards.write().await;
                    senders.insert(
                        board_config.id.clone(),
                        BoardEntry {
                            ip: board_config.ip.clone(),
                            sender: tx,
                        },
                    );
                }
                let actor =
                    BoardActor::new(board_config.id, board_config.ip, state.broadcast_tx.clone());
                tokio::spawn(async move {
                    if let Err(e) = actor.run(rx).await {
                        error!("Actor error: {}", e);
                    }
                });
            }
        }
        Err(e) => {
            warn!("Could not load boards.toml: {}", e);
            info!("Server starting with no boards configured");
        }
    }

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(tower_http::cors::Any);

    // Build API router
    let api_router = Router::new()
        .route("/health", get(hello))
        .route("/boards", get(list_boards).post(register_board))
        .route("/boards/:id", put(update_board).delete(delete_board))
        .route("/groups", post(create_group))
        .route("/groups/:id", put(update_group).delete(delete_group))
        .route("/group/:id/power", post(set_group_power))
        .route("/group/:id/brightness", post(set_group_brightness))
        .route("/group/:id/color", post(set_group_color))
        .route("/group/:id/effect", post(set_group_effect))
        .route("/group/:id/preset", post(set_group_preset))
        .route("/group/:id/presets/sync", post(sync_presets_to_group))
        .route("/board/:id/power", post(set_board_power))
        .route("/board/:id/brightness", post(set_brightness))
        .route("/board/:id/color", post(set_color))
        .route("/board/:id/effect", post(set_effect))
        .route("/board/:id/speed", post(set_speed))
        .route("/board/:id/intensity", post(set_intensity))
        .route("/board/:id/preset", post(set_preset))
        .route("/board/:id/presets", get(get_board_presets))
        .route("/board/:id/led-count", post(set_led_count))
        .route("/board/:id/reset-segment", post(reset_segment))
        .route("/board/:id/presets/sync", post(sync_presets_to_board))
        .route("/events", get(sse_handler))
        .route("/programs", post(save_program))
        .route("/programs", get(list_programs))
        .route("/programs/:id", get(get_program))
        .route("/programs/:id", delete(delete_program))
        .route("/programs/:id", put(update_program))
        .route("/presets", post(save_preset).get(list_presets))
        .route("/presets/:id", get(get_preset).delete(delete_preset))
        .route(
            "/audio/:id",
            post(upload_audio).get(get_audio).delete(delete_audio),
        )
        .route("/osc", post(send_osc))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024)) // 50MB limit
        .with_state(state.clone());

    // API-only server
    let app = Router::new().nest("/api", api_router).layer(cors);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3010".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => {
            info!("API Server running on http://{}", addr);
            l
        }
        Err(e) => {
            error!("Failed to bind to {}: {}", addr, e);
            error!("Is port already in use?");
            return;
        }
    };

    match axum::serve(listener, app).await {
        Ok(_) => info!("Server stopped properly"),
        Err(e) => error!("Server error: {}", e),
    }
}

async fn hello() -> &'static str {
    info!("Health check called");
    "WLED Server Running"
}

async fn update_program(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(program): Json<program::Program>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    if id != program.id {
        return Err((StatusCode::BAD_REQUEST, "ID mismatch".to_string()));
    }

    program
        .save_to_file(&state.storage_paths.programs)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "Failed to save program '{}' to {}: {}",
                    program.id,
                    state.storage_paths.programs.display(),
                    e
                ),
            )
        })?;

    Ok(StatusCode::OK)
}

async fn get_program(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<program::Program>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let program = program::Program::load_from_file(&id, &state.storage_paths.programs)
        .map_err(|_| (StatusCode::NOT_FOUND, format!("Program {} not found", id)))?;

    Ok(Json(program))
}

async fn delete_program(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let program = program::Program::load_from_file(&id, &state.storage_paths.programs)
        .map_err(|_| (StatusCode::NOT_FOUND, format!("Program {} not found", id)))?;

    program
        .delete(&state.storage_paths.programs, &state.storage_paths.audio)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to delete program '{}': {}", id, e),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

async fn save_program(
    State(state): State<SharedState>,
    Json(program): Json<program::Program>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    program
        .save_to_file(&state.storage_paths.programs)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::CREATED)
}

async fn list_programs(
    State(state): State<SharedState>,
) -> Result<Json<Vec<program::Program>>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let programs = program::Program::load_all(&state.storage_paths.programs).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "Failed to load programs from {}: {}",
                state.storage_paths.programs.display(),
                e
            ),
        )
    })?;

    Ok(Json(programs))
}

async fn register_board(
    State(state): State<SharedState>,
    Json(payload): Json<RegisterBoardRequest>,
) -> Result<StatusCode, StatusCode> {
    let board_id = payload.id.clone();
    let (tx, rx) = mpsc::channel(100);

    {
        let mut senders = state.boards.write().await;
        if senders.contains_key(&board_id) {
            return Err(StatusCode::CONFLICT);
        }
        senders.insert(
            board_id.clone(),
            BoardEntry {
                ip: payload.ip.clone(),
                sender: tx,
            },
        );
    }

    let actor = BoardActor::new(
        payload.id.clone(),
        payload.ip.clone(),
        state.broadcast_tx.clone(),
    );
    let board_id_for_spawn = board_id.clone();
    tokio::spawn(async move {
        if let Err(e) = actor.run(rx).await {
            error!(board_id = %board_id_for_spawn, "Actor error: {}", e);
        }
    });

    // Persist to boards.toml
    let mut config = Config::load().unwrap_or(Config {
        boards: vec![],
        groups: vec![],
    });
    config.boards.push(config::BoardConfig {
        id: payload.id,
        ip: payload.ip,
    });

    if let Err(e) = config.save() {
        warn!("Failed to save boards.toml: {}", e);
        // Note: Board is already registered in memory, so we return success
        // but log the error. Alternative would be to roll back the registration.
    }

    info!(board_id = %board_id, "Registered new board");
    Ok(StatusCode::CREATED)
}

async fn delete_board(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
) -> Result<StatusCode, StatusCode> {
    // Check if board exists in memory
    let sender = {
        let senders = state.boards.read().await;
        senders.get(&board_id).map(|entry| entry.sender.clone())
    };

    let tx = sender.ok_or(StatusCode::NOT_FOUND)?;

    // Update and save config FIRST (before touching memory)
    let mut config = Config::load().unwrap_or(Config {
        boards: vec![],
        groups: vec![],
    });
    config.boards.retain(|b| b.id != board_id);

    // Remove board from all groups and delete empty groups
    for group in config.groups.iter_mut() {
        group.members.retain(|member| member != &board_id);
    }
    config.groups.retain(|g| !g.members.is_empty());

    if let Err(e) = config.save() {
        error!(board_id = %board_id, "Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // File saved successfully - now safe to modify memory
    // Send shutdown command (ignore error if actor already stopped)
    let _ = tx.send(BoardCommand::Shutdown).await;

    // Remove from in-memory state
    {
        let mut senders = state.boards.write().await;
        senders.remove(&board_id);
    }

    info!(board_id = %board_id, "Deleted board");
    Ok(StatusCode::NO_CONTENT)
}

async fn update_board(
    State(state): State<SharedState>,
    axum::extract::Path(old_id): axum::extract::Path<String>,
    Json(req): Json<UpdateBoardRequest>,
) -> Result<StatusCode, StatusCode> {
    // Validate: at least one field must be provided
    if req.new_id.is_none() && req.new_ip.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Check if board exists
    let old_sender = {
        let senders = state.boards.read().await;
        senders.get(&old_id).map(|entry| entry.sender.clone())
    };

    let tx = old_sender.ok_or(StatusCode::NOT_FOUND)?;

    // Load config
    let mut config = Config::load().unwrap_or(Config {
        boards: vec![],
        groups: vec![],
    });

    // Find and update board in config
    let board_index = config
        .boards
        .iter()
        .position(|b| b.id == old_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut board_config = config.boards[board_index].clone();

    // Apply updates
    let new_id = req.new_id.clone().unwrap_or(old_id.clone());
    let new_ip = req.new_ip.unwrap_or(board_config.ip.clone());

    board_config.id = new_id.clone();
    board_config.ip = new_ip.clone();

    // Update config and save
    config.boards[board_index] = board_config.clone();

    if let Err(e) = config.save() {
        error!(old_id = %old_id, "Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Shutdown old actor
    let _ = tx.send(BoardCommand::Shutdown).await;

    // Remove old entry from memory
    {
        let mut senders = state.boards.write().await;
        senders.remove(&old_id);
    }

    // Start new actor with updated config
    let (tx, rx) = mpsc::channel::<BoardCommand>(32);
    let actor = BoardActor::new(
        board_config.id.clone(),
        board_config.ip.clone(),
        state.broadcast_tx.clone(),
    );

    tokio::spawn(async move {
        let _ = actor.run(rx).await;
    });

    // Add new entry to memory
    {
        let mut senders = state.boards.write().await;
        senders.insert(
            board_config.id.clone(),
            BoardEntry {
                sender: tx,
                ip: board_config.ip.clone(),
            },
        );
    }

    info!(old_id = %old_id, new_id = %new_id, "Updated board");
    Ok(StatusCode::OK)
}

async fn sse_handler(
    State(state): State<SharedState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("SSE client connected");

    // Subscribe to the shared broadcast channel
    let rx = state.broadcast_tx.subscribe();

    // Send initial connected event
    let connected_event = SseEvent::Connected {
        message: "Connected to WLED server".to_string(),
    };
    let _ = state.broadcast_tx.send(connected_event);

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(event) => event
            .to_sse_message()
            .ok()
            .map(|data| Ok(Event::default().data(data))),
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn list_boards(
    State(state): State<SharedState>,
) -> Result<Json<Vec<BoardState>>, StatusCode> {
    // Collect board entries while holding the lock, then release it
    let board_entries: Vec<(String, String, mpsc::Sender<BoardCommand>)> = {
        let senders_lock = state.boards.read().await;

        senders_lock
            .iter()
            .map(|(id, entry)| (id.clone(), entry.ip.clone(), entry.sender.clone()))
            .collect()
    };

    let mut states = Vec::new();

    let query_tasks: Vec<_> = board_entries
        .into_iter()
        .map(|(board_id, ip, sender)| {
            tokio::spawn(async move {
                let (tx, rx) = tokio::sync::oneshot::channel();

                if sender.send(BoardCommand::GetState(tx)).await.is_err() {
                    warn!(board_id = %board_id, "Failed to send getState to board");
                    return BoardState {
                        id: board_id.clone(),
                        ip: ip.clone(),
                        on: false,
                        brightness: 0,
                        color: [0, 0, 0],
                        effect: 0,
                        speed: 128,
                        intensity: 128,
                        connected: false,
                        led_count: None,
                        max_leds: None,
                        is_group: None,
                        member_ids: None,
                    };
                }
                match tokio::time::timeout(tokio::time::Duration::from_millis(100), rx).await {
                    Ok(Ok(s)) => s,
                    Ok(Err(_)) | Err(_) => {
                        warn!(board_id = %board_id, "Board timed out or channel closed");
                        BoardState {
                            id: board_id.clone(),
                            ip: ip.clone(),
                            on: false,
                            brightness: 0,
                            color: [0, 0, 0],
                            effect: 0,
                            speed: 128,
                            intensity: 128,
                            connected: false,
                            led_count: None,
                            max_leds: None,
                            is_group: None,
                            member_ids: None,
                        }
                    }
                }
            })
        })
        .collect();

    for task in query_tasks {
        if let Ok(state) = task.await {
            states.push(state);
        }
    }

    // Add groups from config with calculated state
    if let Ok(config) = Config::load() {
        for group in config.groups {
            // Get member board states
            let member_states: Vec<BoardState> = group
                .members
                .iter()
                .filter_map(|member_id| {
                    states
                        .iter()
                        .find(|s| s.id == *member_id && s.is_group != Some(true))
                })
                .cloned()
                .collect();

            // Calculate group state from members (same logic as frontend)
            let group_on = if !member_states.is_empty() {
                member_states.iter().any(|m| m.on) // Group is ON if any member is ON
            } else {
                false
            };

            let group_connected = if !member_states.is_empty() {
                member_states.iter().any(|m| m.connected) // Group is connected if any member is connected
            } else {
                false
            };

            let group_color = if !member_states.is_empty() {
                member_states[0].color
            } else {
                [255, 255, 255]
            };

            let group_brightness = if !member_states.is_empty() {
                member_states[0].brightness
            } else {
                128
            };

            let group_effect = if !member_states.is_empty() {
                member_states[0].effect
            } else {
                0
            };

            let group_speed = if !member_states.is_empty() {
                member_states[0].speed
            } else {
                128
            };

            let group_intensity = if !member_states.is_empty() {
                member_states[0].intensity
            } else {
                128
            };

            states.push(BoardState {
                id: group.id,
                ip: String::new(),
                on: group_on,
                brightness: group_brightness,
                color: group_color,
                effect: group_effect,
                speed: group_speed,
                intensity: group_intensity,
                connected: group_connected,
                led_count: None,
                max_leds: None,
                is_group: Some(true),
                member_ids: Some(group.members),
            });
        }
    }

    Ok(Json(states))
}

async fn set_board_power(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
    Json(payload): Json<PowerRequest>,
) -> Result<Json<BoardState>, StatusCode> {
    let sender = {
        let senders_lock = state.boards.read().await;

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    // Send set power command
    sender
        .send(BoardCommand::SetPower(payload.on, payload.transition))
        .await
        .map_err(|e| {
            eprintln!(
                "Failed to send power command to board '{}': {:?}",
                board_id, e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Get updated state
    let (tx, rx) = tokio::sync::oneshot::channel();
    sender.send(BoardCommand::GetState(tx)).await.map_err(|e| {
        eprintln!(
            "Failed to send get state command to board '{}': {:?}",
            board_id, e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let board_state = match tokio::time::timeout(tokio::time::Duration::from_secs(1), rx).await {
        Ok(Ok(state)) => state,
        Ok(Err(_)) => {
            eprintln!("Board '{}' state channel closed", board_id);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Err(_) => {
            eprintln!("Board '{}' state query timed out", board_id);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    Ok(Json(board_state))
}

async fn set_brightness(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let bri = payload["brightness"].as_u64().unwrap_or(128) as u8;
    let transition = payload["transition"].as_u64().unwrap_or(0) as u8;

    let sender = {
        let senders_lock = state.boards.read().await;

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::SetBrightness(bri, transition))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn set_color(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let r = payload["r"].as_u64().unwrap_or(255) as u8;
    let g = payload["g"].as_u64().unwrap_or(255) as u8;
    let b = payload["b"].as_u64().unwrap_or(255) as u8;
    let transition = payload["transition"].as_u64().unwrap_or(0) as u8;

    let sender = {
        let senders_lock = state.boards.read().await;

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::SetColor {
            r,
            g,
            b,
            transition,
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn set_effect(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let effect = payload["effect"].as_u64().unwrap_or(0) as u8;
    let transition = payload["transition"].as_u64().unwrap_or(0) as u8;

    let sender = {
        let senders_lock = state.boards.read().await;

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::SetEffect(effect, transition))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn set_speed(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let speed = payload["speed"].as_u64().unwrap_or(128) as u8;
    let transition = payload["transition"].as_u64().unwrap_or(0) as u8;

    let sender = {
        let senders_lock = state.boards.read().await;

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::SetSpeed(speed, transition))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn set_intensity(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let intensity = payload["intensity"].as_u64().unwrap_or(128) as u8;
    let transition = payload["transition"].as_u64().unwrap_or(0) as u8;

    let sender = {
        let senders_lock = state.boards.read().await;

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::SetIntensity(intensity, transition))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn set_preset(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let preset = payload["preset"].as_u64().unwrap_or(1) as u8;
    let transition = payload["transition"].as_u64().unwrap_or(0) as u8;

    let received_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    println!(
        "🎨 [{}ms] HTTP received: board='{}' preset={} transition={}",
        received_at, board_id, preset, transition
    );

    let sender = {
        let senders_lock = state.boards.read().await;

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::SetPreset(preset, transition))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let sent_to_actor_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    println!(
        "✅ [{}ms] Sent to actor: board='{}' preset={}",
        sent_to_actor_at, board_id, preset
    );

    Ok(StatusCode::OK)
}

async fn set_led_count(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let led_count = payload["led_count"].as_u64().unwrap_or(30) as u16;

    let sender = {
        let senders_lock = state.boards.read().await;

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::SetLedCount(led_count))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn reset_segment(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
) -> Result<StatusCode, StatusCode> {
    let sender = {
        let senders_lock = state.boards.read().await;

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::ResetSegment)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn get_board_presets(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Get board IP
    let board_ip = {
        let senders_lock = state.boards.read().await;
        senders_lock
            .get(&board_id)
            .map(|entry| entry.ip.clone())
            .ok_or((StatusCode::NOT_FOUND, "Board not found".to_string()))?
    };

    let client = reqwest::Client::new();
    let url = format!("http://{}/presets.json", board_ip);

    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                // Get raw text first to handle potential parsing issues
                let text = response.text().await.map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!(
                            "Failed to read response from board {} ({}): {}",
                            board_id, board_ip, e
                        ),
                    )
                })?;

                // Try to parse as JSON - if it fails, return empty object
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(presets_json) => Ok(Json(presets_json)),
                    Err(e) => {
                        // Log the error but return empty presets instead of failing
                        eprintln!("Warning: Board {} ({}) returned invalid JSON for presets: {}. Response: {}",
                            board_id, board_ip, e, text);
                        Ok(Json(serde_json::json!({})))
                    }
                }
            } else if response.status() == reqwest::StatusCode::NOT_FOUND {
                // Board doesn't have presets.json endpoint - return empty object
                Ok(Json(serde_json::json!({})))
            } else {
                Err((
                    StatusCode::BAD_GATEWAY,
                    format!(
                        "Board {} ({}) returned HTTP {} when fetching presets",
                        board_id,
                        board_ip,
                        response.status()
                    ),
                ))
            }
        }
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            format!(
                "Failed to connect to board {} ({}) for presets: {}",
                board_id, board_ip, e
            ),
        )),
    }
}

async fn sync_presets_to_board(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            format!("Storage directories not available. Configured paths: programs={}, audio={}, presets={}",
                state.storage_paths.programs.display(),
                state.storage_paths.audio.display(),
                state.storage_paths.presets.display()
            )
        ));
    }

    // Load ALL global presets from centralized storage
    let presets = WledPreset::load_all(&state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "Failed to load presets from {}: {}",
                state.storage_paths.presets.display(),
                e
            ),
        )
    })?;

    // Get board IP
    let board_ip = {
        let senders_lock = state.boards.read().await;
        senders_lock
            .get(&board_id)
            .map(|entry| entry.ip.clone())
            .ok_or((StatusCode::NOT_FOUND, "Board not found".to_string()))?
    };

    let client = reqwest::Client::new();
    let mut results = Vec::new();
    let mut success_count = 0;

    // Push each preset to its designated WLED slot
    for preset in &presets {
        let payload = preset.to_wled_json();
        let url = format!("http://{}/json/state", board_ip);

        match client.post(&url).json(&payload).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    success_count += 1;
                    results.push(serde_json::json!({
                        "preset_id": preset.id,
                        "name": preset.name,
                        "wled_slot": preset.wled_slot,
                        "status": "success"
                    }));
                } else {
                    results.push(serde_json::json!({
                        "preset_id": preset.id,
                        "name": preset.name,
                        "wled_slot": preset.wled_slot,
                        "status": "failed",
                        "error": format!("HTTP {}", response.status())
                    }));
                }
            }
            Err(e) => {
                results.push(serde_json::json!({
                    "preset_id": preset.id,
                    "name": preset.name,
                    "wled_slot": preset.wled_slot,
                    "status": "failed",
                    "error": format!("Network error to {}: {}", board_ip, e)
                }));
            }
        }

        // Delay to avoid overwhelming board's filesystem (prevents corruption)
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    Ok(Json(serde_json::json!({
        "synced": success_count,
        "total": presets.len(),
        "results": results
    })))
}

async fn send_osc(Json(payload): Json<OscRequest>) -> Result<StatusCode, StatusCode> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| {
        eprintln!("Failed to create OSC socket: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let packet = rosc::encoder::encode(&rosc::OscPacket::Message(rosc::OscMessage {
        addr: payload.address.clone(),
        args: vec![],
    }))
    .map_err(|e| {
        eprintln!("Failed to encode OSC message '{}': {}", payload.address, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let target = "192.168.1.242:9595";
    socket.send_to(&packet, target).map_err(|e| {
        eprintln!("Failed to send OSC message to {}: {}", target, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::OK)
}

async fn create_group(
    State(state): State<SharedState>,
    Json(payload): Json<CreateGroupRequest>,
) -> Result<StatusCode, StatusCode> {
    info!(group_id = %payload.id, members = ?payload.members, "Creating group");

    // Validate that all member IDs exist in boards
    {
        let senders = state.boards.read().await;

        for member_id in &payload.members {
            if !senders.contains_key(member_id) {
                warn!(group_id = %payload.id, member_id = %member_id, "Group creation failed: member not found");
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }

    // Load config and check for duplicate group ID
    let mut config = Config::load().unwrap_or(Config {
        boards: vec![],
        groups: vec![],
    });

    if config.groups.iter().any(|g| g.id == payload.id) {
        return Err(StatusCode::CONFLICT);
    }

    // Check if group ID conflicts with any board ID
    {
        let senders = state.boards.read().await;

        if senders.contains_key(&payload.id) {
            return Err(StatusCode::CONFLICT);
        }
    }

    // Add group to config
    config.groups.push(config::GroupConfig {
        id: payload.id.clone(),
        members: payload.members.clone(),
    });

    // Save config
    if let Err(e) = config.save() {
        error!(group_id = %payload.id, "Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    info!(group_id = %payload.id, "Created group");
    Ok(StatusCode::CREATED)
}

async fn delete_group(
    State(_state): State<SharedState>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
) -> Result<StatusCode, StatusCode> {
    // Load config
    let mut config = Config::load().unwrap_or(Config {
        boards: vec![],
        groups: vec![],
    });

    // Check if group exists
    if !config.groups.iter().any(|g| g.id == group_id) {
        return Err(StatusCode::NOT_FOUND);
    }

    // Remove group
    config.groups.retain(|g| g.id != group_id);

    // Save config
    if let Err(e) = config.save() {
        error!(group_id = %group_id, "Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    info!(group_id = %group_id, "Deleted group");
    Ok(StatusCode::NO_CONTENT)
}

async fn update_group(
    State(state): State<SharedState>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    Json(req): Json<UpdateGroupRequest>,
) -> Result<StatusCode, StatusCode> {
    // Validate that all member IDs exist in boards
    {
        let senders = state.boards.read().await;

        for member_id in &req.members {
            if !senders.contains_key(member_id) {
                warn!(group_id = %group_id, member_id = %member_id, "Group update failed: member not found");
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }

    // Load config
    let mut config = Config::load().unwrap_or(Config {
        boards: vec![],
        groups: vec![],
    });

    // Check if new ID conflicts with existing boards or groups (if ID is changing)
    if req.id != group_id {
        // Check conflict with other groups
        if config.groups.iter().any(|g| g.id == req.id) {
            return Err(StatusCode::CONFLICT);
        }

        // Check conflict with boards
        {
            let senders = state.boards.read().await;

            if senders.contains_key(&req.id) {
                return Err(StatusCode::CONFLICT);
            }
        }
    }

    // Find and update group
    let group = config
        .groups
        .iter_mut()
        .find(|g| g.id == group_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    group.id = req.id;
    group.members = req.members;

    // Save config
    if let Err(e) = config.save() {
        error!(group_id = %group_id, "Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    info!(group_id = %group_id, "Updated group");
    Ok(StatusCode::OK)
}

// Group command handlers

async fn set_group_power(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
    Json(payload): Json<PowerRequest>,
) -> Result<Json<GroupOperationResult>, StatusCode> {
    match group::execute_group_command(
        state,
        &group_id,
        GroupCommand::SetPower(payload.on, payload.transition),
    )
    .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            error!(group_id = %group_id, "Error setting group power: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn set_group_brightness(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
    Json(payload): Json<GroupBrightnessRequest>,
) -> Result<Json<GroupOperationResult>, StatusCode> {
    match group::execute_group_command(
        state,
        &group_id,
        GroupCommand::SetBrightness(payload.brightness, payload.transition),
    )
    .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            error!(group_id = %group_id, "Error setting group brightness: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn set_group_color(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
    Json(payload): Json<GroupColorRequest>,
) -> Result<Json<GroupOperationResult>, StatusCode> {
    match group::execute_group_command(
        state,
        &group_id,
        GroupCommand::SetColor {
            r: payload.r,
            g: payload.g,
            b: payload.b,
            transition: payload.transition,
        },
    )
    .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            error!(group_id = %group_id, "Error setting group color: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn set_group_effect(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
    Json(payload): Json<GroupEffectRequest>,
) -> Result<Json<GroupOperationResult>, StatusCode> {
    match group::execute_group_command(
        state,
        &group_id,
        GroupCommand::SetEffect(payload.effect, payload.transition),
    )
    .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            error!(group_id = %group_id, "Error setting group effect: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn set_group_preset(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
    Json(payload): Json<GroupPresetRequest>,
) -> Result<Json<GroupOperationResult>, StatusCode> {
    let received_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    println!(
        "🎨 [{}ms] HTTP received: group='{}' preset={} transition={}",
        received_at, group_id, payload.preset, payload.transition
    );

    match group::execute_group_command(
        state,
        &group_id,
        GroupCommand::SetPreset(payload.preset, payload.transition),
    )
    .await
    {
        Ok(result) => {
            let sent_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            println!(
                "✅ [{}ms] Group command executed: group='{}' preset={}",
                sent_at, group_id, payload.preset
            );
            Ok(Json(result))
        }
        Err(e) => {
            error!(group_id = %group_id, "Error setting group preset: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn sync_presets_to_group(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    // Get group member boards from frontend groups (stored in localStorage, so we need to get from request)
    // For now, we'll implement a simpler version that syncs to all boards
    let all_boards: Vec<String> = {
        let boards_lock = state.boards.read().await;
        boards_lock.keys().cloned().collect()
    };

    if all_boards.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No boards available".to_string()));
    }

    // Load ALL global presets from centralized storage
    let presets = WledPreset::load_all(&state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load presets: {}", e),
        )
    })?;

    let mut all_results = Vec::new();
    let mut total_success = 0;

    // Sync to each board in parallel
    let mut tasks = Vec::new();

    for board_id in all_boards {
        let state_clone = state.clone();
        let presets_clone = presets.clone();

        let task = tokio::spawn(async move {
            sync_presets_to_board_internal(state_clone, &board_id, presets_clone).await
        });
        tasks.push(task);
    }

    // Wait for all sync operations to complete
    let results = futures::future::join_all(tasks).await;

    for (index, result) in results.into_iter().enumerate() {
        match result {
            Ok(Ok(sync_result)) => {
                let sync_json = sync_result.0;
                if let Ok(sync_data) = serde_json::from_value::<serde_json::Value>(sync_json) {
                    if let Some(synced) = sync_data.get("synced").and_then(|v| v.as_u64()) {
                        total_success += synced as usize;
                    }
                    all_results.push(serde_json::json!({
                        "board_index": index,
                        "result": sync_data
                    }));
                }
            }
            Ok(Err(e)) => {
                all_results.push(serde_json::json!({
                    "board_index": index,
                    "error": e.1
                }));
            }
            Err(e) => {
                all_results.push(serde_json::json!({
                    "board_index": index,
                    "error": format!("Task failed: {}", e)
                }));
            }
        }
    }

    Ok(Json(serde_json::json!({
        "group_id": group_id,
        "total_presets": presets.len(),
        "total_successful_syncs": total_success,
        "member_results": all_results
    })))
}

// Helper function to sync presets to a specific board (extracted from sync_presets_to_board)
async fn sync_presets_to_board_internal(
    state: SharedState,
    board_id: &str,
    presets: Vec<WledPreset>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Get board IP
    let board_ip = {
        let senders_lock = state.boards.read().await;
        senders_lock
            .get(board_id)
            .map(|entry| entry.ip.clone())
            .ok_or((StatusCode::NOT_FOUND, "Board not found".to_string()))?
    };

    let client = reqwest::Client::new();
    let mut results = Vec::new();
    let mut success_count = 0;

    // Push each preset to its designated WLED slot
    for preset in &presets {
        let payload = preset.to_wled_json();
        let url = format!("http://{}/json/state", board_ip);

        match client.post(&url).json(&payload).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    success_count += 1;
                    results.push(serde_json::json!({
                        "preset_id": preset.id,
                        "name": preset.name,
                        "wled_slot": preset.wled_slot,
                        "status": "success"
                    }));
                } else {
                    results.push(serde_json::json!({
                        "preset_id": preset.id,
                        "name": preset.name,
                        "wled_slot": preset.wled_slot,
                        "status": "failed",
                        "error": format!("HTTP {}", response.status())
                    }));
                }
            }
            Err(e) => {
                results.push(serde_json::json!({
                    "preset_id": preset.id,
                    "name": preset.name,
                    "wled_slot": preset.wled_slot,
                    "status": "failed",
                    "error": format!("Network error to {}: {}", board_ip, e)
                }));
            }
        }

        // Delay to avoid overwhelming board's filesystem (prevents corruption)
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    Ok(Json(serde_json::json!({
        "synced": success_count,
        "total": presets.len(),
        "results": results
    })))
}

// Preset management handlers

async fn save_preset(
    State(state): State<SharedState>,
    Json(req): Json<SavePresetRequest>,
) -> Result<Json<WledPreset>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    // Load existing presets
    let mut presets = WledPreset::load_all(&state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "Failed to load presets from {}: {}",
                state.storage_paths.presets.display(),
                e
            ),
        )
    })?;

    // Find next available slot (or use provided slot if specified)
    let wled_slot = if req.wled_slot > 0 {
        // Validate provided slot number
        if req.wled_slot < 1 || req.wled_slot > 250 {
            return Err((
                StatusCode::BAD_REQUEST,
                "wled_slot must be between 1 and 250".to_string(),
            ));
        }

        // Check if slot is already used
        if let Some(existing) = presets.iter().find(|p| p.wled_slot == req.wled_slot) {
            return Err((
                StatusCode::CONFLICT,
                format!(
                    "Slot {} is already used by preset '{}'",
                    req.wled_slot, existing.name
                ),
            ));
        }
        req.wled_slot
    } else {
        // Auto-assign next available slot
        let mut next_slot = 1;
        while presets.iter().any(|p| p.wled_slot == next_slot) && next_slot <= 250 {
            next_slot += 1;
        }

        if next_slot > 250 {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "No available preset slots (1-250)".to_string(),
            ));
        }

        next_slot
    };

    // Use provided state or default
    let preset_state = req.state.unwrap_or(PresetState {
        on: true,
        brightness: 255,
        color: [255, 255, 255],
        effect: 0,
        speed: 128,
        intensity: 128,
    });

    // Create new preset
    let preset = WledPreset {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        wled_slot,
        description: req.description,
        state: preset_state,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // Add to collection and save
    presets.push(preset.clone());
    WledPreset::save_all(&presets, &state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "Failed to save {} presets to {}: {}",
                presets.len(),
                state.storage_paths.presets.display(),
                e
            ),
        )
    })?;

    Ok(Json(preset))
}

async fn list_presets(
    State(state): State<SharedState>,
) -> Result<Json<Vec<WledPreset>>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let presets = WledPreset::load_all(&state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "Failed to list presets from {}: {}",
                state.storage_paths.presets.display(),
                e
            ),
        )
    })?;

    Ok(Json(presets))
}

async fn get_preset(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<WledPreset>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let preset = WledPreset::find_by_id(&id, &state.storage_paths.presets)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error loading presets while searching for '{}': {}", id, e),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, format!("Preset {} not found", id)))?;

    Ok(Json(preset))
}

async fn delete_preset(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    // Load all presets
    let mut presets = WledPreset::load_all(&state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load presets for deletion of '{}': {}", id, e),
        )
    })?;

    // Find and remove
    let initial_len = presets.len();
    presets.retain(|p| p.id != id);

    if presets.len() == initial_len {
        return Err((StatusCode::NOT_FOUND, format!("Preset {} not found", id)));
    }

    // Save updated collection
    WledPreset::save_all(&presets, &state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save presets after deleting '{}': {}", id, e),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Audio Endpoints
// ============================================================================

/// Upload audio file from base64 data URL
async fn upload_audio(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(payload): Json<UploadAudioRequest>,
) -> Result<Json<UploadAudioResponse>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let filename = audio::AudioFile::save(&id, &payload.data_url, &state.storage_paths.audio)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    info!("Uploaded audio file: {}", filename);

    Ok(Json(UploadAudioResponse {
        audio_file: filename,
    }))
}

/// Download/stream audio file
async fn get_audio(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    if !state.storage_paths.is_available() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let bytes = audio::AudioFile::load(&id, &state.storage_paths.audio).map_err(|e| {
        error!("Failed to load audio file '{}': {}", id, e);
        StatusCode::NOT_FOUND
    })?;

    let mime_type = audio::AudioFile::extension_to_mime(&id).to_string();

    Ok(([(axum::http::header::CONTENT_TYPE, mime_type)], bytes))
}

/// Delete audio file
async fn delete_audio(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    if !state.storage_paths.is_available() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    audio::AudioFile::delete(&id, &state.storage_paths.audio).map_err(|e| {
        error!("Failed to delete audio file '{}': {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Deleted audio file: {}", id);

    Ok(StatusCode::NO_CONTENT)
}
