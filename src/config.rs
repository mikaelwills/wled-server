use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize, Serialize)]
  pub struct Config {
      pub boards: Vec<BoardConfig>,
  }

  #[derive(Debug, Deserialize, Serialize, Clone)]
  pub struct BoardConfig {
      pub id: String,
      pub ip: String,
  }

  impl Config {
      pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
          let contents = fs::read_to_string("boards.toml")?;
          let config: Config = toml::from_str(&contents)?;
          Ok(config)
      }

      pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
          let toml_string = toml::to_string_pretty(&self)?;
          let temp_path = "boards.toml.tmp";
          fs::write(temp_path, toml_string)?;
          fs::rename(temp_path, "boards.toml")?;
          Ok(())
      }
  }

