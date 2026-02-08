use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TimecodeConfig {
    #[serde(default)]
    pub osc: OscTimecodeConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OscTimecodeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timecode_osc_ip")]
    pub ip: String,
    #[serde(default = "default_timecode_osc_port")]
    pub port: u16,
    #[serde(default = "default_timecode_osc_address")]
    pub address_prefix: String,
    #[serde(default = "default_timecode_update_rate")]
    pub update_rate_hz: u32,
}

impl Default for OscTimecodeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ip: default_timecode_osc_ip(),
            port: default_timecode_osc_port(),
            address_prefix: default_timecode_osc_address(),
            update_rate_hz: default_timecode_update_rate(),
        }
    }
}

fn default_timecode_osc_ip() -> String {
    "127.0.0.1".to_string()
}

fn default_timecode_osc_port() -> u16 {
    8000
}

fn default_timecode_osc_address() -> String {
    "/timecode".to_string()
}

fn default_timecode_update_rate() -> u32 {
    25
}
