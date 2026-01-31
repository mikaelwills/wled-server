use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use super::DEFAULT_CHANNELS;
use crate::audio;
use crate::types::SharedState;

#[derive(Serialize)]
pub struct RoutingResponse {
    pub device_id: String,
    pub backing_left: u16,
    pub backing_right: u16,
    pub guide: u16,
    pub click: u16,
}

pub async fn get_routing(
    State(state): State<SharedState>,
    Path(device_id): Path<String>,
) -> Json<RoutingResponse> {
    let config = state.config.lock().await;

    let routing = config.audio.get_routing_for_device(&device_id)
        .cloned()
        .unwrap_or_else(|| crate::config::DeviceRouting::new_default(device_id.clone()));

    Json(RoutingResponse {
        device_id: routing.device_id,
        backing_left: routing.backing_left,
        backing_right: routing.backing_right,
        guide: routing.guide,
        click: routing.click,
    })
}

#[derive(Deserialize)]
pub struct UpdateRoutingRequest {
    pub backing_left: u16,
    pub backing_right: u16,
    pub guide: u16,
    pub click: u16,
}

pub async fn update_routing(
    State(state): State<SharedState>,
    Path(device_id): Path<String>,
    Json(payload): Json<UpdateRoutingRequest>,
) -> StatusCode {
    let routing = crate::config::DeviceRouting {
        device_id: device_id.clone(),
        backing_left: payload.backing_left,
        backing_right: payload.backing_right,
        guide: payload.guide,
        click: payload.click,
    };

    {
        let mut config = state.config.lock().await;
        config.audio.set_routing_for_device(routing.clone());
        if let Err(e) = config.save() {
            tracing::error!("Failed to save config: {}", e);
        }
    }

    let selected_device = state.device_manager.get_selected_device();
    if selected_device.as_ref() == Some(&device_id) {
        let output_channels = state.device_manager
            .get_device_output_channels(&device_id)
            .unwrap_or(DEFAULT_CHANNELS) as usize;

        let routing_config = audio::RoutingConfig::from_device_routing(&routing, output_channels);

        let engine = state.audio_engine.lock().await;
        engine.update_routing(routing_config).await;
    }

    info!("Updated routing for device: {}", device_id);
    StatusCode::OK
}

#[derive(Deserialize)]
pub struct SetMuteRequest {
    pub track: String,
    pub muted: bool,
}

pub async fn set_mute(
    State(state): State<SharedState>,
    Json(payload): Json<SetMuteRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let slot = match payload.track.as_str() {
        "backing" => audio::SlotId::Backing,
        "guide" => audio::SlotId::Guide,
        "click" => audio::SlotId::Click,
        "aux" => audio::SlotId::Aux,
        _ => return Err((StatusCode::BAD_REQUEST, format!("Invalid track: {}", payload.track))),
    };

    let engine = state.audio_engine.lock().await;
    engine.set_mute(slot, payload.muted).await;

    info!("Set {} muted: {}", payload.track, payload.muted);
    Ok(StatusCode::OK)
}
