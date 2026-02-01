use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use tracing::info;

use crate::audio;
use crate::types::SharedState;

#[derive(Deserialize)]
pub struct SelectDeviceRequest {
    pub device_id: Option<String>,
}

pub async fn select_device(
    State(state): State<SharedState>,
    Json(payload): Json<SelectDeviceRequest>,
) -> StatusCode {
    let Some(device_id) = payload.device_id else {
        state.device_manager.select_device(None);
        let mut config = state.config.lock().await;
        config.set_preferred_audio_device(None);
        if let Err(e) = config.save() {
            tracing::error!("Failed to save config: {}", e);
        }
        return StatusCode::OK;
    };

    let Some(sample_rate) = state.device_manager.get_device_sample_rate(&device_id) else {
        return StatusCode::NOT_FOUND;
    };
    let Some(output_channels) = state.device_manager.get_device_output_channels(&device_id) else {
        return StatusCode::NOT_FOUND;
    };
    let output_channels = output_channels as usize;

    state.device_manager.select_device(Some(device_id.clone()));

    let routing_snapshot = {
        let mut config = state.config.lock().await;
        config.set_preferred_audio_device(Some(device_id.clone()));
        if let Err(e) = config.save() {
            tracing::error!("Failed to save config: {}", e);
        }

        let routing_config = config
            .audio
            .get_routing_for_device(&device_id)
            .cloned()
            .unwrap_or_else(|| crate::config::DeviceRouting::new_default(device_id.clone()));
        audio::RoutingConfig::from_device_routing(&routing_config, output_channels)
    };

    let tracks_to_resample = {
        let mut engine = state.audio_engine.lock().await;
        engine.set_device_and_routing(device_id.clone(), sample_rate, routing_snapshot).await;
        engine.get_all_tracks()
    };

    if tracks_to_resample.is_empty() {
        return StatusCode::OK;
    }

    tokio::spawn(async move {
        for track in tracks_to_resample {
            if let Err(e) = track.ensure_resampled(sample_rate) {
                eprintln!("[Resampling] Error: {}", e);
            }
        }
    });

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

#[derive(serde::Serialize)]
pub struct DeviceOutputsResponse {
    pub device_id: String,
    pub output_channels: u16,
}

pub async fn get_device_outputs(
    State(state): State<SharedState>,
    Path(device_id): Path<String>,
) -> Result<Json<DeviceOutputsResponse>, StatusCode> {
    let channels = state
        .device_manager
        .get_device_output_channels(&device_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(DeviceOutputsResponse {
        device_id,
        output_channels: channels,
    }))
}

#[derive(serde::Serialize)]
pub struct ResamplingQualityResponse {
    pub quality: crate::config::ResamplingQuality,
}

pub async fn get_resampling_quality(
    State(state): State<SharedState>,
) -> Json<ResamplingQualityResponse> {
    let engine = state.audio_engine.lock().await;
    Json(ResamplingQualityResponse {
        quality: engine.get_resampling_quality(),
    })
}

#[derive(Deserialize)]
pub struct SetResamplingQualityRequest {
    pub quality: crate::config::ResamplingQuality,
}

pub async fn set_resampling_quality(
    State(state): State<SharedState>,
    Json(payload): Json<SetResamplingQualityRequest>,
) -> StatusCode {
    {
        let mut engine = state.audio_engine.lock().await;
        engine.set_resampling_quality(payload.quality);
    }

    {
        let mut config = state.config.lock().await;
        config.audio.resampling_quality = payload.quality;
        if let Err(e) = config.save() {
            tracing::error!("Failed to save config: {}", e);
        }
    }

    StatusCode::OK
}
