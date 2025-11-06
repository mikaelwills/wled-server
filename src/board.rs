use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardState {
    pub id: String,
    pub ip: String,
    pub on: bool,
    pub brightness: u8,
    pub color: [u8; 3],
    pub effect: u8,
    pub speed: u8,
    pub intensity: u8,
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ledCount")]
    pub led_count: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "maxLeds")]
    pub max_leds: Option<u16>,
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
            speed: 128,
            intensity: 128,
            connected: false,
            led_count: None,
            max_leds: None,
            is_group: None,
            member_ids: None,
        }
    }
}

pub enum BoardCommand {
    SetPower(bool, u8), // on/off, transition
    SetBrightness(u8, u8), // brightness, transition
    SetColor { r: u8, g: u8, b: u8, transition: u8 },
    SetEffect(u8, u8), // effect, transition
    SetSpeed(u8, u8), // speed, transition
    SetIntensity(u8, u8), // intensity, transition
    SetPreset(u8, u8), // preset, transition
    SetLedCount(u16), // led_count
    ResetSegment, // reset segment to defaults
    GetState(tokio::sync::oneshot::Sender<BoardState>),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum GroupCommand {
    SetPower(bool, u8), // on/off, transition
    SetBrightness(u8, u8), // brightness, transition
    SetColor { r: u8, g: u8, b: u8, transition: u8 },
    SetEffect(u8, u8), // effect, transition
    SetPreset(u8, u8), // preset, transition
}
