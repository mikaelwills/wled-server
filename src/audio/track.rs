pub struct LoadedTrack {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
}

impl LoadedTrack {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        let duration_secs = samples.len() as f64 / (sample_rate as f64 * channels as f64);
        Self {
            samples,
            sample_rate,
            channels,
            duration_secs,
        }
    }
}
