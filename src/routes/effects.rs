use axum::{extract::State, http::StatusCode, Json};
use tracing::{error, info};

use crate::config;
use crate::effects::EffectType;
use crate::effects_engine::{BoardTarget, EffectConfig, EngineCommand};
use crate::types::{EffectsEngineStartRequest, SharedState};

pub async fn start_effects_engine(
    State(state): State<SharedState>,
    Json(req): Json<EffectsEngineStartRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let cfg = state.config.lock().await;

    let preset = cfg.find_effect_preset(&req.preset).ok_or_else(|| {
        (StatusCode::NOT_FOUND, format!("Preset '{}' not found", req.preset))
    })?;

    let effect_type: EffectType = preset.effect_type.parse().map_err(|e: String| {
        (StatusCode::BAD_REQUEST, e)
    })?;

    let target_boards = cfg.get_target_boards(&req.target);
    if target_boards.is_empty() {
        return Err((StatusCode::NOT_FOUND, format!("Target '{}' not found", req.target)));
    }

    let boards: Vec<BoardTarget> = target_boards
        .iter()
        .map(|b| BoardTarget {
            ip: b.ip.clone(),
            universe: b.universe.unwrap_or(1),
            led_count: b.led_count.unwrap_or(60) as usize,
        })
        .collect();

    let config = EffectConfig {
        effect_type,
        bpm: req.bpm,
        color: preset.color,
    };

    drop(cfg);

    state
        .effects_engine
        .send_command(EngineCommand::Start { config, boards })
        .map_err(|e| {
            error!("Failed to start effects engine: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    info!("Effects engine started: {} @ {} BPM -> {}", req.preset, req.bpm, req.target);
    Ok(StatusCode::OK)
}

pub async fn stop_effects_engine(
    State(state): State<SharedState>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .effects_engine
        .send_command(EngineCommand::Stop)
        .map_err(|e| {
            error!("Failed to stop effects engine: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    info!("Effects engine stopped");
    Ok(StatusCode::OK)
}

pub async fn list_effect_presets(
    State(state): State<SharedState>,
) -> Json<Vec<config::EffectPreset>> {
    let cfg = state.config.lock().await;
    Json(cfg.effect_presets.clone())
}
