use crate::board::{BoardCommand, BoardState, GroupCommand};
use crate::config::Config;
use crate::types::{GroupOperationResult, SharedState};
use futures::future::join_all;
use std::time::Duration;
use tracing::{info, warn};

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

    // OPTIMIZATION: Use shared E1.31 for preset/power/brightness commands
    // All group boards assumed to be on universe 1
    let use_e131 = matches!(command, GroupCommand::SetPreset(_, _) | GroupCommand::SetPower(_, _) | GroupCommand::SetBrightness(_, _));

    if use_e131 {
        // Get group state values BEFORE acquiring lock to avoid holding lock across await
        let group_brightness = get_group_brightness(&state, &group.members).await.unwrap_or(255);
        let group_preset = get_group_preset(&state, &group.members).await.unwrap_or(0);

        // Try E1.31 send in separate scope to ensure guard is dropped
        let e131_success = {
            let mut group_e131_lock = state.group_e131.write().await;
            if let Some(ref mut e131) = *group_e131_lock {
                // Send single E1.31 packet to universe 1 (all boards receive)
                let result = match &command {
                    GroupCommand::SetPreset(preset, _transition) => {
                        info!(group_id = %group_id, preset = preset, "Sending group preset via E1.31 (1 packet to universe 1)");
                        e131.send_preset(*preset, group_brightness)
                    }
                    GroupCommand::SetPower(on, _transition) => {
                        let brightness = if *on {
                            group_brightness
                        } else {
                            0
                        };
                        info!(group_id = %group_id, on = on, "Sending group power via E1.31 (1 packet to universe 1)");
                        e131.send_power(*on, brightness)
                    }
                    GroupCommand::SetBrightness(brightness, _transition) => {
                        info!(group_id = %group_id, brightness = brightness, "Sending group brightness via E1.31 (1 packet to universe 1)");
                        e131.send_brightness(*brightness, group_preset)
                    }
                    _ => unreachable!(),
                };

                if let Err(e) = result {
                    warn!(group_id = %group_id, error = %e, "E1.31 send failed, falling back to WebSocket");
                    false
                } else {
                    true
                }
            } else {
                false
            }
        }; // guard dropped here

        if e131_success {
            // E1.31 succeeded - update states and return
            return execute_group_command_optimistic(&state, group_id, &group.members, &command).await;
        }
    }

    // Fallback: Execute commands for all member boards IN PARALLEL (WebSocket)
    let futures: Vec<_> = group.members.iter()
        .map(|board_id| {
            let board_id = board_id.clone();
            let command = command.clone();
            let state = state.clone();
            async move {
                (board_id.clone(), execute_board_command(state, &board_id, &command).await)
            }
        })
        .collect();

    let results = join_all(futures).await;

    let mut successful_members = Vec::new();
    let mut failed_members = Vec::new();
    let mut member_states = Vec::new();

    for (board_id, result) in results {
        match result {
            Ok(state) => {
                successful_members.push(board_id);
                member_states.push(state);
            }
            Err(e) => {
                failed_members.push((board_id, e.to_string()));
            }
        }
    }

    Ok(GroupOperationResult {
        group_id: group_id.to_string(),
        successful_members,
        failed_members,
        member_states,
    })
}

// Helper: Optimistic state update for E1.31 commands
async fn execute_group_command_optimistic(
    state: &SharedState,
    group_id: &str,
    members: &[String],
    command: &GroupCommand,
) -> Result<GroupOperationResult, Box<dyn std::error::Error + Send + Sync>> {
    let mut successful_members = Vec::new();
    let mut member_states = Vec::new();

    for board_id in members {
        // Send command to actor for state update (they'll use E1.31 individually)
        if let Some(sender) = state.boards.read().await.get(board_id).map(|e| e.sender.clone()) {
            let board_command = match command {
                GroupCommand::SetPreset(preset, transition) => BoardCommand::SetPreset(*preset, *transition),
                GroupCommand::SetPower(on, transition) => BoardCommand::SetPower(*on, *transition),
                GroupCommand::SetBrightness(bri, transition) => BoardCommand::SetBrightness(*bri, *transition),
                _ => continue,
            };

            // Send command (actor will handle state update)
            let _ = sender.send(board_command).await;

            // Get state back
            let (tx, rx) = tokio::sync::oneshot::channel();
            if sender.send(BoardCommand::GetState(tx)).await.is_ok() {
                if let Ok(Ok(board_state)) = tokio::time::timeout(Duration::from_millis(20), rx).await {
                    successful_members.push(board_id.clone());
                    member_states.push(board_state);
                }
            }
        }
    }

    Ok(GroupOperationResult {
        group_id: group_id.to_string(),
        successful_members,
        failed_members: vec![],
        member_states,
    })
}

