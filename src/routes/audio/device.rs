use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::audio::{self, spawn_resampling, ResamplingJob};
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

    let (tracks_to_resample, quality, cancellation_token) = {
        let mut engine = state.audio_engine.lock().await;
        engine.set_device_and_routing(device_id.clone(), sample_rate, routing_snapshot).await;

        let all_tracks = engine.get_all_tracks_with_ids();
        let mut to_resample = Vec::new();
        let mut clicks_to_remove = Vec::new();

        for (slot, id, track) in all_tracks {
            if id.ends_with("_click") {
                clicks_to_remove.push(id);
            } else {
                to_resample.push((slot, id, track));
            }
        }

        for click_id in &clicks_to_remove {
            engine.unload_slot_track(audio::SlotId::Click, click_id);
        }
        if !clicks_to_remove.is_empty() {
            eprintln!("[Device] Removed {} click tracks (will regenerate at new rate)", clicks_to_remove.len());
        }

        let token = engine.create_device_change_cancellation();
        (to_resample, engine.get_resampling_quality(), token)
    };

    if !tracks_to_resample.is_empty() {
        eprintln!("[Device] Starting resampling for {} tracks to {}Hz", tracks_to_resample.len(), sample_rate);

        let jobs: Vec<ResamplingJob> = tracks_to_resample
            .into_iter()
            .map(|(slot, id, track)| ResamplingJob {
                slot,
                id,
                track,
                cancellation: Some(Arc::clone(&cancellation_token)),
            })
            .collect();

        spawn_resampling(
            jobs,
            sample_rate,
            quality,
            Some(state.broadcast_tx.clone()),
        );
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
