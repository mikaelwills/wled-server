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
pub use track::{CachedVersion, LoadedTrack};

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use futures::stream::{self, StreamExt};

use std::collections::HashSet;

use crate::sse::SseEvent;

pub struct AudioFileSet {
    pub backing: Vec<String>,
    pub guide: Vec<String>,
}

pub fn spawn_preload_filtered(
    audio_engine: Arc<Mutex<AudioEngine>>,
    audio_path: impl AsRef<Path>,
    filter: Option<AudioFileSet>,
    broadcast_tx: Option<Arc<broadcast::Sender<SseEvent>>>,
    device_sample_rate: u32,
) {
    let audio_path = audio_path.as_ref().to_path_buf();
    tokio::spawn(async move {
        if !audio_path.exists() {
            return;
        }
        let cache_dir = audio_path.join("resampled");
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            tracing::warn!("Failed to create resampled cache dir: {}", e);
        }

        let allowed_backing: Option<HashSet<String>> = filter.as_ref().map(|f| f.backing.iter().cloned().collect());
        let allowed_guide: Option<HashSet<String>> = filter.as_ref().map(|f| f.guide.iter().cloned().collect());

        let entries: Vec<_> = match std::fs::read_dir(&audio_path) {
            Ok(e) => e.flatten().collect(),
            Err(_) => return,
        };

        if let Some(ref tx) = broadcast_tx {
            let total_tracks = entries.iter().filter(|entry| {
                let path = entry.path();
                let is_audio = path.extension().map_or(false, |ext| ext == "mp3" || ext == "wav");
                if !is_audio { return false; }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { return false };
                let is_guide = stem.contains("_guide");
                let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();
                if let Some(ref allowed) = if is_guide { &allowed_guide } else { &allowed_backing } {
                    if !allowed.contains(&filename) { return false; }
                }
                true
            }).count();
            if total_tracks > 0 && device_sample_rate > 0 {
                let _ = tx.send(SseEvent::ResamplingBatchStarted {
                    total_tracks,
                    target_rate: device_sample_rate,
                });
            }
        }

        let mut to_decode: Vec<(PathBuf, String, bool, PathBuf)> = Vec::new();
        for entry in entries {
            let path = entry.path();
            let is_audio = path.extension().map_or(false, |ext| ext == "mp3" || ext == "wav");
            if !is_audio { continue; }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let stem = stem.to_string();
            let is_guide = stem.contains("_guide");
            let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();

            if let Some(ref allowed) = if is_guide { &allowed_guide } else { &allowed_backing } {
                if !allowed.contains(&filename) {
                    continue;
                }
            }

            to_decode.push((path, stem, is_guide, cache_dir.clone()));
        }

        let mut backing_count = 0;
        let mut guide_count = 0;

        let mut decode_stream = stream::iter(to_decode)
            .map(|(path, stem, is_guide, cache_dir_clone)| {
                tokio::task::spawn_blocking(move || {
                    let result = decode_file_with_path(&path);
                    (path, stem, is_guide, cache_dir_clone, result)
                })
            })
            .buffer_unordered(4);

        while let Some(join_result) = decode_stream.next().await {
            match join_result {
                Ok((_path, stem, is_guide, cache_dir_clone, Ok(decoded))) => {
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
                Ok((path, _, _, _, Err(e))) => {
                    tracing::warn!("Failed to preload audio '{}': {}", path.display(), e);
                }
                Err(e) => {
                    tracing::warn!("Decode task failed: {}", e);
                }
            }
        }

        if backing_count > 0 || guide_count > 0 {
            tracing::info!("Background: preloaded {} backing + {} guide track(s) into engine", backing_count, guide_count);
        }
    });
}

pub fn collect_audio_files(programs: &std::collections::HashMap<String, crate::program::Program>, setlist_id: &str) -> AudioFileSet {
    let mut backing = Vec::new();
    let mut guide = Vec::new();
    for program in programs.values() {
        if program.setlist_id != setlist_id {
            continue;
        }
        if let Some(ref audio_file) = program.audio_file {
            backing.push(audio_file.clone());
        }
        if let Some(ref guide_file) = program.guide_audio_file {
            guide.push(guide_file.clone());
        }
    }
    AudioFileSet { backing, guide }
}
