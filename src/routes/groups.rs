use axum::{extract::{Path, State}, http::StatusCode, Json};
use tracing::{error, info, warn};

use crate::board::GroupCommand;
use crate::config::{self, Config};
use crate::group;
use crate::transport;
use crate::types::{
    CreateGroupRequest, GroupBrightnessRequest, GroupColorRequest, GroupEffectRequest,
    GroupOperationResult, GroupPresetRequest, PowerRequest, SharedState,
    UpdateGroupRequest, WledPreset,
};

pub async fn reconfigure_group_universe(
    state: SharedState,
    group_id: String,
    member_ids: Vec<String>,
    old_universe: Option<u16>,
    new_universe: Option<u16>,
) {
    if old_universe == new_universe {
        return;
    }

    let config = Config::load().unwrap_or(Config {
        loopy_pro: config::LoopyProConfig::default(),
        boards: vec![],
        groups: vec![],
        effect_presets: vec![],
        pattern_presets: vec![],
    });
    let group_index = config.groups.iter().position(|g| g.id == group_id).unwrap_or(0);
    let universe = new_universe.unwrap_or((group_index + 1) as u16);

    info!(
        group_id = %group_id,
        old_universe = ?old_universe,
        new_universe = %universe,
        "Universe changed, reconfiguring member boards and E1.31 transport"
    );

    let mut board_ips = Vec::new();
    for member_id in &member_ids {
        if let Some(board) = config.boards.iter().find(|b| &b.id == member_id) {
            board_ips.push((member_id.clone(), board.ip.clone()));
        }
    }

    let mut tasks = Vec::new();
    for (board_id, board_ip) in &board_ips {
        let board_id = board_id.clone();
        let board_ip = board_ip.clone();
        let uni = universe;

        tasks.push(tokio::spawn(async move {
            info!(board_id = %board_id, universe = %uni, "Reconfiguring board universe");
            match configure_board_universe(&board_ip, uni).await {
                Ok(_) => {
                    info!(board_id = %board_id, universe = %uni, "Successfully reconfigured universe");
                }
                Err(e) => {
                    warn!(
                        board_id = %board_id,
                        universe = %uni,
                        "Failed to reconfigure universe: {}. Board may need manual configuration.",
                        e
                    );
                }
            }
        }));
    }

    for task in tasks {
        let _ = task.await;
    }

    let group_board_ips: Vec<String> = board_ips.iter().map(|(_, ip)| ip.clone()).collect();

    if !group_board_ips.is_empty() {
        let transport_opt = match transport::E131RawTransport::new(group_board_ips.clone(), universe) {
            Ok(t) => Some(t),
            Err(e) => {
                error!(
                    group_id = %group_id,
                    universe = %universe,
                    "Failed to create new E1.31 transport: {}",
                    e.to_string()
                );
                None
            }
        };

        if let Some(new_transport) = transport_opt {
            let mut transports = state.group_e131_transports.write().await;
            transports.insert(group_id.clone(), new_transport);
            info!(
                group_id = %group_id,
                universe = %universe,
                board_count = %group_board_ips.len(),
                "E1.31 transport updated for new universe"
            );
        }
    }
}

pub async fn configure_board_universe(
    board_ip: &str,
    universe: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let cfg_url = format!("http://{}/json/cfg", board_ip);
    let cfg_payload = serde_json::json!({
        "if": {
            "live": {
                "en": true,
                "mc": false,
                "dmx": {
                    "uni": universe,
                    "mode": 6,
                    "addr": 1
                },
                "timeout": 65535
            }
        }
    });

    let response = client
        .post(&cfg_url)
        .json(&cfg_payload)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("Config update failed: HTTP {}", response.status()).into());
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let reboot_url = format!("http://{}/json/state", board_ip);
    let reboot_payload = serde_json::json!({"rb": true});

    let reboot_response = client
        .post(&reboot_url)
        .json(&reboot_payload)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    if reboot_response.status().is_success() {
        Ok(())
    } else {
        Err(format!("Reboot failed: HTTP {}", reboot_response.status()).into())
    }
}

