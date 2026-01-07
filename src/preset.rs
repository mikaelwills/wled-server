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

    /// Convert preset to WLED API JSON format for syncing to board
    pub fn to_wled_json(&self) -> serde_json::Value {
        serde_json::json!({
            "on": self.state.on,
            "bri": self.state.brightness,
            "transition": self.state.transition.unwrap_or(0),  // Transition time (0 = instant)
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

    /// Convert preset to WLED presets.json file format (for direct file upload)
    pub fn to_wled_file_format(&self) -> serde_json::Value {
        serde_json::json!({
            "mainseg": 0,
            "seg": [{
                "id": 0,
                "grp": 1,
                "spc": 0,
                "of": 0,
                "on": self.state.on,
                "frz": false,
                "bri": self.state.brightness,
                "cct": 127,
                "set": 0,
                "col": [
                    [self.state.color[0], self.state.color[1], self.state.color[2], 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0]
                ],
                "fx": self.state.effect,
                "sx": self.state.speed,
                "ix": self.state.intensity,
                "pal": 0,
                "c1": 128,
                "c2": 128,
                "c3": 16,
                "sel": true,
                "rev": false,
                "mi": false,
                "o1": false,
                "o2": false,
                "o3": false,
                "si": 0,
                "m12": 0
            }],
            "n": self.name.clone()
        })
    }

    /// Build complete WLED presets.json file from all presets
    pub fn build_wled_presets_file(presets: &[WledPreset]) -> serde_json::Value {
        let mut file_obj = serde_json::Map::new();
        file_obj.insert("0".to_string(), serde_json::json!({}));
        for preset in presets {
            let slot_key = preset.wled_slot.to_string();
            file_obj.insert(slot_key, preset.to_wled_file_format());
        }
        serde_json::Value::Object(file_obj)
    }
}
