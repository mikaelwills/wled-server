use parking_lot::RwLock;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use super::resampler;
use crate::config::ResamplingQuality;

const CACHE_MAGIC: &[u8; 4] = b"WPCM";
const CACHE_HEADER_SIZE: usize = 24;

#[derive(Debug, Clone, Serialize)]
pub struct CachedVersion {
    pub target_rate: u32,
    pub channels: u16,
    pub quality: String,
    pub size_bytes: u64,
}

pub struct LoadedTrack {
    pub original_samples: Arc<Vec<f32>>,
    pub original_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
    source_path: Option<PathBuf>,
    source_mtime: u64,
    cache_dir: Option<PathBuf>,
    resampled_cache: RwLock<HashMap<u32, Arc<Vec<f32>>>>,
}

impl LoadedTrack {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        let duration_secs = samples.len() as f64 / (sample_rate as f64 * channels as f64);
        Self {
            original_samples: Arc::new(samples),
            original_rate: sample_rate,
            channels,
            duration_secs,
            source_path: None,
            source_mtime: 0,
            cache_dir: None,
            resampled_cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn with_source_info(mut self, source_path: PathBuf, cache_dir: PathBuf) -> Self {
        self.source_mtime = fs::metadata(&source_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.source_path = Some(source_path);
        self.cache_dir = Some(cache_dir);
        self
    }

    fn cache_dir_for_track(&self) -> Option<PathBuf> {
        let source = self.source_path.as_ref()?;
        let cache_dir = self.cache_dir.as_ref()?;
        let filename = source.file_name()?.to_str()?;
        Some(cache_dir.join(filename))
    }

    fn cache_path(&self, target_rate: u32, quality: ResamplingQuality) -> Option<PathBuf> {
        let dir = self.cache_dir_for_track()?;
        Some(dir.join(format!("{}_{}_{}.pcm", target_rate, self.channels, quality.cache_key())))
    }

    fn legacy_cache_path(&self, target_rate: u32) -> Option<PathBuf> {
        let dir = self.cache_dir_for_track()?;
        Some(dir.join(format!("{}_{}.pcm", target_rate, self.channels)))
    }

    fn load_from_cache(&self, target_rate: u32, quality: ResamplingQuality) -> Option<Vec<f32>> {
        let path = self.cache_path(target_rate, quality)?;
        if !path.exists() {
            let legacy = self.legacy_cache_path(target_rate)?;
            if legacy.exists() {
                return self.load_cache_file(&legacy, target_rate);
            }
            return None;
        }
        self.load_cache_file(&path, target_rate)
    }

    fn load_cache_file(&self, path: &PathBuf, target_rate: u32) -> Option<Vec<f32>> {
        let mut file = File::open(path).ok()?;
        let mut header = [0u8; CACHE_HEADER_SIZE];
        file.read_exact(&mut header).ok()?;

        if &header[0..4] != CACHE_MAGIC {
            eprintln!("[Track] Cache invalid magic: {:?}", path);
            return None;
        }

        let cached_rate = u32::from_le_bytes(header[4..8].try_into().ok()?);
        let cached_channels = u16::from_le_bytes(header[8..10].try_into().ok()?);
        let cached_mtime = u64::from_le_bytes(header[16..24].try_into().ok()?);

        if cached_rate != target_rate || cached_channels != self.channels {
            eprintln!("[Track] Cache rate/channel mismatch: {:?}", path);
            return None;
        }

        if cached_mtime != self.source_mtime && self.source_mtime > 0 {
            eprintln!("[Track] Cache stale (mtime {} vs {}): {:?}", cached_mtime, self.source_mtime, path);
            let _ = fs::remove_file(path);
            return None;
        }

        let mut sample_bytes = Vec::new();
        file.read_to_end(&mut sample_bytes).ok()?;

        if sample_bytes.len() % 4 != 0 {
            eprintln!("[Track] Cache corrupted (bad size): {:?}", path);
            return None;
        }

        let samples: Vec<f32> = sample_bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();

        eprintln!("[Track] Loaded {} samples from disk cache: {:?}", samples.len(), path);
        Some(samples)
    }

    pub fn save_to_cache(&self, target_rate: u32, quality: ResamplingQuality, samples: &[f32]) {
        let Some(path) = self.cache_path(target_rate, quality) else { return };

        if let Some(parent) = path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("[Track] Failed to create cache dir: {}", e);
                return;
            }
        }

        self.delete_other_qualities(target_rate, quality);

        let tmp_path = path.with_extension("pcm.tmp");
        let result = (|| -> std::io::Result<()> {
            let mut file = File::create(&tmp_path)?;

            let mut header = [0u8; CACHE_HEADER_SIZE];
            header[0..4].copy_from_slice(CACHE_MAGIC);
            header[4..8].copy_from_slice(&target_rate.to_le_bytes());
            header[8..10].copy_from_slice(&self.channels.to_le_bytes());
            header[10] = quality.header_byte();
            header[16..24].copy_from_slice(&self.source_mtime.to_le_bytes());
            file.write_all(&header)?;

            for sample in samples {
                file.write_all(&sample.to_le_bytes())?;
            }

            file.sync_all()?;
            drop(file);
            fs::rename(&tmp_path, &path)?;
            Ok(())
        })();

        match result {
            Ok(()) => eprintln!("[Track] Saved {} samples to disk cache ({:?}): {:?}", samples.len(), quality, path),
            Err(e) => {
                eprintln!("[Track] Failed to save cache: {}", e);
                let _ = fs::remove_file(&tmp_path);
            }
        }
    }

