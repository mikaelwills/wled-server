use axum::{extract::{Path, State}, http::StatusCode, Json};
use tracing::{info, warn};

use crate::types::{PresetState, SavePresetRequest, SharedState, WledPreset};

pub async fn save_preset(
    State(state): State<SharedState>,
    Json(req): Json<SavePresetRequest>,
) -> Result<Json<WledPreset>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

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

    let wled_slot = if req.wled_slot > 0 {
        if req.wled_slot < 1 || req.wled_slot > 250 {
            return Err((
                StatusCode::BAD_REQUEST,
                "wled_slot must be between 1 and 250".to_string(),
            ));
        }

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

    let preset_state = req.state.unwrap_or(PresetState {
        on: true,
        brightness: 255,
        color: [255, 255, 255],
        effect: 0,
        speed: 128,
        intensity: 128,
        transition: Some(0),
    });

    let preset = WledPreset {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        wled_slot,
        description: req.description,
        state: preset_state,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

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

    if let Some(board_id) = req.board_id {
        if let Some(board_ip) = {
            let senders_lock = state.boards.read().await;
            senders_lock.get(&board_id).map(|entry| entry.ip.clone())
        } {
            let client = reqwest::Client::new();
            let payload = preset.to_wled_json();
            let url = format!("http://{}/json/state", board_ip);

            match client.post(&url).json(&payload).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        info!("Preset '{}' synced to board {} at slot {}", preset.name, board_id, preset.wled_slot);
                    } else {
                        warn!("Failed to sync preset to board {}: HTTP {}", board_id, response.status());
                    }
                }
                Err(e) => {
                    warn!("Failed to sync preset to board {}: {}", board_id, e);
                }
            }
        }
    }

    Ok(Json(preset))
}

pub async fn list_presets(
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

pub async fn get_preset(
    State(state): State<SharedState>,
    Path(wled_slot): Path<u8>,
) -> Result<Json<WledPreset>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let presets = WledPreset::load_all(&state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error loading presets while searching for slot {}: {}", wled_slot, e),
        )
    })?;

    let preset = presets
        .into_iter()
        .find(|p| p.wled_slot == wled_slot)
        .ok_or((StatusCode::NOT_FOUND, format!("Preset at slot {} not found", wled_slot)))?;

    Ok(Json(preset))
}

pub async fn update_preset(
    State(state): State<SharedState>,
    Path(wled_slot): Path<u8>,
    Json(req): Json<SavePresetRequest>,
) -> Result<Json<WledPreset>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let mut presets = WledPreset::load_all(&state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load presets for update of slot {}: {}", wled_slot, e),
        )
    })?;

    let preset = presets
        .iter_mut()
        .find(|p| p.wled_slot == wled_slot)
        .ok_or((
            StatusCode::NOT_FOUND,
            format!("Preset at slot {} not found", wled_slot),
        ))?;

    preset.name = req.name;
    preset.description = req.description;
    if let Some(state) = req.state {
        preset.state = state;
    }

    let updated_preset = preset.clone();

    WledPreset::save_all(&presets, &state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save presets after updating slot {}: {}", wled_slot, e),
        )
    })?;

    info!("Updated preset '{}' at slot {}", updated_preset.name, wled_slot);

    Ok(Json(updated_preset))
}

pub async fn delete_preset(
    State(state): State<SharedState>,
    Path(wled_slot): Path<u8>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let mut presets = WledPreset::load_all(&state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load presets for deletion of slot {}: {}", wled_slot, e),
        )
    })?;

    let initial_len = presets.len();
    presets.retain(|p| p.wled_slot != wled_slot);

    if presets.len() == initial_len {
        return Err((StatusCode::NOT_FOUND, format!("Preset at slot {} not found", wled_slot)));
    }

    WledPreset::save_all(&presets, &state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save presets after deleting slot {}: {}", wled_slot, e),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}
