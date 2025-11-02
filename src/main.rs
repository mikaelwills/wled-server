use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post, put},
    Json, Router,
};
use futures::Stream;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::StreamExt;

mod actor;
mod board;
mod config;
mod sse;
mod types;

use actor::BoardActor;
use board::{BoardCommand, BoardState};
use config::Config;
use sse::SseEvent;
use types::{AppState, BoardEntry, CreateGroupRequest, OscRequest, RegisterBoardRequest, SharedState, UpdateBoardRequest, UpdateGroupRequest};

use http::Method;
use tower_http::cors::CorsLayer;

use std::net::UdpSocket;

#[tokio::main]
async fn main() {
    // Create global broadcast channel for SSE events
    let (broadcast_tx, _) = broadcast::channel::<SseEvent>(100);

    let state: SharedState = Arc::new(AppState {
        boards: Arc::new(RwLock::new(HashMap::new())),
        broadcast_tx: Arc::new(broadcast_tx),
    });

    // Load boards from config if available
    match Config::load() {
        Ok(config) => {
            println!("Loaded {} board(s) from boards.toml", config.boards.len());
            for board_config in config.boards {
                let (tx, rx) = mpsc::channel(100);
                if let Ok(mut senders) = state.boards.write() {
                    senders.insert(
                        board_config.id.clone(),
                        BoardEntry {
                            ip: board_config.ip.clone(),
                            sender: tx,
                        },
                    );
                }
                let actor = BoardActor::new(
                    board_config.id,
                    board_config.ip,
                    state.broadcast_tx.clone(),
                );
                tokio::spawn(async move {
                    if let Err(e) = actor.run(rx).await {
                        eprintln!("Actor error: {}", e);
                    }
                });
            }
        }
        Err(e) => {
            eprintln!("Warning: Could not load boards.toml: {}", e);
            eprintln!("Server starting with no boards configured");
        }
    }

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(tower_http::cors::Any);

    let app = Router::new()
        .route("/", get(hello))
        .route("/boards", get(list_boards).post(register_board))
        .route("/boards/:id", put(update_board).delete(delete_board))
        .route("/groups", post(create_group))
        .route("/groups/:id", put(update_group).delete(delete_group))
        .route("/board/:id/toggle", post(toggle_power))
        .route("/board/:id/brightness", post(set_brightness))
        .route("/board/:id/color", post(set_color))
        .route("/board/:id/effect", post(set_effect))
        .route("/board/:id/preset", post(set_preset))
        .route("/events", get(sse_handler))
        .route("/osc", post(send_osc))
        .layer(cors)
        .with_state(state.clone());

    let listener = match tokio::net::TcpListener::bind("0.0.0.0:3010").await {
        Ok(l) => {
            println!("Server running on http://0.0.0.0:3010");
            l
        }
        Err(e) => {
            eprintln!("Failed to bind to 0.0.0.0:3010: {}", e);
            eprintln!("Is the port already in use?");
            return;
        }
    };

    match axum::serve(listener, app).await {
        Ok(_) => println!("Server stopped properly"),
        Err(e) => eprintln!("Server error: {}", e),
    }
}

async fn hello() -> &'static str {
    "WLED Server Running"
}

async fn register_board(
    State(state): State<SharedState>,
    Json(payload): Json<RegisterBoardRequest>,
) -> Result<StatusCode, StatusCode> {
    let board_id = payload.id.clone();
    let (tx, rx) = mpsc::channel(100);

    {
        let mut senders = state
            .boards
            .write()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
            eprintln!("Actor error for {}: {}", board_id_for_spawn, e);
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
        eprintln!("Failed to save boards.toml: {}", e);
        // Note: Board is already registered in memory, so we return success
        // but log the error. Alternative would be to roll back the registration.
    }

    println!("Registered new board: {}", board_id);
    Ok(StatusCode::CREATED)
}

