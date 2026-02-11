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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guide_audio_file: Option<String>,  // Guide track filename
    #[serde(skip_serializing_if = "Option::is_none")]
    pub click_rate: Option<f64>,  // Click track rate multiplier (0.5, 1.0, or 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guide_volume: Option<f64>,
}

fn default_transition_type() -> String {
    "immediate".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cue {
    pub time: f64,
    pub label: String,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
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

        info!("Loading programs from: {:?}", programs_path);

        if !programs_path.exists() {
            warn!("Programs path does not exist: {:?}", programs_path);
            return Ok(programs);
        }

        let entries: Vec<_> = fs::read_dir(programs_path)?.collect();
        info!("Found {} entries in programs directory", entries.len());

        for entry in entries {
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
        info!("Deleting program: {}", self.id);

        if let Some(guide_file) = &self.guide_audio_file {
            let guide_file_path = audio_path.join(guide_file);
            if guide_file_path.exists() {
                fs::remove_file(&guide_file_path)?;
                info!("Deleted guide audio: {}", guide_file);
            }
            let guide_peaks_path = audio_path.join(format!("{}.peaks.json", guide_file));
            if guide_peaks_path.exists() {
                fs::remove_file(&guide_peaks_path)?;
                info!("Deleted guide peaks: {}.peaks.json", guide_file);
            }
            let guide_cache_dir = audio_path.join("resampled").join(guide_file);
            if guide_cache_dir.exists() {
                fs::remove_dir_all(&guide_cache_dir)?;
                info!("Deleted guide resampled cache: resampled/{}", guide_file);
            }
        }

        if let Some(audio_file) = &self.audio_file {
            let audio_file_path = audio_path.join(audio_file);
            if audio_file_path.exists() {
                fs::remove_file(&audio_file_path)?;
                info!("Deleted audio: {}", audio_file);
            }
            let peaks_path = audio_path.join(format!("{}.peaks.json", audio_file));
            if peaks_path.exists() {
                fs::remove_file(&peaks_path)?;
                info!("Deleted peaks: {}.peaks.json", audio_file);
            }
            let cache_dir = audio_path.join("resampled").join(audio_file);
            if cache_dir.exists() {
                fs::remove_dir_all(&cache_dir)?;
                info!("Deleted resampled cache: resampled/{}", audio_file);
            }
        }

        let program_file = programs_path.join(format!("{}.json", self.id));
        if program_file.exists() {
            fs::remove_file(&program_file)?;
            info!("Deleted program JSON: {}.json", self.id);
        }

        info!("Program deletion complete: {}", self.id);
        Ok(())
    }
}
