use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::timing_metrics::{DriftEvent, MetricsSnapshot};
use crate::types::SharedState;

#[derive(Serialize)]
pub struct TimingSnapshotResponse {
    #[serde(flatten)]
    pub snapshot: MetricsSnapshot,
}

#[derive(Serialize)]
pub struct TimingEventsResponse {
    pub events: Vec<DriftEvent>,
}

#[derive(Deserialize)]
pub struct UpdateThresholdRequest {
    pub drift_threshold_ms: f64,
}

#[derive(Serialize)]
pub struct ThresholdResponse {
    pub drift_threshold_ms: f64,
}

pub async fn get_timing_snapshot(
    State(state): State<SharedState>,
) -> Result<Json<TimingSnapshotResponse>, StatusCode> {
    let snapshot = state.timing_metrics.snapshot();
    Ok(Json(TimingSnapshotResponse { snapshot }))
}

pub async fn get_timing_events(
    State(state): State<SharedState>,
) -> Result<Json<TimingEventsResponse>, StatusCode> {
    let events = state.timing_metrics.get_recent_events();
    Ok(Json(TimingEventsResponse { events }))
}

pub async fn clear_timing_events(
    State(state): State<SharedState>,
) -> Result<StatusCode, StatusCode> {
    state.timing_metrics.clear_events();
    Ok(StatusCode::OK)
}

pub async fn reset_timing_metrics(
    State(state): State<SharedState>,
) -> Result<StatusCode, StatusCode> {
    state.timing_metrics.reset();
    Ok(StatusCode::OK)
}

pub async fn get_timing_threshold(
    State(state): State<SharedState>,
) -> Result<Json<ThresholdResponse>, StatusCode> {
    let threshold = state.timing_metrics.get_drift_threshold_ms();
    Ok(Json(ThresholdResponse { drift_threshold_ms: threshold }))
}

pub async fn update_timing_threshold(
    State(state): State<SharedState>,
    Json(req): Json<UpdateThresholdRequest>,
) -> Result<Json<ThresholdResponse>, StatusCode> {
    state.timing_metrics.set_drift_threshold(req.drift_threshold_ms);
    Ok(Json(ThresholdResponse { drift_threshold_ms: req.drift_threshold_ms }))
}
