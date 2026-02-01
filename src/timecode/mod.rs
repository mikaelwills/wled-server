mod osc;

pub use osc::OscTimecodeOutput;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TimecodePosition {
    pub samples: u64,
    pub seconds: f64,
    pub sample_rate: u32,
    pub channels: u32,
    pub bpm: Option<f64>,
    pub beat: Option<f64>,
}

impl TimecodePosition {
    pub fn from_samples(samples: u64, sample_rate: u32, channels: u32, bpm: Option<f64>) -> Self {
        let seconds = if sample_rate > 0 && channels > 0 {
            samples as f64 / (sample_rate as f64 * channels as f64)
        } else {
            0.0
        };

        let beat = bpm.map(|b| (seconds * b) / 60.0);

        Self {
            samples,
            seconds,
            sample_rate,
            channels,
            bpm,
            beat,
        }
    }

    pub fn smpte_string(&self, frame_rate: f64) -> String {
        let total_frames = (self.seconds * frame_rate) as u64;
        let frames = total_frames % (frame_rate as u64);
        let total_secs = (total_frames / frame_rate as u64) as u64;
        let secs = total_secs % 60;
        let mins = (total_secs / 60) % 60;
        let hours = total_secs / 3600;
        format!("{:02}:{:02}:{:02}:{:02}", hours, mins, secs, frames)
    }
}

pub trait TimecodeOutput: Send + Sync {
    fn name(&self) -> &str;
    fn start(&mut self) -> Result<(), String>;
    fn stop(&mut self) -> Result<(), String>;
    fn send_position(&mut self, position: &TimecodePosition) -> Result<(), String>;
    fn is_enabled(&self) -> bool;
    fn set_enabled(&mut self, enabled: bool);
}

pub struct TimecodeBroadcaster {
    outputs: Vec<Box<dyn TimecodeOutput>>,
    position_source: Option<Arc<AtomicU64>>,
    sample_rate: u32,
    channels: u32,
    bpm: Option<f64>,
    running: bool,
}

impl TimecodeBroadcaster {
    pub fn new() -> Self {
        Self {
            outputs: Vec::new(),
            position_source: None,
            sample_rate: 44100,
            channels: 2,
            bpm: None,
            running: false,
        }
    }

    pub fn add_output(&mut self, output: Box<dyn TimecodeOutput>) {
        self.outputs.push(output);
    }

    pub fn get_output_status(&self) -> HashMap<String, bool> {
        self.outputs
            .iter()
            .map(|o| (o.name().to_string(), o.is_enabled()))
            .collect()
    }

    pub fn set_output_enabled(&mut self, name: &str, enabled: bool) -> bool {
        for output in &mut self.outputs {
            if output.name() == name {
                output.set_enabled(enabled);
                return true;
            }
        }
        false
    }

    pub fn set_position_source(&mut self, position: Arc<AtomicU64>, sample_rate: u32, channels: u32) {
        self.position_source = Some(position);
        self.sample_rate = sample_rate;
        self.channels = channels;
    }

    pub fn set_bpm(&mut self, bpm: Option<f64>) {
        self.bpm = bpm;
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.position_source.is_none() {
            return Err("No position source configured".to_string());
        }

        for output in &mut self.outputs {
            if output.is_enabled() {
                if let Err(e) = output.start() {
                    eprintln!("[Timecode] Failed to start {}: {}", output.name(), e);
                }
            }
        }

        self.running = true;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;

        for output in &mut self.outputs {
            if let Err(e) = output.stop() {
                eprintln!("[Timecode] Failed to stop {}: {}", output.name(), e);
            }
        }
    }

    pub fn broadcast(&mut self) {
        if !self.running {
            return;
        }

        let samples = match &self.position_source {
            Some(pos) => pos.load(Ordering::Acquire),
            None => return,
        };

        let position = TimecodePosition::from_samples(
            samples,
            self.sample_rate,
            self.channels,
            self.bpm,
        );

        for output in &mut self.outputs {
            if output.is_enabled() {
                if let Err(e) = output.send_position(&position) {
                    eprintln!("[Timecode] {} send failed: {}", output.name(), e);
                }
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn clear_position_source(&mut self) {
        self.position_source = None;
    }

    pub fn current_position(&self) -> Option<TimecodePosition> {
        self.position_source.as_ref().map(|pos| {
            let samples = pos.load(Ordering::Acquire);
            TimecodePosition::from_samples(samples, self.sample_rate, self.channels, self.bpm)
        })
    }
}

impl Default for TimecodeBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}
