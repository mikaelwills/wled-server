use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::{error, info};

use super::strip_audio_extension;
use crate::audio;
use crate::types::{SharedState, UploadAudioRequest, UploadAudioResponse};

#[derive(Serialize, Deserialize)]
pub struct PeaksData {
    pub peaks: Vec<Vec<f32>>,
    pub duration: f64,
}

#[derive(Serialize)]
pub struct DecodeTestResponse {
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
    pub total_samples: usize,
}

pub async fn test_decode(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<DecodeTestResponse>, (StatusCode, String)> {
    let id = strip_audio_extension(&id);

    let mp3_path = state.storage_paths.audio.join(format!("{}.mp3", id));
    let wav_path = state.storage_paths.audio.join(format!("{}.wav", id));

    let audio_path = if mp3_path.exists() {
        mp3_path
    } else if wav_path.exists() {
        wav_path
    } else {
        return Err((StatusCode::NOT_FOUND, format!("Audio file not found: {}", id)));
    };

    let track = tokio::task::spawn_blocking(move || audio::decode_file(&audio_path))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(DecodeTestResponse {
        sample_rate: track.original_rate,
        channels: track.channels,
        duration_secs: track.duration_secs,
        total_samples: track.original_samples.len(),
    }))
}

pub async fn upload_audio(
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

    let audio_path = state.storage_paths.audio.join(&filename);
    let cache_dir = state.storage_paths.audio.join("resampled");
    let audio_engine = state.audio_engine.clone();
    let id_clone = id.clone();
    let is_guide = id.contains("_guide");
    tokio::spawn(async move {
        let _ = fs::create_dir_all(&cache_dir);
        let cache_dir_clone = cache_dir.clone();
        let decode_result = tokio::task::spawn_blocking(move || audio::decode_file_with_path(&audio_path)).await;
        match decode_result {
            Ok(Ok(decoded)) => {
                let track = decoded.track.with_source_info(decoded.source_path, cache_dir_clone);
                let mut engine = audio_engine.lock().await;
                if is_guide {
                    engine.load_guide_track(id_clone.clone(), track).await;
                    info!("Loaded guide track into audio engine: {}", id_clone);
                } else {
                    engine.load_track(id_clone.clone(), track).await;
                    info!("Loaded track into audio engine: {}", id_clone);
                }
            }
            Ok(Err(e)) => {
                error!("Failed to decode uploaded audio for engine: {}", e);
            }
            Err(e) => {
                error!("Decode task failed: {}", e);
            }
        }
    });

    Ok(Json(UploadAudioResponse {
        audio_file: filename,
    }))
}

pub async fn get_audio(
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

pub async fn delete_audio(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    if !state.storage_paths.is_available() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    info!("[delete_audio] Request to delete audio: {}", id);

    let track_id = std::path::Path::new(&id)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&id);

    {
        let mut engine = state.audio_engine.lock().await;
        let is_guide = track_id.contains("_guide");
        if is_guide {
            if engine.unload_guide_track(track_id) {
                info!("[delete_audio] Unloaded guide track from audio engine: {}", track_id);
            } else {
                info!("[delete_audio] Guide track was not loaded in engine: {}", track_id);
            }
        } else if engine.unload_track(track_id) {
            info!("[delete_audio] Unloaded track from audio engine: {}", track_id);
        } else {
            info!("[delete_audio] Track was not loaded in engine: {}", track_id);
        }
    }

    audio::AudioFile::delete(&id, &state.storage_paths.audio).map_err(|e| {
        error!("Failed to delete audio file '{}': {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("[delete_audio] Deleted audio file from disk: {}", id);

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_peaks(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    if !state.storage_paths.is_available() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let peaks_path = state.storage_paths.audio.join(format!("{}.peaks.json", id));

    if !peaks_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let data = fs::read_to_string(&peaks_path).map_err(|e| {
        error!("Failed to read peaks file '{}': {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        data,
    ))
}

pub async fn save_peaks(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(peaks): Json<PeaksData>,
) -> Result<StatusCode, StatusCode> {
    if !state.storage_paths.is_available() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let peaks_path = state.storage_paths.audio.join(format!("{}.peaks.json", id));

    let json = serde_json::to_string(&peaks).map_err(|e| {
        error!("Failed to serialize peaks: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    fs::write(&peaks_path, json).map_err(|e| {
        error!("Failed to write peaks file '{}': {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Saved peaks for: {}", id);

    Ok(StatusCode::CREATED)
}
