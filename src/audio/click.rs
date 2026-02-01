use std::sync::OnceLock;

static DOWNBEAT_SAMPLES: OnceLock<Vec<f32>> = OnceLock::new();
static REGULAR_SAMPLES: OnceLock<Vec<f32>> = OnceLock::new();

const DOWNBEAT_WAV: &[u8] = include_bytes!("../../data/audio/click_downbeat.wav");
const REGULAR_WAV: &[u8] = include_bytes!("../../data/audio/click_regular.wav");
const SOURCE_SAMPLE_RATE: u32 = 44100;

fn decode_wav_mono_16bit(wav_data: &[u8]) -> Vec<f32> {
    if wav_data.len() < 44 {
        return vec![];
    }
    let data_start = 44;
    let samples_u8 = &wav_data[data_start..];
    let mut samples = Vec::with_capacity(samples_u8.len() / 2);
    for chunk in samples_u8.chunks_exact(2) {
        let sample_i16 = i16::from_le_bytes([chunk[0], chunk[1]]);
        samples.push(sample_i16 as f32 / 32768.0);
    }
    samples
}

fn get_downbeat_samples() -> &'static Vec<f32> {
    DOWNBEAT_SAMPLES.get_or_init(|| decode_wav_mono_16bit(DOWNBEAT_WAV))
}

fn get_regular_samples() -> &'static Vec<f32> {
    REGULAR_SAMPLES.get_or_init(|| decode_wav_mono_16bit(REGULAR_WAV))
}

fn resample_simple(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let new_len = (samples.len() as f64 / ratio) as usize;
    let mut resampled = Vec::with_capacity(new_len);
    for i in 0..new_len {
        let src_idx = i as f64 * ratio;
        let idx0 = src_idx.floor() as usize;
        let idx1 = (idx0 + 1).min(samples.len() - 1);
        let frac = src_idx - idx0 as f64;
        let sample = samples[idx0] as f64 * (1.0 - frac) + samples[idx1] as f64 * frac;
        resampled.push(sample as f32);
    }
    resampled
}

pub fn generate_click_track(
    bpm: f64,
    grid_offset: f64,
    total_duration: f64,
    sample_rate: u32,
    beats_per_bar: u32,
    click_rate: f64,
) -> Vec<f32> {
    let beat_interval = 60.0 / (bpm * click_rate);
    let total_samples = (total_duration * sample_rate as f64) as usize;
    let mut samples = vec![0.0f32; total_samples];

    let downbeat = resample_simple(get_downbeat_samples(), SOURCE_SAMPLE_RATE, sample_rate);
    let regular = resample_simple(get_regular_samples(), SOURCE_SAMPLE_RATE, sample_rate);

    let mut beat_number = 0u32;
    let mut beat_time = grid_offset;

    while beat_time < total_duration {
        let is_downbeat = beat_number % beats_per_bar == 0;
        let click_samples = if is_downbeat { &downbeat } else { &regular };
        let start_sample = (beat_time * sample_rate as f64) as usize;

        for (i, &sample) in click_samples.iter().enumerate() {
            let idx = start_sample + i;
            if idx < total_samples {
                samples[idx] += sample;
            }
        }

        beat_number += 1;
        beat_time = grid_offset + (beat_number as f64 * beat_interval);
    }

    samples
}
