use crate::board::{BoardCommand, BoardState, GroupCommand};
use crate::config::Config;
use crate::types::{GroupOperationResult, SharedState};
use std::time::Duration;

pub struct GroupCommandRouter {
    state: SharedState,
}

impl GroupCommandRouter {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    pub async fn execute_group_command(
        &self,
        group_id: &str,
        command: GroupCommand,
    ) -> Result<GroupOperationResult, Box<dyn std::error::Error>> {
        // Load group configuration to get member board IDs
        let config = Config::load()?;
        let group = config.groups.iter()
            .find(|g| g.id == group_id)
            .ok_or(format!("Group '{}' not found", group_id))?;

        let mut successful_members = Vec::new();
        let mut failed_members = Vec::new();
        let mut member_states = Vec::new();

        // Execute command for each member board
        for board_id in &group.members {
            match self.execute_board_command(board_id, &command).await {
                Ok(state) => {
                    successful_members.push(board_id.clone());
                    member_states.push(state);
                }
                Err(e) => {
                    failed_members.push((board_id.clone(), e.to_string()));
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
        &self,
        board_id: &str,
        command: &GroupCommand,
    ) -> Result<BoardState, Box<dyn std::error::Error>> {
        // Get the board's sender from existing BoardActor infrastructure
        let sender = {
            let senders_lock = self.state.boards.read()
                .map_err(|_| "Failed to acquire boards lock")?;
            
            senders_lock.get(board_id)
                .ok_or(format!("Board '{}' not found or not connected", board_id))?
                .sender.clone()
        };

        // Send the appropriate command to the BoardActor
        match command {
            GroupCommand::SetPower(target_state, transition) => {
                sender.send(BoardCommand::SetPower(*target_state, *transition)).await
                    .map_err(|_| "Failed to send power command")?;
            }
            GroupCommand::SetBrightness(brightness, transition) => {
                sender.send(BoardCommand::SetBrightness(*brightness, *transition)).await
                    .map_err(|_| "Failed to send brightness command")?;
            }
            GroupCommand::SetColor { r, g, b, transition } => {
                sender.send(BoardCommand::SetColor { r: *r, g: *g, b: *b, transition: *transition }).await
                    .map_err(|_| "Failed to send color command")?;
            }
            GroupCommand::SetEffect(effect, transition) => {
                sender.send(BoardCommand::SetEffect(*effect, *transition)).await
                    .map_err(|_| "Failed to send effect command")?;
            }
            GroupCommand::SetPreset(preset, transition) => {
                sender.send(BoardCommand::SetPreset(*preset, *transition)).await
                    .map_err(|_| "Failed to send preset command")?;
            }
        }

        // Get updated state from the BoardActor
        let (tx, rx) = tokio::sync::oneshot::channel();
        sender.send(BoardCommand::GetState(tx)).await
            .map_err(|_| "Failed to send get state command")?;

        // Wait for the state response with timeout
        let board_state = tokio::time::timeout(
            Duration::from_secs(2), 
            rx
        ).await
        .map_err(|_| "Timeout waiting for board state")?
        .map_err(|_| "Failed to receive board state")?;

        Ok(board_state)
    }
}