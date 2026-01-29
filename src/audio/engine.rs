use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::LoadedTrack;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

pub enum PlaybackCommand {
    Play(Arc<LoadedTrack>),
    Stop,
    Pause,
    Resume,
    Seek(u64),
    SetDevice(String),
}

pub struct AudioEngine {
    state: PlaybackState,
    tracks: HashMap<String, Arc<LoadedTrack>>,
    current_track_id: Option<String>,
    position: Arc<AtomicU64>,
    command_tx: mpsc::Sender<PlaybackCommand>,
    command_rx: Option<mpsc::Receiver<PlaybackCommand>>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(32);
        Self {
            state: PlaybackState::Stopped,
            tracks: HashMap::new(),
            current_track_id: None,
            position: Arc::new(AtomicU64::new(0)),
            command_tx: tx,
            command_rx: Some(rx),
        }
    }

    pub fn take_receiver(&mut self) -> Option<mpsc::Receiver<PlaybackCommand>> {
        self.command_rx.take()
    }

    pub fn get_position_arc(&self) -> Arc<AtomicU64> {
        self.position.clone()
    }

    pub fn load_track(&mut self, id: String, track: LoadedTrack) {
        self.tracks.insert(id, Arc::new(track));
    }

    pub fn get_track(&self, id: &str) -> Option<Arc<LoadedTrack>> {
        self.tracks.get(id).cloned()
    }

    pub fn loaded_track_ids(&self) -> Vec<String> {
        self.tracks.keys().cloned().collect()
    }

    pub fn unload_track(&mut self, id: &str) -> bool {
        self.tracks.remove(id).is_some()
    }

    pub fn memory_usage(&self) -> (usize, usize) {
        let total_bytes: usize = self
            .tracks
            .values()
            .map(|t| t.samples.len() * std::mem::size_of::<f32>())
            .sum();
        (self.tracks.len(), total_bytes)
    }

    pub async fn play(&mut self, track_id: &str, start_sample: Option<u64>) -> bool {
        if let Some(track) = self.tracks.get(track_id).cloned() {
            let start = start_sample.unwrap_or(0);
            self.position.store(start, Ordering::SeqCst);
            if self.command_tx.send(PlaybackCommand::Play(track)).await.is_ok() {
                if start > 0 {
                    let _ = self.command_tx.send(PlaybackCommand::Seek(start)).await;
                }
                self.state = PlaybackState::Playing;
                self.current_track_id = Some(track_id.to_string());
                return true;
            }
        }
        false
    }

    pub async fn play_track(&mut self, track_id: &str) -> bool {
        self.play(track_id, None).await
    }

    pub async fn stop(&mut self) {
        let _ = self.command_tx.send(PlaybackCommand::Stop).await;
        self.state = PlaybackState::Stopped;
        self.current_track_id = None;
        self.position.store(0, Ordering::SeqCst);
    }

    pub async fn pause(&mut self) {
        if self.state == PlaybackState::Playing {
            let _ = self.command_tx.send(PlaybackCommand::Pause).await;
            self.state = PlaybackState::Paused;
        }
    }

    pub async fn resume(&mut self) {
        if self.state == PlaybackState::Paused {
            let _ = self.command_tx.send(PlaybackCommand::Resume).await;
            self.state = PlaybackState::Playing;
        }
    }

    pub async fn seek(&self, position: u64) {
        let _ = self.command_tx.send(PlaybackCommand::Seek(position)).await;
    }

    pub async fn set_device(&self, device_id: String) {
        let _ = self.command_tx.send(PlaybackCommand::SetDevice(device_id)).await;
    }

    pub fn get_state(&self) -> PlaybackState {
        self.state
    }

    pub fn get_position(&self) -> u64 {
        self.position.load(Ordering::SeqCst)
    }

    pub fn get_current_track_id(&self) -> Option<&str> {
        self.current_track_id.as_deref()
    }

    pub fn get_current_track(&self) -> Option<Arc<LoadedTrack>> {
        self.current_track_id.as_ref().and_then(|id| self.tracks.get(id).cloned())
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}
