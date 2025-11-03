use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub id: String,
    pub song_name: String,
    pub loopy_pro_track: String,
    pub file_name: String,
    pub audio_data: String,
    pub cues: Vec<Cue>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cue {
    pub time: f64,
    pub label: String,
    pub boards: Vec<String>,
    pub preset: u8,
    pub color: String,
    pub effect: u8,
    pub brightness: u8,
    pub transition: u8,
}

impl Program {
    pub fn save_to_file(&self) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all("programs")?;
        let file_path = format!("programs/{}.json", self.id);
        let json = serde_json::to_string_pretty(self)?;
        fs::write(file_path, json)?;
        Ok(())
    }

    pub fn load_from_file(id: &str) -> Result<Program, Box<dyn std::error::Error>> {
        let file_path = format!("programs/{}.json", id);
        let json = fs::read_to_string(file_path)?;
        let program = serde_json::from_str(&json)?;
        Ok(program)
    }

     pub fn load_all() -> Result<Vec<Program>, Box<dyn std::error::Error>> {
          let mut programs = Vec::new();

          if !Path::new("programs").exists() {
              return Ok(programs);
          }

          for entry in fs::read_dir("programs")? {
              let entry = entry?;
              if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                  let json = fs::read_to_string(entry.path())?;
                  if let Ok(program) = serde_json::from_str(&json) {
                      programs.push(program);
                  }
              }
          }
          Ok(programs)
      }

}
