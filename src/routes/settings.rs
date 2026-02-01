use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use std::net::UdpSocket;
use tracing::{error, info, warn};

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

#[derive(Serialize)]
pub struct StorageStatus {
    pub usb_mounted: bool,
    pub programs_path: String,
    pub programs_available: bool,
    pub programs_count: usize,
    pub audio_path: String,
    pub audio_available: bool,
    pub audio_count: usize,
}

pub async fn get_storage_status(
    State(state): State<SharedState>,
) -> Json<StorageStatus> {
    let storage = &state.storage_paths;

    let usb_base = std::path::Path::new("/tmp/mountd/disk1_part1");
    let usb_mounted = usb_base.exists() && usb_base.is_dir();

    let programs_available = storage.programs.exists() && storage.programs.is_dir();
    let programs_count = if programs_available {
        std::fs::read_dir(&storage.programs)
            .map(|entries| entries.filter_map(|e| e.ok()).filter(|e| {
                e.path().extension().map(|ext| ext == "json").unwrap_or(false)
            }).count())
            .unwrap_or(0)
    } else {
        0
    };

    let audio_available = storage.audio.exists() && storage.audio.is_dir();
    let audio_count = if audio_available {
        std::fs::read_dir(&storage.audio)
            .map(|entries| entries.filter_map(|e| e.ok()).filter(|e| {
                e.path().extension().map(|ext| ext == "mp3").unwrap_or(false)
            }).count())
            .unwrap_or(0)
    } else {
        0
    };

    Json(StorageStatus {
        usb_mounted,
        programs_path: storage.programs.display().to_string(),
        programs_available,
        programs_count,
        audio_path: storage.audio.display().to_string(),
        audio_available,
        audio_count,
    })
}

pub async fn restart_server() -> Result<StatusCode, (StatusCode, String)> {
    info!("🔄 Server restart requested via API");

    tokio::spawn(async {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        info!("🔄 Executing server restart...");

        let result = std::process::Command::new("/etc/init.d/wled-server")
            .arg("restart")
            .spawn();

        match result {
            Ok(_) => info!("🔄 Server restart command executed"),
            Err(e) => {
                warn!("Failed to restart via init.d, trying alternative: {}", e);
                let _ = std::process::Command::new("sh")
                    .arg("-c")
                    .arg("sleep 1 && /etc/init.d/wled-server restart &")
                    .spawn();
            }
        }
    });

    Ok(StatusCode::OK)
}

pub async fn get_timecode_settings(
    State(state): State<SharedState>,
) -> Result<Json<config::TimecodeConfig>, StatusCode> {
    let config = state.config.lock().await;
    Ok(Json(config.timecode.clone()))
}

pub async fn update_timecode_settings(
    State(state): State<SharedState>,
    Json(payload): Json<config::TimecodeConfig>,
) -> Result<StatusCode, StatusCode> {
    let mut config = state.config.lock().await;
    config.timecode = payload;
    config.save().map_err(|e| {
        error!("Failed to save timecode settings: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    info!(
        "Timecode settings updated: OSC {}:{} ({})",
        config.timecode.osc.ip,
        config.timecode.osc.port,
        if config.timecode.osc.enabled { "enabled" } else { "disabled" }
    );
    Ok(StatusCode::OK)
}
