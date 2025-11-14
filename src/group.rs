use crate::board::{BoardCommand, GroupCommand};
use crate::config::Config;
use crate::types::{GroupOperationResult, SharedState};
use std::sync::atomic::{AtomicU8, Ordering};
use tracing::{info, error, warn};

// Cache last preset sent via E1.31 (default: 1)
static CACHED_PRESET: AtomicU8 = AtomicU8::new(1);

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

    // OPTIMIZATION: Use E1.31 for preset/power/brightness commands
    // Each group has its own E1.31 transport with unique universe
    let use_e131 = matches!(command, GroupCommand::SetPreset(_, _) | GroupCommand::SetPower(_, _) | GroupCommand::SetBrightness(_, _));

    if use_e131 {
        // Try E1.31 send in separate scope to ensure guard is dropped
        {
            let mut transports_lock = state.group_e131_transports.write().await;
            if let Some(e131) = transports_lock.get_mut(group_id) {
                // Log universe and target board information
                let universe = e131.universe();
                let target_ips: Vec<String> = e131.board_ips().iter().map(|addr| addr.to_string()).collect();
                info!(
                    group_id = %group_id,
                    universe = universe,
                    targets = ?target_ips,
                    member_count = target_ips.len(),
                    "E1.31 group command - sending to universe {} with {} member board(s)",
                    universe,
                    target_ips.len()
                );

                // Send E1.31 packet to this group's universe
                let result = match &command {
                    GroupCommand::SetPreset(preset, _transition) => {
                        // Cache the preset for future power/brightness commands
                        CACHED_PRESET.store(*preset, Ordering::Relaxed);
                        info!(group_id = %group_id, preset = preset, universe = universe, "Sending preset command");
                        e131.send_preset(*preset, 255)  // Full brightness
                    }
                    GroupCommand::SetPower(on, _transition) => {
                        let preset = CACHED_PRESET.load(Ordering::Relaxed);
                        info!(group_id = %group_id, on = on, preset = if *on { preset } else { 0 }, universe = universe, "Sending power command");
                        e131.send_power(*on, preset)  // Use cached preset or blackout (preset 0)
                    }
                    GroupCommand::SetBrightness(brightness, _transition) => {
                        let preset = CACHED_PRESET.load(Ordering::Relaxed);
                        info!(group_id = %group_id, brightness = brightness, preset = preset, universe = universe, "Sending brightness command");
                        e131.send_brightness(*brightness, preset)  // Use cached preset
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