    fn delete_other_qualities(&self, target_rate: u32, keep_quality: ResamplingQuality) {
        let Some(dir) = self.cache_dir_for_track() else { return };
        let prefix = format!("{}_{}_", target_rate, self.channels);
        let keep_name = format!("{}{}.pcm", prefix, keep_quality.cache_key());
        let legacy_name = format!("{}_{}.pcm", target_rate, self.channels);

        let Ok(entries) = fs::read_dir(&dir) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else { continue };
            if name_str == keep_name { continue; }
            if (name_str.starts_with(&prefix) && name_str.ends_with(".pcm") && !name_str.ends_with(".tmp"))
                || name_str == legacy_name
            {
                eprintln!("[Track] Removing old cache: {:?}", entry.path());
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    pub fn delete_cache_for_rate(&self, target_rate: u32) {
        let Some(dir) = self.cache_dir_for_track() else { return };
        let prefix = format!("{}_{}", target_rate, self.channels);

        let Ok(entries) = fs::read_dir(&dir) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else { continue };
            if name_str.starts_with(&prefix) && name_str.ends_with(".pcm") && !name_str.ends_with(".tmp") {
                eprintln!("[Track] Deleting cache file: {:?}", entry.path());
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    pub fn clear_resampled_for_rate(&self, rate: u32) {
        let mut cache = self.resampled_cache.write();
        cache.remove(&rate);
    }

    pub fn find_cached_versions(&self) -> Vec<CachedVersion> {
        let Some(dir) = self.cache_dir_for_track() else { return Vec::new() };
        let Ok(entries) = fs::read_dir(&dir) else { return Vec::new() };

        let mut versions = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else { continue };
            let Some(stem) = name_str.strip_suffix(".pcm") else { continue };
            if stem.ends_with(".pcm") { continue; }

            let parts: Vec<&str> = stem.split('_').collect();
            if parts.len() == 3 {
                let Some(rate) = parts[0].parse::<u32>().ok() else { continue };
                let Some(channels) = parts[1].parse::<u16>().ok() else { continue };
                let quality_key = parts[2];
                if ResamplingQuality::from_cache_key(quality_key).is_none() { continue; }
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                versions.push(CachedVersion {
                    target_rate: rate,
                    channels,
                    quality: quality_key.to_string(),
                    size_bytes: size,
                });
            } else if parts.len() == 2 {
                let Some(rate) = parts[0].parse::<u32>().ok() else { continue };
                let Some(channels) = parts[1].parse::<u16>().ok() else { continue };
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                versions.push(CachedVersion {
                    target_rate: rate,
                    channels,
                    quality: ResamplingQuality::Fast.cache_key().to_string(),
                    size_bytes: size,
                });
            }
        }
        versions.sort_by_key(|v| v.target_rate);
        versions
    }

    pub fn get_samples_for_rate(&self, target_rate: u32) -> Arc<Vec<f32>> {
        if target_rate == 0 || target_rate == self.original_rate {
            return Arc::clone(&self.original_samples);
        }

        {
            let cache = self.resampled_cache.read();
            if let Some(cached) = cache.get(&target_rate) {
                return Arc::clone(cached);
            }
        }

        let quality = ResamplingQuality::default();
        if let Some(cached_samples) = self.load_from_cache(target_rate, quality) {
            eprintln!("[Track] Loaded {}Hz from disk cache ({} samples)", target_rate, cached_samples.len());
            let resampled = Arc::new(cached_samples);
            let mut cache = self.resampled_cache.write();
            cache.insert(target_rate, Arc::clone(&resampled));
            return resampled;
        }

        eprintln!(
            "[Track] Cache miss for {}Hz, resampling from {}Hz ({} samples)",
            target_rate,
            self.original_rate,
            self.original_samples.len()
        );

        let start = std::time::Instant::now();
        match resampler::resample(
            &self.original_samples,
            self.channels,
            self.original_rate,
            target_rate,
        ) {
            Ok(resampled) => {
                let elapsed = start.elapsed();
                eprintln!(
                    "[Track] On-demand resample completed in {:?} ({} -> {} samples)",
                    elapsed,
                    self.original_samples.len(),
                    resampled.len()
                );
                self.save_to_cache(target_rate, quality, &resampled);
                let resampled = Arc::new(resampled);
                let mut cache = self.resampled_cache.write();
                cache.insert(target_rate, Arc::clone(&resampled));
                resampled
            }
            Err(e) => {
                eprintln!(
                    "[Track] On-demand resample failed ({}Hz -> {}Hz): {}, using original samples",
                    self.original_rate,
                    target_rate,
                    e
                );
                Arc::clone(&self.original_samples)
            }
        }
    }

    pub fn resample_to_memory<F>(&self, target_rate: u32, quality: ResamplingQuality, progress_callback: Option<F>) -> Result<(), String>
    where
        F: Fn(u32, u32) -> bool,
    {
        if target_rate == self.original_rate {
            return Ok(());
        }

        {
            let cache = self.resampled_cache.read();
            if cache.contains_key(&target_rate) {
                return Ok(());
            }
        }

        if let Some(cached_samples) = self.load_from_cache(target_rate, quality) {
            let mut cache = self.resampled_cache.write();
            cache.insert(target_rate, Arc::new(cached_samples));
            return Ok(());
        }

        let resampled = resampler::resample_with_options(
            &self.original_samples,
            self.channels,
            self.original_rate,
            target_rate,
            quality,
            progress_callback,
        )?;

        let resampled = Arc::new(resampled);
        let mut cache = self.resampled_cache.write();
        cache.insert(target_rate, Arc::clone(&resampled));

        Ok(())
    }

    pub fn get_resampled_samples(&self, target_rate: u32) -> Option<Arc<Vec<f32>>> {
        let cache = self.resampled_cache.read();
        cache.get(&target_rate).cloned()
    }

    pub fn ensure_resampled(&self, target_rate: u32) -> Result<(), String> {
        self.ensure_resampled_with_options(target_rate, ResamplingQuality::default(), None::<fn(u32, u32) -> bool>)
    }

    pub fn ensure_resampled_with_progress<F>(&self, target_rate: u32, progress_callback: Option<F>) -> Result<(), String>
    where
        F: Fn(u32, u32) -> bool,
    {
        self.ensure_resampled_with_options(target_rate, ResamplingQuality::default(), progress_callback)
    }

    pub fn ensure_resampled_with_options<F>(&self, target_rate: u32, quality: ResamplingQuality, progress_callback: Option<F>) -> Result<(), String>
    where
        F: Fn(u32, u32) -> bool,
    {
        if target_rate == self.original_rate {
            eprintln!("[Resampler] Using original {}Hz samples", target_rate);
            return Ok(());
        }

        {
            let cache = self.resampled_cache.read();
            if cache.contains_key(&target_rate) {
                eprintln!("[Resampler] MEMORY CACHE HIT for {}Hz", target_rate);
                return Ok(());
            }
        }

        if let Some(cached_samples) = self.load_from_cache(target_rate, quality) {
            eprintln!("[Resampler] DISK CACHE HIT for {}Hz ({} samples)", target_rate, cached_samples.len());
            let mut cache = self.resampled_cache.write();
            cache.insert(target_rate, Arc::new(cached_samples));
            return Ok(());
        }

        eprintln!(
            "[Resampler] CACHE MISS {}Hz -> {}Hz ({} samples, {:?})",
            self.original_rate,
            target_rate,
            self.original_samples.len(),
            quality
        );

        let start = std::time::Instant::now();
        let resampled = resampler::resample_with_options(
            &self.original_samples,
            self.channels,
            self.original_rate,
            target_rate,
            quality,
            progress_callback,
        )?;
        let elapsed = start.elapsed();

        eprintln!(
            "[Resampler] Completed in {:?} ({} -> {} samples)",
            elapsed,
            self.original_samples.len(),
            resampled.len()
        );

        let resampled = Arc::new(resampled);
        let mut cache = self.resampled_cache.write();
        cache.insert(target_rate, Arc::clone(&resampled));
        drop(cache);

        self.save_to_cache(target_rate, quality, &resampled);

        Ok(())
    }

    pub fn memory_usage(&self) -> usize {
        let original_bytes = self.original_samples.len() * std::mem::size_of::<f32>();
        let cache = self.resampled_cache.read();
        let cache_bytes: usize = cache
            .values()
            .map(|samples| samples.len() * std::mem::size_of::<f32>())
            .sum();
        original_bytes + cache_bytes
    }

    pub fn cached_rates(&self) -> Vec<u32> {
        let cache = self.resampled_cache.read();
        cache.keys().copied().collect()
    }

    pub fn is_ready_for_rate(&self, target_rate: u32) -> bool {
        if target_rate == 0 || target_rate == self.original_rate {
            return true;
        }
        let cache = self.resampled_cache.read();
        cache.contains_key(&target_rate)
    }
}
