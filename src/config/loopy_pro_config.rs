use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioSource {
    #[default]
    AudioEngine,
    LoopyPro,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LoopyProConfig {
    #[serde(default = "default_loopy_ip")]
    pub ip: String,
    #[serde(default = "default_loopy_port")]
    pub port: u16,
    #[serde(default)]
    pub audio_source: AudioSource,
    #[serde(default)]
    pub audio_sync_delay_ms: i64,
}

impl Default for LoopyProConfig {
    fn default() -> Self {
        Self {
            ip: default_loopy_ip(),
            port: default_loopy_port(),
            audio_source: AudioSource::AudioEngine,
            audio_sync_delay_ms: 0,
        }
    }
}

fn default_loopy_ip() -> String {
    "192.168.1.242".to_string()
}

fn default_loopy_port() -> u16 {
    9595
}
