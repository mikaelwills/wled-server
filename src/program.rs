use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub id: String,
    pub song_name: String,
    pub loopy_pro_track: String,
    pub file_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_data: Option<String>,  // Legacy: base64 data for backwards compatibility
    pub audio_file: Option<String>,  // New: filename reference
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
    pub fn save_to_file(&self, programs_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(programs_path)?;
        let file_path = programs_path.join(format!("{}.json", self.id));
        let json = serde_json::to_string_pretty(self)?;
        fs::write(file_path, json)?;
        Ok(())
    }

    pub fn load_from_file(id: &str, programs_path: &Path) -> Result<Program, Box<dyn std::error::Error>> {
        let file_path = programs_path.join(format!("{}.json", id));
        let json = fs::read_to_string(file_path)?;
        let program = serde_json::from_str(&json)?;
        Ok(program)
    }

    pub fn load_all(programs_path: &Path) -> Result<Vec<Program>, Box<dyn std::error::Error>> {
        let mut programs = Vec::new();

        if !programs_path.exists() {
            return Ok(programs);
        }

        for entry in fs::read_dir(programs_path)? {
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

    pub fn delete(&self, programs_path: &Path, audio_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        // Delete program JSON
        let program_file = programs_path.join(format!("{}.json", self.id));
        if program_file.exists() {
            fs::remove_file(program_file)?;
        }

        // Delete audio file if it exists
        if let Some(audio_file) = &self.audio_file {
            let audio_file_path = audio_path.join(audio_file);
            if audio_file_path.exists() {
                fs::remove_file(audio_file_path)?;
            }
        }

        Ok(())
    }
}
