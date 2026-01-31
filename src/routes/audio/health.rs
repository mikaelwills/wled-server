use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::Serialize;

use super::DEFAULT_CHANNELS;
use crate::audio;
use crate::types::SharedState;

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

#[derive(Serialize)]
pub struct HealthResponse {
    pub playing: bool,
    pub sample_rate: u32,
    pub position_samples: u64,
    pub position_secs: f64,
    pub callback_count: u64,
    pub underrun_count: u64,
    pub buffer_size: u32,
    pub samples_delivered: u64,
    pub silence_frames: u64,
    pub max_callback_interval_ms: f64,
    pub late_callbacks: u64,
    pub expected_interval_ms: f64,
}

#[derive(Serialize)]
pub struct ResamplingStatusResponse {
    pub active: bool,
    pub current: u32,
    pub total: u32,
}

pub async fn get_resampling_status(
    State(state): State<SharedState>,
) -> Json<ResamplingStatusResponse> {
    let engine = state.audio_engine.lock().await;
    let progress = engine.get_resampling_progress();
    let (current, total) = progress.get();

    Json(ResamplingStatusResponse {
        active: progress.is_active(),
        current,
        total,
    })
}

pub async fn reset_engine_health(
    State(state): State<SharedState>,
) -> StatusCode {
    let engine = state.audio_engine.lock().await;
    engine.reset_health_stats();
    StatusCode::OK
}

pub async fn get_engine_health(
    State(state): State<SharedState>,
) -> Json<HealthResponse> {
    let engine = state.audio_engine.lock().await;
    let health = engine.get_health_stats();
    let position = engine.get_position();
    let sample_rate = engine.get_device_sample_rate();
    let playing = engine.get_state() == audio::PlaybackState::Playing;

    let current_track = engine.get_current_track();
    let channels = current_track.as_ref().map(|t| t.channels as u32).unwrap_or(DEFAULT_CHANNELS as u32);

    let position_secs = if sample_rate > 0 && channels > 0 {
        position as f64 / (sample_rate as f64 * channels as f64)
    } else {
        0.0
    };

    let expected_interval_ms = if sample_rate > 0 && channels > 0 && health.buffer_size > 0 {
        (health.buffer_size as f64 / channels as f64) / sample_rate as f64 * 1000.0
    } else {
        0.0
    };

    Json(HealthResponse {
        playing,
        sample_rate,
        position_samples: position,
        position_secs,
        callback_count: health.callback_count,
        underrun_count: health.underrun_count,
        buffer_size: health.buffer_size,
        samples_delivered: health.samples_delivered,
        silence_frames: health.silence_frames,
        max_callback_interval_ms: health.max_callback_interval_us as f64 / 1000.0,
        late_callbacks: health.late_callbacks,
        expected_interval_ms,
    })
}
