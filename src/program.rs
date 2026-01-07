use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

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
    #[serde(default)]
    pub display_order: i32,  // Order for performance page display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_target_board: Option<String>,  // Default board/group for new cues
    // Auto-play chain fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_program_id: Option<String>,  // ID of program to auto-play next
    #[serde(default = "default_transition_type")]
    pub transition_type: String,  // "immediate", "blackout", or "hold"
    #[serde(default)]
    pub transition_duration: u32,  // Duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_duration: Option<f64>,  // Audio duration in seconds (for muted playback chains)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bpm: Option<u16>,  // BPM for speed-synced effects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_offset: Option<f64>,  // Downbeat position for beat grid alignment
}

fn default_transition_type() -> String {
    "immediate".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cue {
    pub time: f64,
    pub label: String,
    pub targets: Vec<String>,
    pub preset_name: String,
    #[serde(default = "default_sync_rate")]
    pub sync_rate: f64,
}

fn default_sync_rate() -> f64 {
    1.0
}

impl Program {
    pub fn save_to_file(&self, programs_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(programs_path)?;
        let file_path = programs_path.join(format!("{}.json", self.id));
        let json = serde_json::to_string_pretty(self)?;
        fs::write(file_path, json)?;
        Ok(())
    }

    pub fn load_all(programs_path: &Path) -> Result<Vec<Program>, Box<dyn std::error::Error>> {
        let mut programs = Vec::new();

        if !programs_path.exists() {
            return Ok(programs);
        }

        for entry in fs::read_dir(programs_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                match fs::read_to_string(&path) {
                    Ok(json) => {
                        match serde_json::from_str::<Program>(&json) {
                            Ok(program) => {
                                info!("Loaded program: {}", program.id);
                                programs.push(program);
                            }
                            Err(e) => {
                                warn!("Failed to parse program {}: {}", path.display(), e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read program file {}: {}", path.display(), e);
                    }
                }
            }
        }

        // Sort by display_order (ascending)
        programs.sort_by_key(|p| p.display_order);

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
