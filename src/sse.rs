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
    ResamplingProgress {
        slot: String,
        program_id: String,
        track_name: String,
        current: u32,
        total: u32,
        active: bool,
        from_rate: u32,
        to_rate: u32,
    },
    #[serde(rename = "resampling_complete")]
    ResamplingComplete {
        slot: String,
        program_id: String,
        target_rate: u32,
        quality: String,
    },
    #[serde(rename = "playback_started")]
    PlaybackStarted {
        program_id: String,
        duration_secs: f64,
    },
    #[serde(rename = "playback_position")]
    PlaybackPosition {
        program_id: String,
        position_secs: f64,
        duration_secs: f64,
    },
    #[serde(rename = "playback_stopped")]
    PlaybackStopped {
        program_id: String,
        reason: String,
    },
    #[serde(rename = "resampling_batch_started")]
    ResamplingBatchStarted {
        total_tracks: usize,
        target_rate: u32,
    },
    #[serde(rename = "audio_device_lost")]
    AudioDeviceLost {
        device_name: String,
    },
    #[serde(rename = "audio_device_restored")]
    AudioDeviceRestored {
        device_name: String,
    },
}
