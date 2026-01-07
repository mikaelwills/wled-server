use crate::board::{BoardCommand, GroupCommand};
use crate::config::Config;
use crate::types::{GroupOperationResult, SharedState, WledPreset};
use std::sync::atomic::{AtomicU8, Ordering};
use tracing::{info, error, warn};

static CACHED_COLOR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x00FFFFFF);
static CACHED_BRIGHTNESS: AtomicU8 = AtomicU8::new(255);

pub async fn execute_group_command(
    state: SharedState,
    group_id: &str,
    command: GroupCommand,
) -> Result<GroupOperationResult, Box<dyn std::error::Error + Send + Sync>> {
    // Load group configuration to get member board IDs
    let config = Config::load()
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())) })?;
    let group = config.groups.iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> { Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, format!("Group '{}' not found", group_id))) })?;

    // OPTIMIZATION: Use E1.31 only for presets (performance mode)
    // Power/brightness should use WebSocket for home use mode
    let use_e131 = matches!(command, GroupCommand::SetPreset(_, _));

    if use_e131 {
        // Try E1.31 send in separate scope to ensure guard is dropped
        {
            let mut transports_lock = state.group_e131_transports.write().await;
            if let Some(e131) = transports_lock.get_mut(group_id) {
                let universe = e131.universe();
                let broadcast = e131.broadcast_addr();
                info!(
                    group_id = %group_id,
                    universe = universe,
                    broadcast = %broadcast,
                    "E1.31 broadcast group command - universe {} → {}",
                    universe, broadcast
                );

                // Send E1.31 packet via Mode 6 (direct LED control)
                let result = match &command {
                    GroupCommand::SetPreset(preset_slot, _transition) => {
                        let presets = match WledPreset::load_all(&state.storage_paths.presets) {
                            Ok(p) => p,
                            Err(e) => {
                                warn!(group_id = %group_id, "Failed to load presets: {}", e);
                                return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                            }
                        };

                        if let Some(preset) = presets.iter().find(|p| p.wled_slot == *preset_slot) {
                            let [r, g, b] = preset.state.color;
                            let brightness = preset.state.brightness;
                            let color_packed = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                            CACHED_COLOR.store(color_packed, Ordering::Relaxed);
                            CACHED_BRIGHTNESS.store(brightness, Ordering::Relaxed);
                            info!(
                                group_id = %group_id,
                                preset_slot = preset_slot,
                                preset_name = %preset.name,
                                r = r, g = g, b = b,
                                brightness = brightness,
                                universe = universe,
                                "Sending preset color via Mode 6"
                            );
                            e131.send_solid_color(r, g, b, brightness)
                        } else {
                            warn!(group_id = %group_id, preset_slot = preset_slot, "Preset slot not found");
                            Err(Box::new(std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!("Preset slot {} not found", preset_slot)
                            )) as Box<dyn std::error::Error>)
                        }
                    }
                    GroupCommand::SetPower(on, _transition) => {
                        info!(group_id = %group_id, on = on, universe = universe, "Sending power via Mode 6");
                        if *on {
                            let color = CACHED_COLOR.load(Ordering::Relaxed);
                            let r = ((color >> 16) & 0xFF) as u8;
                            let g = ((color >> 8) & 0xFF) as u8;
                            let b = (color & 0xFF) as u8;
                            let brightness = CACHED_BRIGHTNESS.load(Ordering::Relaxed);
                            e131.send_solid_color(r, g, b, brightness)
                        } else {
                            e131.send_blackout()
                        }
                    }
                    GroupCommand::SetBrightness(brightness, _transition) => {
                        CACHED_BRIGHTNESS.store(*brightness, Ordering::Relaxed);
                        let color = CACHED_COLOR.load(Ordering::Relaxed);
                        let r = ((color >> 16) & 0xFF) as u8;
                        let g = ((color >> 8) & 0xFF) as u8;
                        let b = (color & 0xFF) as u8;
                        info!(group_id = %group_id, brightness = brightness, universe = universe, "Sending brightness via Mode 6");
                        e131.send_solid_color(r, g, b, *brightness)
                    }
                    _ => unreachable!(),
                };

                if let Err(e) = result {
                    warn!(group_id = %group_id, "Group command failed: {}", e);
                    return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())) as Box<dyn std::error::Error + Send + Sync>);
                }
            } else {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "E1.31 transport not available"
                )));
            }
        }; // guard dropped here

        // E1.31 succeeded - now synchronize actor state for all member boards
        // This prevents WebSocket reconnection logic from restoring old cached state
        info!(group_id = %group_id, "Synchronizing actor state after E1.31 command");

        let boards_lock = state.boards.read().await;
        for board_id in &group.members {
            if let Some(board_entry) = boards_lock.get(board_id) {
                // Convert GroupCommand to state sync commands (no WebSocket send)
                match &command {
                    GroupCommand::SetPreset(preset, _transition) => {
                        // Preset command affects multiple state fields
                        if let Err(e) = board_entry.sender.send(BoardCommand::SyncPresetState(*preset)).await {
                            warn!(board_id = %board_id, "Failed to sync preset state: {}", e);
                        }
                    }
                    GroupCommand::SetPower(on, _transition) => {
                        if let Err(e) = board_entry.sender.send(BoardCommand::SyncPowerState(*on)).await {
                            warn!(board_id = %board_id, "Failed to sync power state: {}", e);
                        }
                    }
                    GroupCommand::SetBrightness(brightness, _transition) => {
                        if let Err(e) = board_entry.sender.send(BoardCommand::SyncBrightnessState(*brightness)).await {
                            warn!(board_id = %board_id, "Failed to sync brightness state: {}", e);
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
        drop(boards_lock);

        // E1.31 succeeded - boards already updated
        return Ok(GroupOperationResult {
            group_id: group_id.to_string(),
            successful_members: group.members.clone(),
            failed_members: vec![],
            member_states: vec![],  // Don't query state, boards will update via E1.31
        });
    }

    // If we get here, E1.31 is not being used for this command type
    // Fall back to WebSocket for color/effect commands
    info!(group_id = %group_id, "Using WebSocket fallback for group command (parallel execution)");

    // Get board actors
    let boards_lock = state.boards.read().await;

    // Spawn parallel tasks for each member board
    let mut tasks = Vec::new();
    for board_id in &group.members {
        if let Some(board_entry) = boards_lock.get(board_id) {
            let sender = board_entry.sender.clone();
            let board_id_clone = board_id.clone();

            // Convert GroupCommand to BoardCommand for this board
            let board_command = match &command {
                GroupCommand::SetPower(on, transition) => {
                    BoardCommand::SetPower(*on, *transition)
                }
                GroupCommand::SetBrightness(brightness, transition) => {
                    BoardCommand::SetBrightness(*brightness, *transition)
                }
                GroupCommand::SetColor { r, g, b, transition } => {
                    BoardCommand::SetColor { r: *r, g: *g, b: *b, transition: *transition }
                }
                GroupCommand::SetEffect(effect, transition) => {
                    BoardCommand::SetEffect(*effect, *transition)
                }
                _ => {
                    error!(board_id = %board_id, "Unsupported group command type");
                    continue;
                }
            };

            // Spawn task for parallel execution
            let task = tokio::spawn(async move {
                match sender.send(board_command).await {
                    Ok(_) => Ok(board_id_clone.clone()),
                    Err(e) => Err((board_id_clone, e.to_string())),
                }
            });
            tasks.push(task);
        } else {
            error!(board_id = %board_id, "Board not found in state");
        }
    }

    drop(boards_lock); // Release lock before awaiting

    // Wait for all tasks to complete in parallel
    let results = futures::future::join_all(tasks).await;

    // Collect successful and failed members
    let mut successful_members = Vec::new();
    let mut failed_members = Vec::new();

    for result in results {
        match result {
            Ok(Ok(board_id)) => {
                successful_members.push(board_id);
            }
            Ok(Err((board_id, error))) => {
                failed_members.push((board_id, error));
            }
            Err(e) => {
                error!("Task join error: {}", e);
            }
        }
    }

    info!(
        group_id = %group_id,
        successful = successful_members.len(),
        failed = failed_members.len(),
        "WebSocket fallback completed"
    );

    Ok(GroupOperationResult {
        group_id: group_id.to_string(),
        successful_members,
        failed_members,
        member_states: vec![],  // Don't query state for WebSocket fallback
    })
}

// execute_board_command removed - no longer using WebSocket fallback for group commands
