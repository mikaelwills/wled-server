use crate::types::WledPreset;
use std::fs;
use std::path::Path;

impl WledPreset {
    /// Load all presets from centralized presets.json file
    pub fn load_all(presets_path: &Path) -> Result<Vec<WledPreset>, Box<dyn std::error::Error>> {
        let file_path = presets_path.join("presets.json");

        if !file_path.exists() {
            return Ok(Vec::new());
        }

        let json = fs::read_to_string(file_path)?;
        let presets: Vec<WledPreset> = serde_json::from_str(&json)?;
        Ok(presets)
    }

    /// Save all presets to centralized presets.json file
    pub fn save_all(presets: &[WledPreset], presets_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(presets_path)?;
        let file_path = presets_path.join("presets.json");
        let json = serde_json::to_string_pretty(presets)?;
        fs::write(file_path, json)?;
        Ok(())
    }

    /// Find preset by ID
    pub fn find_by_id(id: &str, presets_path: &Path) -> Result<Option<WledPreset>, Box<dyn std::error::Error>> {
        let all_presets = Self::load_all(presets_path)?;
        Ok(all_presets.into_iter().find(|p| p.id == id))
    }

    /// Convert preset to WLED API JSON format for syncing to board
    pub fn to_wled_json(&self) -> serde_json::Value {
        serde_json::json!({
            "on": self.state.on,
            "bri": self.state.brightness,
            "seg": [{
                "col": [[
                    self.state.color[0],
                    self.state.color[1],
                    self.state.color[2]
                ]],
                "fx": self.state.effect,
                "sx": self.state.speed,
                "ix": self.state.intensity,
            }],
            "psave": self.wled_slot,  // Save to this slot on WLED board
            "n": self.name.clone(),    // Preset name
        })
    }
}
