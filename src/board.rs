use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardState {
    pub id: String,
    pub ip: String,
    pub on: bool,
    pub brightness: u8,
    pub color: [u8; 3],
    pub effect: u8,
    pub connected: bool,
}

impl BoardState {
    pub fn new(id: String, ip: String) -> Self {
        Self {
            id,
            ip,
            on: false,
            brightness: 128,
            color: [255, 255, 255],
            effect: 0,
            connected: false,
        }
    }
}

pub enum BoardCommand {
    TogglePower,
    SetBrightness(u8),
    SetColor { r: u8, g: u8, b: u8 },
    SetEffect(u8),
    SetPreset(u8),
    GetState(tokio::sync::oneshot::Sender<BoardState>),
    Shutdown,
}
