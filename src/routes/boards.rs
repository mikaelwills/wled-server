use axum::{extract::{Path, State}, http::StatusCode, Json};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::actor::BoardActor;
use crate::board::{BoardCommand, BoardState};
use crate::config::{self, Config};
use crate::types::{BoardEntry, PowerRequest, RegisterBoardRequest, SharedState, UpdateBoardRequest, WledPreset};

pub async fn register_board(
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
        state.connected_ips.clone(),
        state.performance_mode.clone(),
    );
    let board_id_for_spawn = board_id.clone();
    tokio::spawn(async move {
        if let Err(e) = actor.run(rx).await {
            error!(board_id = %board_id_for_spawn, "Actor error: {}", e);
        }
    });

    let mut config = Config::load().unwrap_or(Config {
        loopy_pro: config::LoopyProConfig::default(),
        boards: vec![],
        groups: vec![],
        effect_presets: vec![],
        pattern_presets: vec![],
    });
    config.boards.push(config::BoardConfig {
        id: payload.id,
        ip: payload.ip,
        transition: None,
        led_count: payload.led_count,
        universe: payload.universe,
    });

    if let Err(e) = config.save() {
        warn!("Failed to save boards.toml: {}", e);
    }

    info!(board_id = %board_id, "Registered new board");
    Ok(StatusCode::CREATED)
}

