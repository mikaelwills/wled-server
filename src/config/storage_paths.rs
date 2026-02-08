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

    pub fn is_available(&self) -> bool {
        self.programs.exists()
            && self.audio.exists()
            && self.presets.exists()
            && self.history.exists()
    }

    /// Wait for USB-mounted programs directory to become available.
    /// Only applies when programs path is on USB (starts with /tmp/mountd/).
    pub async fn wait_for_usb(&self) {
        let usb_programs_path = &self.programs;
        let is_usb_path = usb_programs_path.to_string_lossy().starts_with("/tmp/mountd/");

        if !is_usb_path {
            return;
        }

        let max_usb_wait_secs = 30;
        let mut usb_waited = 0;

        fn count_json_files(path: &std::path::Path) -> usize {
            match std::fs::read_dir(path) {
                Ok(entries) => entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
                    .count(),
                Err(_) => 0,
            }
        }

        let mut file_count = count_json_files(usb_programs_path);
        while file_count == 0 && usb_waited < max_usb_wait_secs {
            if usb_waited == 0 {
                tracing::warn!("USB programs directory empty or not ready at {:?}, waiting...", usb_programs_path);
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            usb_waited += 1;
            file_count = count_json_files(usb_programs_path);
            if usb_waited % 5 == 0 {
                tracing::info!("Still waiting for USB programs... ({}/{}s, found {} files)", usb_waited, max_usb_wait_secs, file_count);
            }
        }

        if file_count > 0 {
            if usb_waited > 0 {
                tracing::info!("USB programs ready after {}s ({} files found)", usb_waited, file_count);
            } else {
                tracing::info!("USB programs detected ({} files), waiting for stability...", file_count);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }

            if count_json_files(usb_programs_path) == 0 {
                tracing::warn!("USB disappeared after initial detection! Waiting for remount...");
                let mut remount_wait = 0;
                let max_remount_wait = 30;
                while count_json_files(usb_programs_path) == 0 && remount_wait < max_remount_wait {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    remount_wait += 1;
                    if remount_wait % 5 == 0 {
                        tracing::info!("Still waiting for USB remount... ({}/{}s)", remount_wait, max_remount_wait);
                    }
                }
                let final_count = count_json_files(usb_programs_path);
                if final_count > 0 {
                    tracing::info!("USB remounted after {}s ({} files)", remount_wait, final_count);
                } else {
                    tracing::warn!("USB did not remount after {}s", max_remount_wait);
                }
            }
        } else {
            tracing::warn!("No program files found after {}s - directory may be empty or USB not mounted", max_usb_wait_secs);
        }
    }
}
