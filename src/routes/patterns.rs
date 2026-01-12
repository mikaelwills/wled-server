use std::collections::HashMap;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::config::PatternType;
use crate::pattern::generate_sequence;
use crate::pattern_engine::{BoardInfo, PatternCommand};
use crate::types::SharedState;

#[derive(Serialize)]
pub struct PatternPresetResponse {
    pub name: String,
    pub pattern: String,
    pub color: [u8; 3],
}

pub async fn list_pattern_presets(
    State(state): State<SharedState>,
) -> Json<Vec<PatternPresetResponse>> {
    let cfg = state.config.lock().await;
    let presets: Vec<PatternPresetResponse> = cfg.pattern_presets.iter().map(|p| {
        PatternPresetResponse {
            name: p.name.clone(),
            pattern: format!("{:?}", p.pattern).to_lowercase(),
            color: p.colour,
        }
    }).collect();
    Json(presets)
}

#[derive(Deserialize)]
pub struct PatternStartRequest {
    pub preset: String,
    pub target: String,
    pub bpm: f64,
    pub sync_rate: f64,
}

pub async fn start_pattern(
    State(state): State<SharedState>,
    Json(req): Json<PatternStartRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let cfg = state.config.lock().await;
    let online_ips = state.connected_ips.read().await;

    let preset = cfg
        .pattern_presets
        .iter()
        .find(|p| p.name == req.preset)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Pattern '{}' not found", req.preset)))?
        .clone();

    let group = cfg
        .find_group(&req.target)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Group '{}' not found", req.target)))?
        .clone();

    let all_boards = cfg.get_target_boards(&req.target);
    let mut boards: HashMap<String, BoardInfo> = HashMap::new();

    for board in &all_boards {
        if online_ips.contains(&board.ip) {
            boards.insert(
                board.id.clone(),
                BoardInfo {
                    ip: board.ip.clone(),
                    universe: board.universe.unwrap_or(1),
                    led_count: board.led_count.unwrap_or(60) as usize,
                },
            );
        }
    }

    drop(online_ips);
    drop(cfg);

    let online_members: Vec<String> = group.members.iter()
        .filter(|m| boards.contains_key(*m))
        .cloned()
        .collect();

    if online_members.is_empty() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "No online boards in group".to_string()));
    }

    info!("Pattern: {}/{} boards online", online_members.len(), group.members.len());

    let sequence = generate_sequence(&online_members, &preset.pattern, req.bpm, req.sync_rate);

    let is_random = preset.pattern == PatternType::Random;
    let is_ping_pong = preset.pattern == PatternType::PingPong;

    state.pattern_engine.send_command(PatternCommand::Start {
        sequence,
        color: preset.colour,
        boards,
        is_random,
        is_ping_pong,
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    info!("Pattern started: {} @ {} BPM -> {}", req.preset, req.bpm, req.target);
    Ok(StatusCode::OK)
}

pub async fn stop_pattern(
    State(state): State<SharedState>,
) -> Result<StatusCode, (StatusCode, String)> {
    state.pattern_engine.send_command(PatternCommand::Stop)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    info!("Pattern stopped");
    Ok(StatusCode::OK)
}
