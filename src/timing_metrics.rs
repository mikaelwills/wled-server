use serde::ser::SerializeStruct;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_EVENTS: usize = 100;
const MAX_FRAME_SAMPLES: usize = 100;
const MAX_LABEL_LEN: usize = 48;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum DriftSource {
    Cue,
    Frame,
}

#[derive(Debug, Clone, Copy)]
pub struct DriftEvent {
    pub timestamp: u64,
    pub source: DriftSource,
    pub drift_ms: f64,
    label_buf: [u8; MAX_LABEL_LEN],
    label_len: u8,
}

impl DriftEvent {
    fn new(timestamp: u64, source: DriftSource, drift_ms: f64, label: &str) -> Self {
        let mut label_buf = [0u8; MAX_LABEL_LEN];
        let bytes = label.as_bytes();
        let len = bytes.len().min(MAX_LABEL_LEN);
        label_buf[..len].copy_from_slice(&bytes[..len]);
        Self {
            timestamp,
            source,
            drift_ms,
            label_buf,
            label_len: len as u8,
        }
    }

    pub fn label(&self) -> &str {
        std::str::from_utf8(&self.label_buf[..self.label_len as usize]).unwrap_or("")
    }
}

impl Serialize for DriftEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("DriftEvent", 4)?;
        state.serialize_field("timestamp", &self.timestamp)?;
        state.serialize_field("source", &self.source)?;
        state.serialize_field("drift_ms", &self.drift_ms)?;
        state.serialize_field("label", self.label())?;
        state.end()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub cue_count: u64,
    pub cues_drifted: u64,
    pub cue_drift_total_ms: f64,
    pub cue_drift_max_ms: f64,
    pub frame_count: u64,
    pub frame_avg_ms: f64,
    pub frame_max_ms: f64,
    pub packets_ok: u64,
    pub packets_wouldblock: u64,
    pub packets_err: u64,
    pub recent_events: Vec<DriftEvent>,
    pub drift_threshold_ms: f64,
}

pub struct TimingMetrics {
    cue_count: AtomicU64,
    cues_drifted: AtomicU64,
    cue_drift_total_us: AtomicU64,
    cue_drift_max_us: AtomicU64,

    frame_count: AtomicU64,
    frame_total_us: AtomicU64,
    frame_max_us: AtomicU64,
    frame_samples: Mutex<VecDeque<f64>>,

    packets_ok: AtomicU64,
    packets_wouldblock: AtomicU64,
    packets_err: AtomicU64,

    events: Mutex<VecDeque<DriftEvent>>,
    drift_threshold_ms: AtomicU64,
}

impl TimingMetrics {
    pub fn new() -> Self {
        Self {
            cue_count: AtomicU64::new(0),
            cues_drifted: AtomicU64::new(0),
            cue_drift_total_us: AtomicU64::new(0),
            cue_drift_max_us: AtomicU64::new(0),

            frame_count: AtomicU64::new(0),
            frame_total_us: AtomicU64::new(0),
            frame_max_us: AtomicU64::new(0),
            frame_samples: Mutex::new(VecDeque::with_capacity(MAX_FRAME_SAMPLES)),

            packets_ok: AtomicU64::new(0),
            packets_wouldblock: AtomicU64::new(0),
            packets_err: AtomicU64::new(0),

            events: Mutex::new(VecDeque::with_capacity(MAX_EVENTS)),
            drift_threshold_ms: AtomicU64::new(10_000),
        }
    }

    pub fn set_drift_threshold(&self, threshold_ms: f64) {
        let threshold_us = (threshold_ms * 1000.0) as u64;
        self.drift_threshold_ms.store(threshold_us, Ordering::Relaxed);
    }

    pub fn get_drift_threshold_ms(&self) -> f64 {
        self.drift_threshold_ms.load(Ordering::Relaxed) as f64 / 1000.0
    }

