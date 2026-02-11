mod audio_config;
mod board_config;
mod effect_preset;
mod group_config;
mod loopy_pro_config;
mod pattern_preset;
mod storage_paths;
mod timecode_config;

pub use audio_config::{AudioConfig, DeviceRouting, ResamplingQuality};
pub use board_config::{configure_board_universe, BoardConfig};
pub use effect_preset::EffectPreset;
pub use group_config::GroupConfig;
pub use loopy_pro_config::{AudioSource, LoopyProConfig};
pub use pattern_preset::{PatternPreset, PatternType};
pub use storage_paths::StoragePaths;
pub use timecode_config::{OscTimecodeConfig, TimecodeConfig};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use tracing::{info, warn};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Config {
    #[serde(default)]
    pub boards: Vec<BoardConfig>,
    #[serde(default)]
    pub groups: Vec<GroupConfig>,
    #[serde(default)]
    pub loopy_pro: LoopyProConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub timecode: TimecodeConfig,
    #[serde(default)]
    pub effect_presets: Vec<EffectPreset>,
    #[serde(default)]
    pub pattern_presets: Vec<PatternPreset>,
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = std::path::Path::new("data/boards.toml");
        if !path.exists() {
            warn!("data/boards.toml not found, using defults");
            return Ok(Config::default());
        }
        let contents = fs::read_to_string(path)?;
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

    pub fn set_preferred_audio_device(&mut self, device_id: Option<String>) {
        self.audio.preferred_device_id = device_id;
    }

    pub async fn init_e131_transports(
        &self,
    ) -> HashMap<String, crate::transport::E131RawTransport> {
        let mut group_e131_transports = HashMap::new();

        for (universe_index, group) in self.groups.iter().enumerate() {
            let mut group_board_ips: Vec<String> = Vec::new();
            for member_id in &group.members {
                if let Some(board) = self.boards.iter().find(|b| &b.id == member_id) {
                    if !group_board_ips.contains(&board.ip) {
                        group_board_ips.push(board.ip.clone());
                    }
                }
            }

            if !group_board_ips.is_empty() {
                let universe = group.universe.unwrap_or((universe_index + 1) as u16);

                info!(
                    group_id = %group.id,
                    universe = universe,
                    board_count = group_board_ips.len(),
                    "Initializing E1.31 transport for group: {:?}",
                    group_board_ips
                );

                match crate::transport::E131RawTransport::new(group_board_ips, universe) {
                    Ok(transport) => {
                        group_e131_transports.insert(group.id.clone(), transport);
                        info!(group_id = %group.id, universe = universe, "E1.31 transport initialized");
                    }
                    Err(e) => {
                        warn!(group_id = %group.id, "Failed to initialize E1.31 transport: {}", e);
                    }
                }
            } else {
                warn!(group_id = %group.id, "No boards found for group - will use WebSocket only");
            }
        }

        info!(
            "Initialized {} E1.31 group transport(s)",
            group_e131_transports.len()
        );

        info!("Configuring board E1.31 universes in parallel...");
        let mut config_tasks = Vec::new();

        for board in &self.boards {
            if let Some(universe) = board.universe {
                let board_id = board.id.clone();
                let board_ip = board.ip.clone();

                let task = tokio::spawn(async move {
                    info!(
                        board_id = %board_id,
                        universe = universe,
                        "Configuring board universe"
                    );

                    match configure_board_universe(&board_ip, universe).await {
                        Ok(()) => {
                            info!(board_id = %board_id, universe = universe, "Successfully configured universe");
                        }
                        Err(e) => {
                            warn!(
                                board_id = %board_id,
                                universe = universe,
                                "Failed to configure universe: {}. Board may need manual configuration.", e
                            );
                        }
                    }
                });

                config_tasks.push(task);
            }
        }

        let config_timeout = tokio::time::Duration::from_secs(10);
        match tokio::time::timeout(config_timeout, futures::future::join_all(config_tasks)).await {
            Ok(_) => info!("Universe configuration complete"),
            Err(_) => {
                warn!("Universe configuration timed out after 10s - some boards may not be configured")
            }
        }

        group_e131_transports
    }
}
