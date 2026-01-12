use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::playback_history::PlaybackSession;
use crate::types::SharedState;

#[derive(Serialize)]
pub struct HistoryListResponse {
    pub sessions: Vec<PlaybackSession>,
    pub current: Option<PlaybackSession>,
}

#[derive(Serialize)]
pub struct SessionResponse {
    pub session: PlaybackSession,
}

#[derive(Serialize)]
pub struct DeleteResponse {
    pub deleted: bool,
}

#[derive(Serialize)]
pub struct ClearResponse {
    pub deleted_count: usize,
}

pub async fn get_history(
    State(state): State<SharedState>,
) -> Result<Json<HistoryListResponse>, StatusCode> {
    let sessions = state.playback_history.get_sessions();
    let current = state.playback_history.get_current_session();
    Ok(Json(HistoryListResponse { sessions, current }))
}

pub async fn get_session(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<SessionResponse>, StatusCode> {
    match state.playback_history.get_session(&id) {
        Some(session) => Ok(Json(SessionResponse { session })),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn delete_session(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, StatusCode> {
    match state.playback_history.delete_session(&id) {
        Ok(_) => Ok(Json(DeleteResponse { deleted: true })),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn clear_history(
    State(state): State<SharedState>,
) -> Result<Json<ClearResponse>, StatusCode> {
    match state.playback_history.clear_all() {
        Ok(count) => Ok(Json(ClearResponse { deleted_count: count })),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
