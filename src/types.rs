use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, mpsc};

use crate::board::BoardCommand;
use crate::sse::SseEvent;

// Request structs
#[derive(Deserialize)]
pub struct RegisterBoardRequest {
    pub id: String,
    pub ip: String,
}

#[derive(Deserialize)]
pub struct UpdateBoardRequest {
    pub new_id: Option<String>,
    pub new_ip: Option<String>,
}

#[derive(Deserialize)]
pub struct OscRequest {
    pub address: String,
}

#[derive(Deserialize)]
pub struct CreateGroupRequest {
    pub id: String,
    pub members: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateGroupRequest {
    pub members: Vec<String>,
}

// Application state structs
pub struct BoardEntry {
    pub ip: String,
    pub sender: mpsc::Sender<BoardCommand>,
}

#[derive(Clone)]
pub struct AppState {
    pub boards: Arc<RwLock<HashMap<String, BoardEntry>>>,
    pub broadcast_tx: Arc<broadcast::Sender<SseEvent>>,
}

pub type SharedState = Arc<AppState>;
