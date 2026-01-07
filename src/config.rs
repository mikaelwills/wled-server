use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct StoragePaths {
    pub programs: PathBuf,
    pub audio: PathBuf,
    pub presets: PathBuf,
}

impl Default for StoragePaths {
    fn default() -> Self {
        Self {
            programs: env::var("WLED_PROGRAMS_PATH")
                .unwrap_or_else(|_| "programs".to_string())
                .into(),
            audio: env::var("WLED_AUDIO_PATH")
                .unwrap_or_else(|_| "audio".to_string())
                .into(),
            presets: env::var("WLED_PRESETS_PATH")
                .unwrap_or_else(|_| "presets".to_string())
                .into(),
        }
    }
}

impl StoragePaths {
    pub fn init(&self) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(&self.programs)?;
        fs::create_dir_all(&self.audio)?;
        fs::create_dir_all(&self.presets)?;
        tracing::info!("Storage paths initialized:");
        tracing::info!("  Programs: {:?}", self.programs);
        tracing::info!("  Audio: {:?}", self.audio);
        tracing::info!("  Presets: {:?}", self.presets);
        Ok(())
    }

    pub fn is_available(&self) -> bool {
        self.programs.exists() && self.audio.exists() && self.presets.exists()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub boards: Vec<BoardConfig>,
    #[serde(default)]
    pub groups: Vec<GroupConfig>,
    #[serde(default)]
    pub loopy_pro: LoopyProConfig,
    #[serde(default = "default_effect_presets")]
    pub effect_presets: Vec<EffectPreset>,
    #[serde(default)]
    pub pattern_presets: Vec<PatternPreset>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LoopyProConfig {
    #[serde(default = "default_loopy_ip")]
    pub ip: String,
    #[serde(default = "default_loopy_port")]
    pub port: u16,
    #[serde(default)]
    pub mute_audio: bool,
    #[serde(default)]
    pub audio_sync_delay_ms: i64,
}

impl Default for LoopyProConfig {
    fn default() -> Self {
        Self {
            ip: default_loopy_ip(),
            port: default_loopy_port(),
            mute_audio: false,
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

fn default_effect_presets() -> Vec<EffectPreset> {
    vec![EffectPreset {
        name: "Off".to_string(),
        effect_type: "solid".to_string(),
        color: [0, 0, 0],
    }]
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BoardConfig {
    pub id: String,
    pub ip: String,
    #[serde(default = "default_transition")]
    pub transition: Option<u8>,
    #[serde(default)]
    pub led_count: Option<u16>,
    #[serde(default)]
    pub universe: Option<u16>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GroupConfig {
    pub id: String,
    pub members: Vec<String>,
    #[serde(default)]
    pub universe: Option<u16>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EffectPreset {
    pub name: String,
    pub effect_type: String,
    pub color: [u8; 3],
}

fn default_transition() -> Option<u8> {
    None
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string("data/boards.toml")?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let toml_string = toml::to_string_pretty(&self)?;
        let temp_path = "data/boards.toml.tmp";

        let mut file = fs::File::create(temp_path)?;
        file.write_all(toml_string.as_bytes())?;
        file.sync_all()?;
        drop(file);

        fs::rename(temp_path, "data/boards.toml")?;
        Ok(())
    }

    pub fn find_board(&self, id: &str) -> Option<&BoardConfig> {
        self.boards.iter().find(|b| b.id == id)
    }

    pub fn find_group(&self, id: &str) -> Option<&GroupConfig> {
        self.groups.iter().find(|g| g.id == id)
    }

    pub fn find_effect_preset(&self, name: &str) -> Option<&EffectPreset> {
        let name_lower = name.to_lowercase();
        self.effect_presets
            .iter()
            .find(|p| p.name.to_lowercase() == name_lower)
    }

    pub fn get_target_boards(&self, target: &str) -> Vec<&BoardConfig> {
        if let Some(group) = self.find_group(target) {
            group
                .members
                .iter()
                .filter_map(|member_id| self.find_board(member_id))
                .collect()
        } else if let Some(board) = self.find_board(target) {
            vec![board]
        } else {
            vec![]
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    Wave,
    WaveReverse,
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