pub async fn delete_board(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let sender = {
        let senders = state.boards.read().await;
        senders.get(&board_id).map(|entry| entry.sender.clone())
    };

    let tx = sender.ok_or(StatusCode::NOT_FOUND)?;

    let mut config = Config::load().unwrap_or(Config {
        loopy_pro: config::LoopyProConfig::default(),
        boards: vec![],
        groups: vec![],
        effect_presets: vec![],
        pattern_presets: vec![],
    });
    config.boards.retain(|b| b.id != board_id);

    for group in config.groups.iter_mut() {
        group.members.retain(|member| member != &board_id);
    }
    config.groups.retain(|g| !g.members.is_empty());

    if let Err(e) = config.save() {
        error!(board_id = %board_id, "Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let _ = tx.send(BoardCommand::Shutdown).await;

    {
        let mut senders = state.boards.write().await;
        senders.remove(&board_id);
    }

    info!(board_id = %board_id, "Deleted board");
    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_board(
    State(state): State<SharedState>,
    Path(old_id): Path<String>,
    Json(req): Json<UpdateBoardRequest>,
) -> Result<StatusCode, StatusCode> {
    if req.new_id.is_none() && req.new_ip.is_none() && req.led_count.is_none() && req.universe.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let old_sender = {
        let senders = state.boards.read().await;
        senders.get(&old_id).map(|entry| entry.sender.clone())
    };

    let tx = old_sender.ok_or(StatusCode::NOT_FOUND)?;

    let mut config = Config::load().unwrap_or(Config {
        loopy_pro: config::LoopyProConfig::default(),
        boards: vec![],
        groups: vec![],
        effect_presets: vec![],
        pattern_presets: vec![],
    });

    let board_index = config
        .boards
        .iter()
        .position(|b| b.id == old_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut board_config = config.boards[board_index].clone();

    let new_id = req.new_id.clone().unwrap_or(old_id.clone());
    let new_ip = req.new_ip.unwrap_or(board_config.ip.clone());

    board_config.id = new_id.clone();
    board_config.ip = new_ip.clone();
    if req.led_count.is_some() {
        board_config.led_count = req.led_count;
    }
    if req.universe.is_some() {
        board_config.universe = req.universe;
    }

    config.boards[board_index] = board_config.clone();

    if let Err(e) = config.save() {
        error!(old_id = %old_id, "Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let _ = tx.send(BoardCommand::Shutdown).await;

    {
        let mut senders = state.boards.write().await;
        senders.remove(&old_id);
    }

    let (tx, rx) = mpsc::channel::<BoardCommand>(32);
    let actor = BoardActor::new_with_config(
        board_config.id.clone(),
        board_config.ip.clone(),
        board_config.transition,
        board_config.led_count,
        board_config.universe,
        state.broadcast_tx.clone(),
        state.connected_ips.clone(),
        state.performance_mode.clone(),
    );

    tokio::spawn(async move {
        let _ = actor.run(rx).await;
    });

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

async fn save_board_transition(board_id: &str, transition: u8) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::load().unwrap_or(Config {
        loopy_pro: config::LoopyProConfig::default(),
        boards: vec![],
        groups: vec![],
        effect_presets: vec![],
        pattern_presets: vec![],
    });

    if let Some(board_config) = config.boards.iter_mut().find(|b| b.id == board_id) {
        board_config.transition = Some(transition);
    } else {
        config.boards.push(config::BoardConfig {
            id: board_id.to_string(),
            ip: String::new(),
            transition: Some(transition),
            led_count: None,
            universe: None,
        });
    }

    config.save()?;
    Ok(())
}

pub async fn set_board_power(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
    Json(payload): Json<PowerRequest>,
) -> Result<Json<BoardState>, StatusCode> {
    let sender = {
        let senders_lock = state.boards.read().await;

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

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

pub async fn set_brightness(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<BoardState>, StatusCode> {
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

    let (tx, rx) = tokio::sync::oneshot::channel();
    sender.send(BoardCommand::GetState(tx)).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let board_state = match tokio::time::timeout(tokio::time::Duration::from_secs(1), rx).await {
        Ok(Ok(state)) => state,
        Ok(Err(_)) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    Ok(Json(board_state))
}

pub async fn set_color(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<BoardState>, StatusCode> {
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

    let (tx, rx) = tokio::sync::oneshot::channel();
    sender.send(BoardCommand::GetState(tx)).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let board_state = match tokio::time::timeout(tokio::time::Duration::from_secs(1), rx).await {
        Ok(Ok(state)) => state,
        Ok(Err(_)) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    Ok(Json(board_state))
}

pub async fn set_effect(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<BoardState>, StatusCode> {
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

    let (tx, rx) = tokio::sync::oneshot::channel();
    sender.send(BoardCommand::GetState(tx)).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let board_state = match tokio::time::timeout(tokio::time::Duration::from_secs(1), rx).await {
        Ok(Ok(state)) => state,
        Ok(Err(_)) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    Ok(Json(board_state))
}

pub async fn set_speed(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<BoardState>, StatusCode> {
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

    let (tx, rx) = tokio::sync::oneshot::channel();
    sender.send(BoardCommand::GetState(tx)).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let board_state = match tokio::time::timeout(tokio::time::Duration::from_secs(1), rx).await {
        Ok(Ok(state)) => state,
        Ok(Err(_)) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    Ok(Json(board_state))
}

pub async fn set_intensity(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<BoardState>, StatusCode> {
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

    let (tx, rx) = tokio::sync::oneshot::channel();
    sender.send(BoardCommand::GetState(tx)).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let board_state = match tokio::time::timeout(tokio::time::Duration::from_secs(1), rx).await {
        Ok(Ok(state)) => state,
        Ok(Err(_)) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    Ok(Json(board_state))
}

pub async fn set_preset(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
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

pub async fn set_led_count(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
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

pub async fn reset_segment(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
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

pub async fn set_transition(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let transition = payload["transition"].as_u64().unwrap_or(0) as u8;

    let sender = {
        let senders_lock = state.boards.read().await;

        match senders_lock.get(&board_id) {
            Some(entry) => entry.sender.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    sender
        .send(BoardCommand::SetTransition(transition))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Err(e) = save_board_transition(&board_id, transition).await {
        error!(board_id = %board_id, "Failed to save transition: {}", e);
    }

    Ok(StatusCode::OK)
}

pub async fn get_board_presets(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
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
                let text = response.text().await.map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!(
                            "Failed to read response from board {} ({}): {}",
                            board_id, board_ip, e
                        ),
                    )
                })?;

                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(presets_json) => Ok(Json(presets_json)),
                    Err(e) => {
                        eprintln!("Warning: Board {} ({}) returned invalid JSON for presets: {}. Response: {}",
                            board_id, board_ip, e, text);
                        Ok(Json(serde_json::json!({})))
                    }
                }
            } else if response.status() == reqwest::StatusCode::NOT_FOUND {
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

pub async fn delete_board_preset(
    State(state): State<SharedState>,
    Path((board_id, slot)): Path<(String, u8)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let board_ip = {
        let senders_lock = state.boards.read().await;
        senders_lock
            .get(&board_id)
            .map(|entry| entry.ip.clone())
            .ok_or((StatusCode::NOT_FOUND, "Board not found".to_string()))?
    };

    let empty_preset = serde_json::json!({
        "psave": slot,
        "n": "",
        "on": false,
        "bri": 0,
        "seg": [{
            "col": [[0, 0, 0]],
            "fx": 0,
            "sx": 128,
            "ix": 128
        }]
    });

    let client = reqwest::Client::new();
    let url = format!("http://{}/json/state", board_ip);

    match client.post(&url).json(&empty_preset).send().await {
        Ok(response) => {
            if response.status().is_success() {
                info!("Deleted preset from slot {} on board {} ({})", slot, board_id, board_ip);
                Ok(StatusCode::OK)
            } else {
                Err((
                    StatusCode::BAD_GATEWAY,
                    format!(
                        "Board {} ({}) returned HTTP {} when deleting preset",
                        board_id, board_ip, response.status()
                    ),
                ))
            }
        }
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            format!(
                "Failed to connect to board {} ({}) to delete preset: {}",
                board_id, board_ip, e
            ),
        )),
    }
}

pub async fn sync_presets_to_board(
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

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    Ok(Json(serde_json::json!({
        "synced": success_count,
        "total": presets.len(),
        "results": results
    })))
}

pub async fn replace_presets_on_board(
    State(state): State<SharedState>,
    Path(board_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage directories not available".to_string()
        ));
    }

    let presets = WledPreset::load_all(&state.storage_paths.presets).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to load presets: {}", e))
    })?;

    let board_ip = {
        let senders_lock = state.boards.read().await;
        senders_lock
            .get(&board_id)
            .map(|entry| entry.ip.clone())
            .ok_or((StatusCode::NOT_FOUND, "Board not found".to_string()))?
    };

    let wled_presets_json = WledPreset::build_wled_presets_file(&presets);
    let json_string = serde_json::to_string(&wled_presets_json).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serialize presets: {}", e))
    })?;

    info!(
        board_id = %board_id,
        board_ip = %board_ip,
        preset_count = presets.len(),
        file_size = json_string.len(),
        "Uploading presets.json to board via /upload"
    );

    let client = reqwest::Client::new();
    let part = reqwest::multipart::Part::bytes(json_string.into_bytes())
        .file_name("presets.json")
        .mime_str("application/json")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create multipart: {}", e)))?;

    let form = reqwest::multipart::Form::new().part("file", part);

    let url = format!("http://{}/upload", board_ip);
    let response = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed to upload to board: {}", e)))?;

    if response.status().is_success() {
        info!(board_id = %board_id, "Presets replaced successfully");
        Ok(Json(serde_json::json!({
            "status": "success",
            "message": "Presets replaced on board",
            "preset_count": presets.len()
        })))
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err((StatusCode::BAD_GATEWAY, format!("Board returned {}: {}", status, body)))
    }
}
