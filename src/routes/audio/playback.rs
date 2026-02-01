use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use super::strip_audio_extension;
use crate::audio;
use crate::types::SharedState;

pub async fn load_track(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let id = strip_audio_extension(&id).to_string();

    let mp3_path = state.storage_paths.audio.join(format!("{}.mp3", id));
    let wav_path = state.storage_paths.audio.join(format!("{}.wav", id));

    let audio_path = if mp3_path.exists() {
        mp3_path
    } else if wav_path.exists() {
        wav_path
    } else {
        return Err((StatusCode::NOT_FOUND, format!("Audio file not found: {}", id)));
    };

    let cache_dir = state.storage_paths.audio.join("resampled");
    let _ = std::fs::create_dir_all(&cache_dir);

    let decoded = tokio::task::spawn_blocking(move || audio::decode_file_with_path(&audio_path))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let track = decoded.track.with_source_info(decoded.source_path, cache_dir);

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
pub struct TrackReadinessResponse {
    pub tracks: Vec<TrackReadiness>,
    pub device_sample_rate: u32,
}

#[derive(Serialize)]
pub struct TrackReadiness {
    pub id: String,
    pub ready: bool,
    pub original_rate: u32,
}

pub async fn get_track_readiness(
    State(state): State<SharedState>,
) -> Json<TrackReadinessResponse> {
    let engine = state.audio_engine.lock().await;
    let device_rate = engine.get_device_sample_rate();

    let tracks: Vec<TrackReadiness> = engine
        .get_track_readiness()
        .into_iter()
        .map(|(id, ready)| {
            let original_rate = engine.get_track(&id).map(|t| t.original_rate).unwrap_or(0);
            TrackReadiness { id, ready, original_rate }
        })
        .collect();

    Json(TrackReadinessResponse {
        tracks,
        device_sample_rate: device_rate,
    })
}
