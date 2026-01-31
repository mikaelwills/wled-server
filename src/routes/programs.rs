use axum::{extract::{Path, State}, http::StatusCode, Json};
use tracing::info;

use crate::program;
use crate::types::SharedState;

pub async fn update_program(
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

    {
        let mut programs = state.programs.write().await;
        programs.insert(program.id.clone(), program);
    }

    Ok(StatusCode::OK)
}

pub async fn get_program(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<program::Program>, (StatusCode, String)> {
    let programs = state.programs.read().await;
    let program = programs
        .get(&id)
        .cloned()
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Program {} not found", id)))?;
    Ok(Json(program))
}

pub async fn delete_program(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let program = {
        let programs = state.programs.read().await;
        programs.get(&id).cloned()
    };

    let program = program.ok_or_else(|| (StatusCode::NOT_FOUND, format!("Program {} not found", id)))?;

    {
        let mut engine = state.audio_engine.lock().await;

        if let Some(ref audio_file) = program.audio_file {
            let track_id = std::path::Path::new(audio_file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(audio_file);
            if engine.unload_track(track_id) {
                info!("Unloaded audio track from engine: {}", track_id);
            }
        }

        if let Some(ref guide_file) = program.guide_audio_file {
            let track_id = std::path::Path::new(guide_file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(guide_file);
            if engine.unload_guide_track(track_id) {
                info!("Unloaded guide audio track from engine: {}", track_id);
            }
        }
    }

    program
        .delete(&state.storage_paths.programs, &state.storage_paths.audio)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to delete program '{}': {}", id, e),
            )
        })?;

    {
        let mut programs = state.programs.write().await;
        programs.remove(&id);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn save_program(
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

    {
        let mut programs = state.programs.write().await;
        programs.insert(program.id.clone(), program);
    }

    Ok(StatusCode::CREATED)
}

pub async fn list_programs(
    State(state): State<SharedState>,
) -> Json<Vec<program::Program>> {
    let programs = state.programs.read().await;
    let mut list: Vec<program::Program> = programs.values().cloned().collect();
    list.sort_by_key(|p| p.display_order);
    Json(list)
}

#[derive(serde::Deserialize)]
pub struct PlayProgramRequest {
    #[serde(default)]
    start: f64,
}

pub async fn play_program(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<PlayProgramRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let program = {
        let programs = state.programs.read().await;
        programs.get(&id).cloned()
    };

    let program = program.ok_or_else(|| (StatusCode::NOT_FOUND, format!("Program {} not found", id)))?;

    info!("▶️ Playing program {} @ {}s", program.id, params.start);

    state
        .program_engine
        .play(program, params.start)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::OK)
}

pub async fn stop_program(
    State(state): State<SharedState>,
) -> Result<StatusCode, (StatusCode, String)> {
    println!("🛑🛑🛑 API: /programs/stop called - MANUAL STOP FROM FRONTEND 🛑🛑🛑");

    match state.program_engine.stop().await {
        Ok(_) => {
            println!("🛑 API: stop() succeeded");
            info!("⏹️ Stopped program playback");
            Ok(StatusCode::OK)
        }
        Err(e) => {
            println!("🛑 API: stop() failed: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e))
        }
    }
}

#[derive(serde::Serialize)]
pub struct ReloadResponse {
    pub loaded: usize,
    pub previous: usize,
}

pub async fn reload_programs(
    State(state): State<SharedState>,
) -> Result<Json<ReloadResponse>, (StatusCode, String)> {
    let previous = {
        let programs = state.programs.read().await;
        programs.len()
    };

    info!("🔄 Reloading programs from disk (previous count: {})", previous);

    let new_programs = program::Program::load_all(&state.storage_paths.programs)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load programs: {}", e),
            )
        })?;

    let loaded = new_programs.len();

    {
        let mut programs = state.programs.write().await;
        programs.clear();
        for p in new_programs {
            programs.insert(p.id.clone(), p);
        }
    }

    info!("✅ Reloaded {} program(s) from disk", loaded);

    Ok(Json(ReloadResponse { loaded, previous }))
}
