use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EffectPreset {
    pub name: String,
    pub effect_type: String,
    pub color: [u8; 3],
}