// Helper: Get brightness from first available board
async fn get_group_brightness(state: &SharedState, members: &[String]) -> Option<u8> {
    for board_id in members {
        if let Some(sender) = state.boards.read().await.get(board_id).map(|e| e.sender.clone()) {
            let (tx, rx) = tokio::sync::oneshot::channel();
            if sender.send(BoardCommand::GetState(tx)).await.is_ok() {
                if let Ok(Ok(board_state)) = tokio::time::timeout(Duration::from_millis(20), rx).await {
                    return Some(board_state.brightness);
                }
            }
        }
    }
    None
}

// Helper: Get preset from first available board
async fn get_group_preset(state: &SharedState, members: &[String]) -> Option<u8> {
    for board_id in members {
        if let Some(sender) = state.boards.read().await.get(board_id).map(|e| e.sender.clone()) {
            let (tx, rx) = tokio::sync::oneshot::channel();
            if sender.send(BoardCommand::GetState(tx)).await.is_ok() {
                if let Ok(Ok(board_state)) = tokio::time::timeout(Duration::from_millis(20), rx).await {
                    return board_state.preset;
                }
            }
        }
    }
    None
}

async fn execute_board_command(
    state: SharedState,
    board_id: &str,
    command: &GroupCommand,
) -> Result<BoardState, Box<dyn std::error::Error + Send + Sync>> {
    // Get the board's sender from existing BoardActor infrastructure
    let sender = {
        let senders_lock = state.boards.read().await;

        senders_lock.get(board_id)
            .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
                Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, format!("Board '{}' not found or not connected", board_id)))
            })?
            .sender.clone()
    };

    // Send the appropriate command to the BoardActor
    match command {
        GroupCommand::SetPower(target_state, transition) => {
            sender.send(BoardCommand::SetPower(*target_state, *transition)).await
                .map_err(|_| -> Box<dyn std::error::Error + Send + Sync> {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Failed to send power command"))
                })?;
        }
        GroupCommand::SetBrightness(brightness, transition) => {
            sender.send(BoardCommand::SetBrightness(*brightness, *transition)).await
                .map_err(|_| -> Box<dyn std::error::Error + Send + Sync> {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Failed to send brightness command"))
                })?;
        }
        GroupCommand::SetColor { r, g, b, transition } => {
            sender.send(BoardCommand::SetColor { r: *r, g: *g, b: *b, transition: *transition }).await
                .map_err(|_| -> Box<dyn std::error::Error + Send + Sync> {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Failed to send color command"))
                })?;
        }
        GroupCommand::SetEffect(effect, transition) => {
            sender.send(BoardCommand::SetEffect(*effect, *transition)).await
                .map_err(|_| -> Box<dyn std::error::Error + Send + Sync> {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Failed to send effect command"))
                })?;
        }
        GroupCommand::SetPreset(preset, transition) => {
            sender.send(BoardCommand::SetPreset(*preset, *transition)).await
                .map_err(|_| -> Box<dyn std::error::Error + Send + Sync> {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Failed to send preset command"))
                })?;
        }
    }

    // Get updated state from the BoardActor
    let (tx, rx) = tokio::sync::oneshot::channel();
    sender.send(BoardCommand::GetState(tx)).await
        .map_err(|_| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Failed to send get state command"))
        })?;

    // Wait for the state response with short timeout (20ms - don't block on offline boards)
    let board_state = tokio::time::timeout(
        Duration::from_millis(20),
        rx
    ).await
    .map_err(|_| -> Box<dyn std::error::Error + Send + Sync> {
        Box::new(std::io::Error::new(std::io::ErrorKind::TimedOut, "Timeout waiting for board state"))
    })?
    .map_err(|_| -> Box<dyn std::error::Error + Send + Sync> {
        Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Failed to receive board state"))
    })?;

    Ok(board_state)
}
