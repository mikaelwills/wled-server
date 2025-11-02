use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;

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

