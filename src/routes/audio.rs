use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::{error, info};

use crate::audio;
use crate::types::{SharedState, UploadAudioRequest, UploadAudioResponse};

#[derive(Serialize, Deserialize)]
pub struct PeaksData {
    pub peaks: Vec<Vec<f32>>,
    pub duration: f64,
}

#[derive(Deserialize)]
pub struct SelectDeviceRequest {
    pub device_id: Option<String>,
}

pub async fn select_device(
    State(state): State<SharedState>,
    Json(payload): Json<SelectDeviceRequest>,
) -> StatusCode {
    state
        .device_manager
        .select_device(payload.device_id.clone());

    let mut config = state.config.lock().await;
    config.set_preferred_audio_device(payload.device_id.clone());
    let _ = config.save();

    if let Some(ref device_id) = payload.device_id {
        let sample_rate = state.device_manager
            .get_device_sample_rate(device_id)
            .unwrap_or(48000);
        let device_id = device_id.clone();
        let engine = state.audio_engine.clone();

        tokio::spawn(async move {
            let mut engine = engine.lock().await;
            engine.set_device(device_id.clone(), sample_rate).await;
            info!("Audio device switched to: {} ({}Hz)", device_id, sample_rate);
        });
    }

    StatusCode::OK
}

pub async fn list_devices(
    State(state): State<SharedState>,
) -> Json<Vec<crate::audio::AudioDevice>> {
    let devices = state.device_manager.list_devices();
    Json(devices)
}

pub async fn get_audio_settings(
    State(state): State<SharedState>,
) -> Json<crate::config::AudioConfig> {
    let config = state.config.lock().await;
    Json(config.audio.clone())
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
    let id = id
        .strip_suffix(".mp3")
        .or_else(|| id.strip_suffix(".wav"))
        .unwrap_or(&id);

    let audio_path = state.storage_paths.audio.join(format!("{}.mp3", id));

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
    let decode_result = tokio::task::spawn_blocking(move || audio::decode_file(&audio_path)).await;
    match decode_result {
        Ok(Ok(track)) => {
            let mut engine = state.audio_engine.lock().await;
            engine.load_track(id.clone(), track).await;
            info!("Loaded track into audio engine: {}", id);
        }
        Ok(Err(e)) => {
            error!("Failed to decode uploaded audio for engine: {}", e);
        }
        Err(e) => {
            error!("Decode task failed: {}", e);
        }
    }

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

    audio::AudioFile::delete(&id, &state.storage_paths.audio).map_err(|e| {
        error!("Failed to delete audio file '{}': {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Deleted audio file: {}", id);

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

pub async fn load_track(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let id = id
        .strip_suffix(".mp3")
        .or_else(|| id.strip_suffix(".wav"))
        .unwrap_or(&id)
        .to_string();

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

    let mut engine = state.audio_engine.lock().await;
    engine.load_track(id.clone(), track).await;

    info!("Loaded track into audio engine: {}", id);
    Ok(StatusCode::OK)
}

pub async fn play_track(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut engine = state.audio_engine.lock().await;

    if engine.play(&id, None).await {
        info!("Playing track: {}", id);
        Ok(StatusCode::OK)
    } else {
        Err((StatusCode::NOT_FOUND, format!("Track not loaded: {}", id)))
    }
}

pub async fn stop_playback(
    State(state): State<SharedState>,
) -> StatusCode {
    let mut engine = state.audio_engine.lock().await;
    engine.stop().await;
    info!("Stopped playback");
    StatusCode::OK
}

#[derive(Deserialize)]
pub struct SeekRequest {
    pub position: u64,
}

pub async fn seek_playback(
    State(state): State<SharedState>,
    Json(payload): Json<SeekRequest>,
) -> StatusCode {
    let engine = state.audio_engine.lock().await;
    engine.seek(payload.position).await;
    StatusCode::OK
}

pub async fn pause_playback(
    State(state): State<SharedState>,
) -> StatusCode {
    let mut engine = state.audio_engine.lock().await;
    engine.pause().await;
    info!("Paused playback");
    StatusCode::OK
}

pub async fn resume_playback(
    State(state): State<SharedState>,
) -> StatusCode {
    let mut engine = state.audio_engine.lock().await;
    engine.resume().await;
    info!("Resumed playback");
    StatusCode::OK
}

#[derive(Serialize)]
pub struct PlaybackStatusResponse {
    pub position: u64,
    pub position_secs: f64,
    pub state: String,
    pub current_track_id: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub duration_secs: Option<f64>,
    pub loaded_tracks: Vec<String>,
}

pub async fn get_playback_status(
    State(state): State<SharedState>,
) -> Json<PlaybackStatusResponse> {
    let engine = state.audio_engine.lock().await;

    let state_str = match engine.get_state() {
        audio::PlaybackState::Stopped => "stopped",
        audio::PlaybackState::Playing => "playing",
        audio::PlaybackState::Paused => "paused",
    };

    let position = engine.get_position();
    let current_track = engine.get_current_track();
    let current_track_id = engine.get_current_track_id().map(|s| s.to_string());

    let device_rate = engine.get_device_sample_rate();
    let (sample_rate, channels, duration_secs, position_secs) = match &current_track {
        Some(track) => {
            let playback_rate = if device_rate > 0 { device_rate } else { track.original_rate };
            let ch = track.channels;
            let pos_secs = if playback_rate > 0 && ch > 0 {
                position as f64 / (playback_rate as f64 * ch as f64)
            } else {
                0.0
            };
            (Some(track.original_rate), Some(ch), Some(track.duration_secs), pos_secs)
        }
        None => (None, None, None, 0.0),
    };

    Json(PlaybackStatusResponse {
        position,
        position_secs,
        state: state_str.to_string(),
        current_track_id,
        sample_rate,
        channels,
        duration_secs,
        loaded_tracks: engine.loaded_track_ids(),
    })
}

#[derive(Serialize)]
pub struct MemoryStatsResponse {
    pub track_count: usize,
    pub memory_bytes: usize,
    pub memory_mb: f64,
}

pub async fn get_memory_stats(
    State(state): State<SharedState>,
) -> Json<MemoryStatsResponse> {
    let engine = state.audio_engine.lock().await;
    let (track_count, memory_bytes) = engine.memory_usage();

    Json(MemoryStatsResponse {
        track_count,
        memory_bytes,
        memory_mb: memory_bytes as f64 / (1024.0 * 1024.0),
    })
}