async fn delete_board(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
) -> Result<StatusCode, StatusCode> {
    // Check if board exists in memory
    let sender = {
        let senders = state
            .boards
            .read()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
        eprintln!("Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // File saved successfully - now safe to modify memory
    // Send shutdown command (ignore error if actor already stopped)
    let _ = tx.send(BoardCommand::Shutdown).await;

    // Remove from in-memory state
    let mut senders = state
        .boards
        .write()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    senders.remove(&board_id);

    println!("Deleted board: {}", board_id);
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
        let senders = state
            .boards
            .read()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
        eprintln!("Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Shutdown old actor
    let _ = tx.send(BoardCommand::Shutdown).await;

    // Remove old entry from memory
    {
        let mut senders = state
            .boards
            .write()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
        let mut senders = state
            .boards
            .write()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        senders.insert(
            board_config.id.clone(),
            BoardEntry {
                sender: tx,
                ip: board_config.ip.clone(),
            },
        );
    }

    println!("Updated board: {} -> {}", old_id, new_id);
    Ok(StatusCode::OK)
}

async fn sse_handler(
    State(state): State<SharedState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    println!("SSE client connected");

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
        let senders_lock = match state.boards.read() {
            Ok(lock) => lock,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

        senders_lock
            .iter()
            .map(|(id, entry)| (id.clone(), entry.ip.clone(), entry.sender.clone()))
            .collect()
    };

    let mut states = Vec::new();

    for (board_id, ip, sender) in board_entries {
        let (tx, rx) = tokio::sync::oneshot::channel();

        if sender
            .send(BoardCommand::GetState(tx))
            .await
            .is_err()
        {
            eprintln!("Failed to send GetState to board {}", board_id);
            // Return fallback state for unresponsive actor
            states.push(BoardState {
                id: board_id.clone(),
                ip: ip.clone(),
                on: false,
                brightness: 0,
                color: [0, 0, 0],
                effect: 0,
                connected: false,
                is_group: None,
                member_ids: None,
            });
            continue;
        }

        // Add timeout to prevent hanging on unresponsive actors
        let state = match tokio::time::timeout(tokio::time::Duration::from_secs(1), rx).await {
            Ok(Ok(s)) => s,
            Ok(Err(_)) | Err(_) => {
                eprintln!("Board {} timed out or channel closed", board_id);
                // Return fallback state instead of skipping
                BoardState {
                    id: board_id.clone(),
                    ip: ip.clone(),
                    on: false,
                    brightness: 0,
                    color: [0, 0, 0],
                    effect: 0,
                    connected: false,
                    is_group: None,
                    member_ids: None,
                }
            }
        };

        states.push(state);
    }

    // Add groups from config
    if let Ok(config) = Config::load() {
        for group in config.groups {
            states.push(BoardState::new_group(group.id, group.members));
        }
    }

    Ok(Json(states))
}

async fn toggle_power(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
) -> Result<Json<BoardState>, StatusCode> {
    let sender = {
        let senders_lock = match state.boards.read() {
            Ok(lock) => lock,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    // Send toggle command
    sender
        .send(BoardCommand::TogglePower)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get updated state
    let (tx, rx) = tokio::sync::oneshot::channel();
    sender
        .send(BoardCommand::GetState(tx))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let board_state = match tokio::time::timeout(tokio::time::Duration::from_secs(1), rx).await {
        Ok(Ok(state)) => state,
        Ok(Err(_)) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    Ok(Json(board_state))
}

async fn set_brightness(
    State(state): State<SharedState>,
    axum::extract::Path(board_id): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let bri = payload["brightness"].as_u64().unwrap_or(128) as u8;

    let sender = {
        let senders_lock = match state.boards.read() {
            Ok(lock) => lock,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::SetBrightness(bri))
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

    let sender = {
        let senders_lock = match state.boards.read() {
            Ok(lock) => lock,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::SetColor { r, g, b })
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

    let sender = {
        let senders_lock = match state.boards.read() {
            Ok(lock) => lock,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::SetEffect(effect))
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

    let sender = {
        let senders_lock = match state.boards.read() {
            Ok(lock) => lock,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::SetPreset(preset))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn send_osc(Json(payload): Json<OscRequest>) -> Result<StatusCode, StatusCode> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

async fn create_group(
    State(state): State<SharedState>,
    Json(payload): Json<CreateGroupRequest>,
) -> Result<StatusCode, StatusCode> {
    // Validate that all member IDs exist in boards
    {
        let senders = state
            .boards
            .read()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        for member_id in &payload.members {
            if !senders.contains_key(member_id) {
                eprintln!("Group creation failed: member '{}' not found", member_id);
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

    // Add group to config
    config.groups.push(config::GroupConfig {
        id: payload.id.clone(),
        members: payload.members.clone(),
    });

    // Save config
    if let Err(e) = config.save() {
        eprintln!("Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    println!("Created group: {}", payload.id);
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
        eprintln!("Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    println!("Deleted group: {}", group_id);
    Ok(StatusCode::NO_CONTENT)
}

async fn update_group(
    State(state): State<SharedState>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    Json(req): Json<UpdateGroupRequest>,
) -> Result<StatusCode, StatusCode> {
    // Validate that all member IDs exist in boards
    {
        let senders = state
            .boards
            .read()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        for member_id in &req.members {
            if !senders.contains_key(member_id) {
                eprintln!("Group update failed: member '{}' not found", member_id);
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }

    // Load config
    let mut config = Config::load().unwrap_or(Config {
        boards: vec![],
        groups: vec![],
    });

    // Find and update group
    let group = config
        .groups
        .iter_mut()
        .find(|g| g.id == group_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    group.members = req.members;

    // Save config
    if let Err(e) = config.save() {
        eprintln!("Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    println!("Updated group: {}", group_id);
    Ok(StatusCode::OK)
}
