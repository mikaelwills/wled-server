use crate::board::BoardState;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum SseEvent {
    #[serde(rename = "state_update")]
    StateUpdate { board_id: String, state: BoardState },
    #[serde(rename = "connection_status")]
    ConnectionStatus { board_id: String, connected: bool },
    #[serde(rename = "resampling_progress")]
    ResamplingProgress { current: u32, total: u32, active: bool, track_name: String, from_rate: u32, to_rate: u32 },
}
