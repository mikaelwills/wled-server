use axum::{extract::{Path, State}, http::StatusCode, Json};
use tracing::{error, info};

use crate::audio;
use crate::types::{SharedState, UploadAudioRequest, UploadAudioResponse};

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
