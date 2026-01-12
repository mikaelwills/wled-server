use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock, Mutex};

use crate::board::{BoardCommand, BoardState};
use crate::sse::SseEvent;

// Request structs
#[derive(Deserialize)]
pub struct RegisterBoardRequest {
    pub id: String,
    pub ip: String,
    pub led_count: Option<u16>,
    pub universe: Option<u16>,
}

#[derive(Deserialize)]
pub struct UpdateBoardRequest {
    pub new_id: Option<String>,
    pub new_ip: Option<String>,
    pub led_count: Option<u16>,
    pub universe: Option<u16>,
}

#[derive(Deserialize)]
pub struct OscRequest {
    pub address: String,
    pub ip: String,
    pub port: u16,
}

#[derive(Deserialize)]
pub struct CreateGroupRequest {
    pub id: String,
    pub members: Vec<String>,
    #[serde(default)]
    pub universe: Option<u16>,
}

#[derive(Deserialize)]
pub struct UpdateGroupRequest {
    pub id: String,
    pub members: Vec<String>,
    #[serde(default)]
    pub universe: Option<u16>,
}

#[derive(Deserialize)]
pub struct PowerRequest {
    pub on: bool,
    #[serde(default = "default_transition")]
    pub transition: u8,
}

// Group operation result
#[derive(Debug, Clone, Serialize)]
pub struct GroupOperationResult {
    pub group_id: String,
    pub successful_members: Vec<String>,
    pub failed_members: Vec<(String, String)>, // (board_id, error_message)
    pub member_states: Vec<BoardState>,
}

// Group command request payloads
#[derive(Deserialize)]
pub struct GroupBrightnessRequest {
    pub brightness: u8,
    #[serde(default = "default_transition")]
    pub transition: u8,
}

#[derive(Deserialize)]
pub struct GroupColorRequest {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    #[serde(default = "default_transition")]
    pub transition: u8,
}

#[derive(Deserialize)]
pub struct GroupEffectRequest {
    pub effect: u8,
    #[serde(default = "default_transition")]
    pub transition: u8,
}

#[derive(Deserialize)]
pub struct GroupPresetRequest {
    #[serde(default)]
    pub preset: u8,
    #[serde(default = "default_transition")]
    pub transition: u8,
    #[serde(default)]
    pub bpm: Option<u16>,
    #[serde(default)]
    pub preset_name: Option<String>,
}

#[derive(Deserialize)]
pub struct EffectsEngineStartRequest {
    pub preset: String,
    pub bpm: f64,
    pub target: String,
}

fn default_transition() -> u8 { 0 }

// Audio request/response structs
#[derive(Deserialize)]
pub struct UploadAudioRequest {
    pub data_url: String,
}

#[derive(Serialize)]
pub struct UploadAudioResponse {
    pub audio_file: String,
}

// Preset request structs
#[derive(Deserialize)]
pub struct SavePresetRequest {
    pub name: String,
    pub wled_slot: u8,
    pub board_id: Option<String>,  // Optional board ID to sync preset to
    pub description: Option<String>,
    pub state: Option<PresetState>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WledPreset {
    pub id: String,
    pub name: String,
    pub wled_slot: u8,
    pub description: Option<String>,
    pub state: PresetState,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PresetState {
    pub on: bool,
    pub brightness: u8,
    pub color: [u8; 3],
    pub effect: u8,
    pub speed: u8,
    pub intensity: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<u8>,
}

// Application state structs
#[derive(Clone)]
pub struct BoardEntry {
    pub ip: String,
    pub sender: mpsc::Sender<BoardCommand>,
}

#[derive(Clone)]
pub struct AppState {
    pub boards: Arc<RwLock<HashMap<String, BoardEntry>>>,
    pub broadcast_tx: Arc<broadcast::Sender<SseEvent>>,
    pub storage_paths: Arc<crate::config::StoragePaths>,
    pub group_e131_transports: Arc<RwLock<HashMap<String, crate::transport::E131RawTransport>>>,
    pub config: Arc<Mutex<crate::config::Config>>,
    pub effects_engine: Arc<crate::effects_engine::EffectsEngine>,
    pub pattern_engine: Arc<crate::pattern_engine::PatternEngine>,
    pub programs: Arc<RwLock<HashMap<String, crate::program::Program>>>,
    pub program_engine: Arc<crate::program_engine::ProgramEngine>,
    pub connected_ips: Arc<RwLock<HashSet<String>>>,
    pub performance_mode: Arc<AtomicBool>,
    pub timing_metrics: Arc<crate::timing_metrics::TimingMetrics>,
    pub playback_history: Arc<crate::playback_history::PlaybackHistory>,
}

pub type SharedState = Arc<AppState>;
