use crate::board::GroupCommand;
use crate::config::Config;
use crate::types::{GroupOperationResult, SharedState};
use std::sync::atomic::{AtomicU8, Ordering};
use tracing::{info, error};

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

    // OPTIMIZATION: Use shared E1.31 for preset/power/brightness commands
    // All group boards assumed to be on universe 1
    let use_e131 = matches!(command, GroupCommand::SetPreset(_, _) | GroupCommand::SetPower(_, _) | GroupCommand::SetBrightness(_, _));

    if use_e131 {
        // Try E1.31 send in separate scope to ensure guard is dropped
        {
            let mut group_e131_lock = state.group_e131.write().await;
            if let Some(ref mut e131) = *group_e131_lock {
                // Send single E1.31 packet to universe 1 (all boards receive)
                let result = match &command {
                    GroupCommand::SetPreset(preset, _transition) => {
                        // Cache the preset for future power/brightness commands
                        CACHED_PRESET.store(*preset, Ordering::Relaxed);
                        info!(group_id = %group_id, preset = preset, "Sending group preset via E1.31 (1 packet to universe 1)");
                        e131.send_preset(*preset, 255)  // Full brightness
                    }
                    GroupCommand::SetPower(on, _transition) => {
                        let preset = CACHED_PRESET.load(Ordering::Relaxed);
                        info!(group_id = %group_id, on = on, preset = if *on { preset } else { 0 }, "Sending group power via E1.31 (1 packet to universe 1)");
                        e131.send_power(*on, preset)  // Use cached preset or blackout (preset 0)
                    }
                    GroupCommand::SetBrightness(brightness, _transition) => {
                        let preset = CACHED_PRESET.load(Ordering::Relaxed);
                        info!(group_id = %group_id, brightness = brightness, preset = preset, "Sending group brightness via E1.31 (1 packet to universe 1)");
                        e131.send_brightness(*brightness, preset)  // Use cached preset
                    }
                    _ => unreachable!(),
                };

                if let Err(e) = result {
                    error!(group_id = %group_id, error = %e, "E1.31 send failed - NO FALLBACK");
                    return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())) as Box<dyn std::error::Error + Send + Sync>);
                }
            } else {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "E1.31 transport not available"
                )));
            }
        }; // guard dropped here

        // E1.31 succeeded - boards already updated
        info!(group_id = %group_id, members = ?group.members, "E1.31 unicast successful, no WebSocket needed");
        return Ok(GroupOperationResult {
            group_id: group_id.to_string(),
            successful_members: group.members.clone(),
            failed_members: vec![],
            member_states: vec![],  // Don't query state, boards will update via E1.31
        });
    }

    // If we get here, E1.31 is not being used for this command type
    // This shouldn't happen for preset/power/brightness commands
    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Unsupported group command type without E1.31")
    )))
}

// execute_board_command removed - no longer using WebSocket fallback for group commands
