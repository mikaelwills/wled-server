use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct StoragePaths {
    pub programs: PathBuf,
    pub audio: PathBuf,
    pub presets: PathBuf,
    pub history: PathBuf,
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
            history: env::var("WLED_HISTORY_PATH")
                .unwrap_or_else(|_| "history".to_string())
                .into(),
        }
    }
}

impl StoragePaths {
    pub fn is_available(&self) -> bool {
        self.programs.exists()
            && self.audio.exists()
            && self.presets.exists()
            && self.history.exists()
    }

    pub fn init(&self) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(&self.programs)?;
        fs::create_dir_all(&self.audio)?;
        fs::create_dir_all(&self.presets)?;
        fs::create_dir_all(&self.history)?;
        tracing::info!("Storage paths initialized:");
        tracing::info!("  Programs: {:?}", self.programs);
        tracing::info!("  Audio: {:?}", self.audio);
        tracing::info!("  Presets: {:?}", self.presets);
        tracing::info!("  History: {:?}", self.history);
        Ok(())
    }

}
