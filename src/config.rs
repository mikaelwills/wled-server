use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::env;

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

#[derive(Debug, Deserialize, Serialize)]
  pub struct Config {
      pub boards: Vec<BoardConfig>,
      #[serde(default)]
      pub groups: Vec<GroupConfig>,
  }

  #[derive(Debug, Deserialize, Serialize, Clone)]
  pub struct BoardConfig {
      pub id: String,
      pub ip: String,
      #[serde(default)]
      pub e131_enabled: bool,
      #[serde(default = "default_e131_universe")]
      pub e131_universe: u16,
  }

  fn default_e131_universe() -> u16 {
      1 // Default E1.31 universe
  }

  impl BoardConfig {
      pub fn e131_config(&self) -> Option<E131Config> {
          if self.e131_enabled {
              Some(E131Config {
                  enabled: true,
                  universe: self.e131_universe,
              })
          } else {
              None
          }
      }
  }

  #[derive(Debug, Clone)]
  pub struct E131Config {
      pub enabled: bool,
      pub universe: u16,
  }

  #[derive(Debug, Deserialize, Serialize, Clone)]
  pub struct GroupConfig {
      pub id: String,
      pub members: Vec<String>,
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

          // Write to temp file and explicitly sync to disk
          let mut file = fs::File::create(temp_path)?;
          file.write_all(toml_string.as_bytes())?;
          file.sync_all()?; // Ensure data is flushed to disk
          drop(file); // Close file before rename

          fs::rename(temp_path, "data/boards.toml")?;
          Ok(())
      }
  }

