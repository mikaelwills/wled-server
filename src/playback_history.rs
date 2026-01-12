use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};
use uuid::Uuid;

use crate::timing_metrics::MetricsSnapshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackSession {
    pub id: String,
    pub program_id: String,
    pub program_name: String,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub duration_ms: u64,
    pub cue_count: u64,
    pub cues_drifted: u64,
    pub cue_drift_avg_ms: f64,
    pub cue_drift_max_ms: f64,
    pub packets_ok: u64,
    pub packets_wouldblock: u64,
    pub packets_err: u64,
    pub frame_count: u64,
    pub frame_avg_ms: f64,
    pub completed: bool,
}

pub struct PlaybackHistory {
    storage_path: PathBuf,
    current_session: RwLock<Option<PlaybackSession>>,
}

impl PlaybackHistory {
    pub fn new(storage_path: PathBuf) -> Self {
        Self {
            storage_path,
            current_session: RwLock::new(None),
        }
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    pub fn start_session(&self, program_id: &str, program_name: &str) -> String {
        let session_id = Uuid::new_v4().to_string();
        let session = PlaybackSession {
            id: session_id.clone(),
            program_id: program_id.to_string(),
            program_name: program_name.to_string(),
            started_at: Self::now_ms(),
            ended_at: None,
            duration_ms: 0,
            cue_count: 0,
            cues_drifted: 0,
            cue_drift_avg_ms: 0.0,
            cue_drift_max_ms: 0.0,
            packets_ok: 0,
            packets_wouldblock: 0,
            packets_err: 0,
            frame_count: 0,
            frame_avg_ms: 0.0,
            completed: false,
        };

        if let Ok(mut current) = self.current_session.write() {
            *current = Some(session);
        }

        info!(session_id = %session_id, program = %program_name, "Playback session started");
        session_id
    }

    pub fn end_session(&self, session_id: &str, metrics: &MetricsSnapshot, completed: bool) {
        let session = {
            let mut current = match self.current_session.write() {
                Ok(c) => c,
                Err(_) => return,
            };
            if current.as_ref().map(|s| s.id.as_str()) != Some(session_id) {
                return;
            }
            current.take()
        };

        if let Some(mut session) = session {
            let now = Self::now_ms();
            session.ended_at = Some(now);
            session.duration_ms = now.saturating_sub(session.started_at);
            session.cue_count = metrics.cue_count;
            session.cues_drifted = metrics.cues_drifted;
            session.cue_drift_avg_ms = if metrics.cue_count > 0 {
                metrics.cue_drift_total_ms / metrics.cue_count as f64
            } else {
                0.0
            };
            session.cue_drift_max_ms = metrics.cue_drift_max_ms;
            session.packets_ok = metrics.packets_ok;
            session.packets_wouldblock = metrics.packets_wouldblock;
            session.packets_err = metrics.packets_err;
            session.frame_count = metrics.frame_count;
            session.frame_avg_ms = metrics.frame_avg_ms;
            session.completed = completed;

            if let Err(e) = self.save_session(&session) {
                warn!(error = %e, "Failed to save playback session");
            } else {
                info!(
                    session_id = %session.id,
                    program = %session.program_name,
                    duration_ms = session.duration_ms,
                    cue_count = session.cue_count,
                    completed = completed,
                    "Playback session ended"
                );
            }
        }
    }

    pub fn get_current_session(&self) -> Option<PlaybackSession> {
        self.current_session.read().ok()?.clone()
    }

    fn save_session(&self, session: &PlaybackSession) -> Result<(), Box<dyn std::error::Error>> {
        let file_path = self.storage_path.join(format!("{}.json", session.id));
        let json = serde_json::to_string_pretty(session)?;
        fs::write(&file_path, json)?;
        Ok(())
    }

    pub fn get_sessions(&self) -> Vec<PlaybackSession> {
        let mut sessions = Vec::new();

        let entries = match fs::read_dir(&self.storage_path) {
            Ok(e) => e,
            Err(_) => return sessions,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<PlaybackSession>(&content) {
                        sessions.push(session);
                    }
                }
            }
        }

        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        sessions
    }

    pub fn get_session(&self, id: &str) -> Option<PlaybackSession> {
        let file_path = self.storage_path.join(format!("{}.json", id));
        let content = fs::read_to_string(&file_path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn delete_session(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let file_path = self.storage_path.join(format!("{}.json", id));
        fs::remove_file(file_path)?;
        info!(session_id = %id, "Playback session deleted");
        Ok(())
    }

    pub fn clear_all(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let mut count = 0;
        let entries = fs::read_dir(&self.storage_path)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if fs::remove_file(&path).is_ok() {
                    count += 1;
                }
            }
        }

        info!(count = count, "Cleared all playback history");
        Ok(count)
    }
}
