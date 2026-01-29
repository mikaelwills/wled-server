use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use super::resampler;

pub struct LoadedTrack {
    pub original_samples: Arc<Vec<f32>>,
    pub original_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
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
            resampled_cache: RwLock::new(HashMap::new()),
        }
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

    pub fn ensure_resampled(&self, target_rate: u32) -> Result<(), String> {
        if target_rate == self.original_rate {
            return Ok(());
        }

        {
            let cache = self.resampled_cache.read();
            if cache.contains_key(&target_rate) {
                return Ok(());
            }
        }

        eprintln!(
            "[Resampler] Resampling from {}Hz to {}Hz ({} samples, {} channels)",
            self.original_rate,
            target_rate,
            self.original_samples.len(),
            self.channels
        );

        let start = std::time::Instant::now();
        let resampled = resampler::resample(
            &self.original_samples,
            self.channels,
            self.original_rate,
            target_rate,
        )?;
        let elapsed = start.elapsed();

        eprintln!(
            "[Resampler] Completed in {:?} ({} -> {} samples)",
            elapsed,
            self.original_samples.len(),
            resampled.len()
        );

        let mut cache = self.resampled_cache.write();
        cache.insert(target_rate, Arc::new(resampled));
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
}
