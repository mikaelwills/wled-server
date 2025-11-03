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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "isGroup")]
    pub is_group: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "memberIds")]
    pub member_ids: Option<Vec<String>>,
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
            is_group: None,
            member_ids: None,
        }
    }

    pub fn new_group(id: String, members: Vec<String>) -> Self {
        Self {
            id,
            ip: String::new(),
            on: false,
            brightness: 128,
            color: [255, 255, 255],
            effect: 0,
            connected: true,
            is_group: Some(true),
            member_ids: Some(members),
        }
    }
}

pub enum BoardCommand {
    TogglePower,
    SetBrightness(u8, u8), // brightness, transition
    SetColor { r: u8, g: u8, b: u8, transition: u8 },
    SetEffect(u8, u8), // effect, transition
    SetPreset(u8, u8), // preset, transition
    GetState(tokio::sync::oneshot::Sender<BoardState>),
    Shutdown,
}
