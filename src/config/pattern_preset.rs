use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    Wave,
    WaveReverse,
    PingPong,
    Alternate,
    OutsideIn,
    CenterOut,
    Random,
}

impl Default for PatternType {
    fn default() -> Self {
        PatternType::Wave
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PatternPreset {
    pub name: String,
    pub pattern: PatternType,
    pub colour: [u8; 3],
}
