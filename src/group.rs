use crate::board::{BoardCommand, BoardState, GroupCommand};
use crate::config::Config;
use crate::types::{GroupOperationResult, SharedState};
use futures::future::join_all;
use std::time::Duration;

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

    // Execute commands for all member boards IN PARALLEL
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