    pub fn record_cue_drift(&self, drift_ms: f64, label: &str) {
        let drift_us = (drift_ms.abs() * 1000.0) as u64;

        self.cue_count.fetch_add(1, Ordering::Relaxed);
        self.cue_drift_total_us.fetch_add(drift_us, Ordering::Relaxed);

        let mut current_max = self.cue_drift_max_us.load(Ordering::Relaxed);
        while drift_us > current_max {
            match self.cue_drift_max_us.compare_exchange_weak(
                current_max,
                drift_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_max = x,
            }
        }

        let threshold_us = self.drift_threshold_ms.load(Ordering::Relaxed);
        if drift_us >= threshold_us {
            self.cues_drifted.fetch_add(1, Ordering::Relaxed);
            self.add_event(DriftSource::Cue, drift_ms, label);
        }
    }

    pub fn record_frame_tick(&self, actual_ms: f64) {
        let actual_us = (actual_ms * 1000.0) as u64;

        self.frame_count.fetch_add(1, Ordering::Relaxed);
        self.frame_total_us.fetch_add(actual_us, Ordering::Relaxed);

        let mut current_max = self.frame_max_us.load(Ordering::Relaxed);
        while actual_us > current_max {
            match self.frame_max_us.compare_exchange_weak(
                current_max,
                actual_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_max = x,
            }
        }
    }

    pub fn record_packet_ok(&self) {
        self.packets_ok.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_packet_wouldblock(&self) {
        self.packets_wouldblock.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_packet_err(&self) {
        self.packets_err.fetch_add(1, Ordering::Relaxed);
    }

    fn add_event(&self, source: DriftSource, drift_ms: f64, label: &str) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let event = DriftEvent::new(timestamp, source, drift_ms, label);

        if let Ok(mut events) = self.events.lock() {
            if events.len() >= MAX_EVENTS {
                events.pop_front();
            }
            events.push_back(event);
        }
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let cue_count = self.cue_count.load(Ordering::Relaxed);
        let cues_drifted = self.cues_drifted.load(Ordering::Relaxed);
        let cue_drift_total_ms = self.cue_drift_total_us.load(Ordering::Relaxed) as f64 / 1000.0;
        let cue_drift_max_ms = self.cue_drift_max_us.load(Ordering::Relaxed) as f64 / 1000.0;

        let frame_count = self.frame_count.load(Ordering::Relaxed);
        let frame_total_us = self.frame_total_us.load(Ordering::Relaxed);
        let frame_avg_ms = if frame_count > 0 {
            (frame_total_us as f64 / frame_count as f64) / 1000.0
        } else {
            0.0
        };
        let frame_max_ms = self.frame_max_us.load(Ordering::Relaxed) as f64 / 1000.0;

        let recent_events = self.events.lock()
            .map(|e| e.iter().cloned().collect())
            .unwrap_or_default();

        MetricsSnapshot {
            cue_count,
            cues_drifted,
            cue_drift_total_ms,
            cue_drift_max_ms,
            frame_count,
            frame_avg_ms,
            frame_max_ms,
            packets_ok: self.packets_ok.load(Ordering::Relaxed),
            packets_wouldblock: self.packets_wouldblock.load(Ordering::Relaxed),
            packets_err: self.packets_err.load(Ordering::Relaxed),
            recent_events,
            drift_threshold_ms: self.get_drift_threshold_ms(),
        }
    }

    pub fn get_recent_events(&self) -> Vec<DriftEvent> {
        self.events.lock()
            .map(|e| e.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn clear_events(&self) {
        if let Ok(mut events) = self.events.lock() {
            events.clear();
        }
    }

    pub fn reset(&self) {
        self.cue_count.store(0, Ordering::Relaxed);
        self.cues_drifted.store(0, Ordering::Relaxed);
        self.cue_drift_total_us.store(0, Ordering::Relaxed);
        self.cue_drift_max_us.store(0, Ordering::Relaxed);
        self.frame_count.store(0, Ordering::Relaxed);
        self.frame_total_us.store(0, Ordering::Relaxed);
        self.frame_max_us.store(0, Ordering::Relaxed);
        self.packets_ok.store(0, Ordering::Relaxed);
        self.packets_wouldblock.store(0, Ordering::Relaxed);
        self.packets_err.store(0, Ordering::Relaxed);
        if let Ok(mut samples) = self.frame_samples.lock() {
            samples.clear();
        }
        self.clear_events();
    }
}

impl Default for TimingMetrics {
    fn default() -> Self {
        Self::new()
    }
}
