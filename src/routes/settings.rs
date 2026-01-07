use axum::{extract::State, http::StatusCode, Json};
use std::net::UdpSocket;
use tracing::{error, info};

use crate::config;
use crate::types::{OscRequest, SharedState};

pub async fn send_osc(Json(payload): Json<OscRequest>) -> Result<StatusCode, StatusCode> {
    tokio::time::sleep(tokio::time::Duration::from_millis(25)).await;

    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| {
        eprintln!("Failed to create OSC socket: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let packet = rosc::encoder::encode(&rosc::OscPacket::Message(rosc::OscMessage {
        addr: payload.address.clone(),
        args: vec![],
    }))
    .map_err(|e| {
        eprintln!("Failed to encode OSC message '{}': {}", payload.address, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let target = format!("{}:{}", payload.ip, payload.port);
    socket.send_to(&packet, &target).map_err(|e| {
        eprintln!("Failed to send OSC message to {}: {}", target, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::OK)
}

pub fn send_osc_sync(ip: &str, port: u16, address: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    let packet = rosc::encoder::encode(&rosc::OscPacket::Message(rosc::OscMessage {
        addr: address.to_string(),
        args: vec![],
    }))?;
    let target = format!("{}:{}", ip, port);
    socket.send_to(&packet, &target)?;
    println!("📤 OSC sent: {} -> {}", address, target);
    Ok(())
}

pub async fn get_loopy_pro_settings(
    State(state): State<SharedState>,
) -> Result<Json<config::LoopyProConfig>, StatusCode> {
    let config = state.config.lock().await;
    Ok(Json(config.loopy_pro.clone()))
}

pub async fn update_loopy_pro_settings(
    State(state): State<SharedState>,
    Json(payload): Json<config::LoopyProConfig>,
) -> Result<StatusCode, StatusCode> {
    let mut config = state.config.lock().await;
    config.loopy_pro = payload;
    config.save().map_err(|e| {
        error!("Failed to save Loopy Pro settings: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    info!("Loopy Pro settings updated: {}:{}", config.loopy_pro.ip, config.loopy_pro.port);
    Ok(StatusCode::OK)
}