pub async fn create_group(
    State(state): State<SharedState>,
    Json(payload): Json<CreateGroupRequest>,
) -> Result<StatusCode, StatusCode> {
    info!(group_id = %payload.id, members = ?payload.members, "Creating group");

    {
        let senders = state.boards.read().await;

        for member_id in &payload.members {
            if !senders.contains_key(member_id) {
                warn!(group_id = %payload.id, member_id = %member_id, "Group creation failed: member not found");
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }

    let mut config = Config::load().unwrap_or(Config {
        loopy_pro: config::LoopyProConfig::default(),
        boards: vec![],
        groups: vec![],
        effect_presets: vec![],
        pattern_presets: vec![],
    });

    if config.groups.iter().any(|g| g.id == payload.id) {
        return Err(StatusCode::CONFLICT);
    }

    {
        let senders = state.boards.read().await;

        if senders.contains_key(&payload.id) {
            return Err(StatusCode::CONFLICT);
        }
    }

    config.groups.push(config::GroupConfig {
        id: payload.id.clone(),
        members: payload.members.clone(),
        universe: payload.universe,
    });

    if let Err(e) = config.save() {
        error!(group_id = %payload.id, "Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    info!(group_id = %payload.id, "Created group");
    Ok(StatusCode::CREATED)
}

pub async fn delete_group(
    State(_state): State<SharedState>,
    Path(group_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let mut config = Config::load().unwrap_or(Config {
        loopy_pro: config::LoopyProConfig::default(),
        boards: vec![],
        groups: vec![],
        effect_presets: vec![],
        pattern_presets: vec![],
    });

    if !config.groups.iter().any(|g| g.id == group_id) {
        return Err(StatusCode::NOT_FOUND);
    }

    config.groups.retain(|g| g.id != group_id);

    if let Err(e) = config.save() {
        error!(group_id = %group_id, "Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    info!(group_id = %group_id, "Deleted group");
    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_group(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
    Json(req): Json<UpdateGroupRequest>,
) -> Result<StatusCode, StatusCode> {
    {
        let senders = state.boards.read().await;

        for member_id in &req.members {
            if !senders.contains_key(member_id) {
                warn!(group_id = %group_id, member_id = %member_id, "Group update failed: member not found");
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }

    let mut config = Config::load().unwrap_or(Config {
        loopy_pro: config::LoopyProConfig::default(),
        boards: vec![],
        groups: vec![],
        effect_presets: vec![],
        pattern_presets: vec![],
    });

    if req.id != group_id {
        if config.groups.iter().any(|g| g.id == req.id) {
            return Err(StatusCode::CONFLICT);
        }

        {
            let senders = state.boards.read().await;

            if senders.contains_key(&req.id) {
                return Err(StatusCode::CONFLICT);
            }
        }
    }

    let group = config
        .groups
        .iter_mut()
        .find(|g| g.id == group_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let old_universe = group.universe;
    let old_members = group.members.clone();
    let new_id = req.id.clone();
    let new_members = req.members.clone();
    let new_universe = req.universe;

    group.id = req.id;
    group.members = req.members;
    group.universe = req.universe;

    if let Err(e) = config.save() {
        error!(group_id = %group_id, "Failed to save boards.toml: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let members_changed = old_members != new_members;
    let universe_changed = old_universe != new_universe;

    if members_changed || universe_changed {
        let current_universe = new_universe.or(old_universe);

        tokio::spawn(reconfigure_group_universe(
            state,
            new_id.clone(),
            new_members,
            None,
            current_universe,
        ));
    }

    info!(group_id = %new_id, "Updated group");
    Ok(StatusCode::OK)
}

pub async fn set_group_power(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
    Json(payload): Json<PowerRequest>,
) -> Result<Json<GroupOperationResult>, StatusCode> {
    let result = group::execute_group_command(
        state,
        &group_id,
        GroupCommand::SetPower(payload.on, payload.transition),
    )
    .await;

    match result {
        Ok(r) => Ok(Json(r)),
        Err(_e) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn set_group_brightness(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
    Json(payload): Json<GroupBrightnessRequest>,
) -> Result<Json<GroupOperationResult>, StatusCode> {
    let result = group::execute_group_command(
        state,
        &group_id,
        GroupCommand::SetBrightness(payload.brightness, payload.transition),
    )
    .await;

    match result {
        Ok(r) => Ok(Json(r)),
        Err(_e) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn set_group_color(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
    Json(payload): Json<GroupColorRequest>,
) -> Result<Json<GroupOperationResult>, StatusCode> {
    match group::execute_group_command(
        state,
        &group_id,
        GroupCommand::SetColor {
            r: payload.r,
            g: payload.g,
            b: payload.b,
            transition: payload.transition,
        },
    )
    .await
    {
        Ok(result) => Ok(Json(result)),
        Err(_e) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn set_group_effect(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
    Json(payload): Json<GroupEffectRequest>,
) -> Result<Json<GroupOperationResult>, StatusCode> {
    match group::execute_group_command(
        state,
        &group_id,
        GroupCommand::SetEffect(payload.effect, payload.transition),
    )
    .await
    {
        Ok(result) => Ok(Json(result)),
        Err(_e) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn set_group_preset(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
    Json(payload): Json<GroupPresetRequest>,
) -> Result<Json<GroupOperationResult>, StatusCode> {
    let received_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    println!(
        "🎨 [{}ms] HTTP received: group='{}' preset={} bpm={:?} transition={}",
        received_at, group_id, payload.preset, payload.bpm, payload.transition
    );

    let preset_slot = if let Some(ref name) = payload.preset_name {
        let presets = WledPreset::load_all(&state.storage_paths.presets)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some(preset) = presets.iter().find(|p| &p.name == name) {
            preset.wled_slot
        } else {
            warn!("Preset '{}' not found in presets.json", name);
            return Err(StatusCode::NOT_FOUND);
        }
    } else {
        payload.preset
    };

    let command = GroupCommand::SetPreset(preset_slot, payload.transition);

    match group::execute_group_command(state, &group_id, command).await {
        Ok(result) => {
            let sent_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            println!(
                "✅ [{}ms] Group command executed: group='{}' preset={}",
                sent_at, group_id, payload.preset
            );
            Ok(Json(result))
        }
        Err(_e) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn sync_presets_to_group(
    State(state): State<SharedState>,
    Path(group_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let group_members: Vec<String> = {
        let config = state.config.lock().await;
        config
            .groups
            .iter()
            .find(|g| g.id == group_id)
            .map(|g| g.members.clone())
            .ok_or((
                StatusCode::NOT_FOUND,
                format!("Group '{}' not found", group_id),
            ))?
    };

    if group_members.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Group '{}' has no members", group_id),
        ));
    }

    let presets = WledPreset::load_all(&state.storage_paths.presets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load presets: {}", e),
        )
    })?;

    let mut all_results = Vec::new();
    let mut total_success = 0;

    let mut tasks = Vec::new();
    let member_ids = group_members.clone();

    for board_id in group_members {
        let state_clone = state.clone();
        let presets_clone = presets.clone();
        let board_id_clone = board_id.clone();

        let task = tokio::spawn(async move {
            (board_id_clone.clone(), replace_presets_on_board_internal(state_clone, &board_id_clone, presets_clone).await)
        });
        tasks.push(task);
    }

    let results = futures::future::join_all(tasks).await;

    for result in results.into_iter() {
        match result {
            Ok((board_id, Ok(_))) => {
                total_success += 1;
                all_results.push(serde_json::json!({
                    "board_id": board_id,
                    "status": "success"
                }));
            }
            Ok((board_id, Err(e))) => {
                all_results.push(serde_json::json!({
                    "board_id": board_id,
                    "status": "failed",
                    "error": e.1
                }));
            }
            Err(e) => {
                all_results.push(serde_json::json!({
                    "status": "failed",
                    "error": format!("Task failed: {}", e)
                }));
            }
        }
    }

    Ok(Json(serde_json::json!({
        "group_id": group_id,
        "total_presets": presets.len(),
        "successful_boards": total_success,
        "total_boards": member_ids.len(),
        "member_results": all_results
    })))
}

pub async fn replace_presets_on_board_internal(
    state: SharedState,
    board_id: &str,
    presets: Vec<WledPreset>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let board_ip = {
        let senders_lock = state.boards.read().await;
        senders_lock
            .get(board_id)
            .map(|entry| entry.ip.clone())
            .ok_or((StatusCode::NOT_FOUND, format!("Board '{}' not found", board_id)))?
    };

    let wled_presets_json = WledPreset::build_wled_presets_file(&presets);
    let json_string = serde_json::to_string(&wled_presets_json).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serialize presets: {}", e))
    })?;

    let client = reqwest::Client::new();
    let part = reqwest::multipart::Part::bytes(json_string.into_bytes())
        .file_name("presets.json")
        .mime_str("application/json")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create multipart: {}", e)))?;

    let form = reqwest::multipart::Form::new().part("file", part);
    let url = format!("http://{}/upload", board_ip);

    let response = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed to upload to {}: {}", board_id, e)))?;

    if response.status().is_success() {
        Ok(Json(serde_json::json!({
            "board_id": board_id,
            "status": "success",
            "preset_count": presets.len()
        })))
    } else {
        let status = response.status();
        Err((StatusCode::BAD_GATEWAY, format!("Board {} returned {}", board_id, status)))
    }
}
