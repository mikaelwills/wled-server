use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setlist {
    pub id: String,
    pub name: String,
    pub display_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetlistStore {
    pub setlists: Vec<Setlist>,
    #[serde(default = "default_active_setlist_id")]
    pub active_setlist_id: String,
}

fn default_active_setlist_id() -> String {
    "default".to_string()
}

impl SetlistStore {
    pub fn load(programs_path: &Path) -> Self {
        let file_path = programs_path.join("setlists.json");
        if file_path.exists() {
            match fs::read_to_string(&file_path) {
                Ok(json) => match serde_json::from_str::<SetlistStore>(&json) {
                    Ok(store) => {
                        info!("Loaded {} setlist(s) from disk", store.setlists.len());
                        return store;
                    }
                    Err(e) => {
                        warn!("Failed to parse setlists.json: {}", e);
                    }
                },
                Err(e) => {
                    warn!("Failed to read setlists.json: {}", e);
                }
            }
        }

        let store = SetlistStore {
            setlists: vec![Setlist {
                id: "default".to_string(),
                name: "Default Set".to_string(),
                display_order: 0,
            }],
            active_setlist_id: "default".to_string(),
        };
        info!("Created default setlist store");
        store
    }

    pub fn save(&self, programs_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let file_path = programs_path.join("setlists.json");
        let json = serde_json::to_string_pretty(self)?;
        fs::write(file_path, json)?;
        Ok(())
    }
}
