mod audio_thread;
mod click;
mod device_manager;
mod engine;
mod file;
mod loader;
mod resampler;
mod resampling_progress;
mod track;

pub use audio_thread::{AudioThread, PlaybackHealth};
pub use device_manager::{AudioDevice, DeviceManager};
pub use engine::{
    AudioEngine, PlaybackCommand, PlaybackState, RoutingConfig, SlotId, SLOT_COUNT,
};
pub use file::AudioFile;
pub use loader::{decode_file, decode_file_with_path, DecodedAudio};
pub use resampler::{spawn_resampling, ResamplingJob};
pub use resampling_progress::ResamplingProgress;
pub use track::LoadedTrack;

use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Spawn a background task that preloads all audio files into the engine.
pub fn spawn_preload_task(audio_engine: Arc<Mutex<AudioEngine>>, audio_path: impl AsRef<Path>) {
    let audio_path = audio_path.as_ref().to_path_buf();
    tokio::spawn(async move {
        if !audio_path.exists() {
            return;
        }
        let cache_dir = audio_path.join("resampled");
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            tracing::warn!("Failed to create resampled cache dir: {}", e);
        }
        let entries: Vec<_> = match std::fs::read_dir(&audio_path) {
            Ok(e) => e.flatten().collect(),
            Err(_) => return,
        };
        let mut backing_count = 0;
        let mut guide_count = 0;
        for entry in entries {
            let path = entry.path();
            let is_audio = path.extension().map_or(false, |ext| {
                ext == "mp3" || ext == "wav"
            });
            if is_audio {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let stem = stem.to_string();
                    let is_guide = stem.contains("_guide");
                    let path_clone = path.clone();
                    let cache_dir_clone = cache_dir.clone();
                    let decode_result = tokio::task::spawn_blocking(move || {
                        decode_file_with_path(&path_clone)
                    }).await;

                    match decode_result {
                        Ok(Ok(decoded)) => {
                            let track = decoded.track.with_source_info(decoded.source_path, cache_dir_clone);
                            let mut engine = audio_engine.lock().await;
                            if is_guide {
                                engine.load_guide_track(stem, track).await;
                                guide_count += 1;
                            } else {
                                engine.load_track(stem, track).await;
                                backing_count += 1;
                            }
                        }
                        Ok(Err(e)) => {
                            tracing::warn!("Failed to preload audio '{}': {}", path.display(), e);
                        }
                        Err(e) => {
                            tracing::warn!("Decode task failed for '{}': {}", path.display(), e);
                        }
                    }
                }
            }
        }
        if backing_count > 0 || guide_count > 0 {
            tracing::info!("Background: preloaded {} backing + {} guide track(s) into engine", backing_count, guide_count);
        }
    });
}
