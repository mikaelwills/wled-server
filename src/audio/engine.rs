use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

use super::{LoadedTrack, PlaybackHealth, ResamplingProgress};
use crate::config::ResamplingQuality;
use crate::sse::SseEvent;

pub const SLOT_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum SlotId {
    Backing = 0,
    Guide = 1,
    Click = 2,
    Aux = 3,
}

impl SlotId {
    pub fn all() -> &'static [SlotId] {
        &[SlotId::Backing, SlotId::Guide, SlotId::Click, SlotId::Aux]
    }

    pub fn name(&self) -> &'static str {
        match self {
            SlotId::Backing => "backing",
            SlotId::Guide => "guide",
            SlotId::Click => "click",
            SlotId::Aux => "aux",
        }
    }
}

#[derive(Clone, Copy)]
pub struct SlotRouting {
    pub left_channel: usize,
    pub right_channel: usize,
    pub muted: bool,
    pub is_stereo: bool,
}

impl Default for SlotRouting {
    fn default() -> Self {
        Self {
            left_channel: 0,
            right_channel: 1,
            muted: false,
            is_stereo: true,
        }
    }
}

#[derive(Clone, Copy)]
pub struct RoutingConfig {
    pub output_channels: usize,
    pub slots: [SlotRouting; SLOT_COUNT],
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            output_channels: 2,
            slots: [
                SlotRouting { left_channel: 0, right_channel: 1, muted: false, is_stereo: true },
                SlotRouting { left_channel: 2, right_channel: 2, muted: false, is_stereo: false },
                SlotRouting { left_channel: 3, right_channel: 3, muted: false, is_stereo: false },
                SlotRouting { left_channel: 4, right_channel: 4, muted: false, is_stereo: false },
            ],
        }
    }
}

impl RoutingConfig {
    pub fn is_stereo_mode(&self) -> bool {
        self.output_channels <= 2
    }

    pub fn from_device_routing(routing: &crate::config::DeviceRouting, output_channels: usize) -> Self {
        Self {
            output_channels,
            slots: [
                SlotRouting {
                    left_channel: (routing.backing_left as usize).saturating_sub(1),
                    right_channel: (routing.backing_right as usize).saturating_sub(1),
                    muted: false,
                    is_stereo: true,
                },
                SlotRouting {
                    left_channel: (routing.guide as usize).saturating_sub(1),
                    right_channel: (routing.guide as usize).saturating_sub(1),
                    muted: false,
                    is_stereo: false,
                },
                SlotRouting {
                    left_channel: (routing.click as usize).saturating_sub(1),
                    right_channel: (routing.click as usize).saturating_sub(1),
                    muted: false,
                    is_stereo: false,
                },
                SlotRouting {
                    left_channel: 4,
                    right_channel: 4,
                    muted: false,
                    is_stereo: false,
                },
            ],
        }
    }

    pub fn set_mute(&mut self, slot: SlotId, muted: bool) {
        self.slots[slot as usize].muted = muted;
    }
}

pub enum PlaybackCommand {
    Play(Arc<LoadedTrack>),
    Stop,
    Pause,
    Resume,
    Seek(u64),
    SetDevice(String),
    UpdateRouting(RoutingConfig),
    SetMute { slot: SlotId, muted: bool },
    LoadSlot { slot: SlotId, track: Arc<LoadedTrack> },
    ClearSlot(SlotId),
}

pub struct AudioEngine {
    state: PlaybackState,
    slot_tracks: [HashMap<String, Arc<LoadedTrack>>; SLOT_COUNT],
    current_slot_ids: [Option<String>; SLOT_COUNT],
    position: Arc<AtomicU64>,
    health: Arc<PlaybackHealth>,
    resampling_progress: Arc<ResamplingProgress>,
    resampling_cancellation: HashMap<String, Arc<AtomicBool>>,
    command_tx: mpsc::Sender<PlaybackCommand>,
    command_rx: Option<mpsc::Receiver<PlaybackCommand>>,
    device_sample_rate: u32,
    broadcast_tx: Option<Arc<broadcast::Sender<SseEvent>>>,
    resampling_quality: ResamplingQuality,
}

impl AudioEngine {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(32);
        Self {
            state: PlaybackState::Stopped,
            slot_tracks: Default::default(),
            current_slot_ids: Default::default(),
            position: Arc::new(AtomicU64::new(0)),
            health: PlaybackHealth::new(),
            resampling_progress: ResamplingProgress::new(),
            resampling_cancellation: HashMap::new(),
            command_tx: tx,
            command_rx: Some(rx),
            device_sample_rate: 0,
            broadcast_tx: None,
            resampling_quality: ResamplingQuality::default(),
        }
    }

    pub fn set_broadcast_tx(&mut self, tx: Arc<broadcast::Sender<SseEvent>>) {
        self.broadcast_tx = Some(tx);
    }

    pub fn get_resampling_quality(&self) -> ResamplingQuality {
        self.resampling_quality
    }

    pub fn set_resampling_quality(&mut self, quality: ResamplingQuality) {
        self.resampling_quality = quality;
        eprintln!("[AudioEngine] Resampling quality set to {:?}", quality);
    }

    fn broadcast_resampling_progress(&self, current: u32, total: u32, active: bool, track_name: &str, from_rate: u32, to_rate: u32) {
        if let Some(ref tx) = self.broadcast_tx {
            let _ = tx.send(SseEvent::ResamplingProgress {
                current,
                total,
                active,
                track_name: track_name.to_string(),
                from_rate,
                to_rate,
            });
        }
    }

    pub fn get_resampling_progress(&self) -> Arc<ResamplingProgress> {
        self.resampling_progress.clone()
    }

    pub fn take_receiver(&mut self) -> Option<mpsc::Receiver<PlaybackCommand>> {
        self.command_rx.take()
    }

    pub fn get_position_arc(&self) -> Arc<AtomicU64> {
        self.position.clone()
    }

    pub fn get_health_arc(&self) -> Arc<PlaybackHealth> {
        self.health.clone()
    }

    pub fn get_health_stats(&self) -> HealthStats {
        HealthStats {
            callback_count: self.health.callback_count.load(Ordering::Relaxed),
            underrun_count: self.health.underrun_count.load(Ordering::Relaxed),
            buffer_size: self.health.last_buffer_size.load(Ordering::Relaxed),
            samples_delivered: self.health.samples_delivered.load(Ordering::Relaxed),
            silence_frames: self.health.silence_frames.load(Ordering::Relaxed),
            max_callback_interval_us: self.health.max_callback_interval_us.load(Ordering::Relaxed),
            late_callbacks: self.health.late_callbacks.load(Ordering::Relaxed),
        }
    }

    pub fn reset_health_stats(&self) {
        self.health.reset_all();
    }

    pub fn set_device_sample_rate(&mut self, rate: u32) {
        if rate != self.device_sample_rate {
            eprintln!("[AudioEngine] Device sample rate changed: {}Hz -> {}Hz", self.device_sample_rate, rate);
            self.device_sample_rate = rate;
        }
    }

    pub fn get_device_sample_rate(&self) -> u32 {
        self.device_sample_rate
    }

    pub async fn load_slot_track(&mut self, slot: SlotId, id: String, track: LoadedTrack) {
        let track = Arc::new(track);
        let device_rate = self.device_sample_rate;
        let original_rate = track.original_rate;
        let quality = self.resampling_quality;
        let slot_name = slot.name();

        self.slot_tracks[slot as usize].insert(id.clone(), Arc::clone(&track));

        if device_rate > 0 && original_rate != device_rate {
            let cancelled = Arc::new(AtomicBool::new(false));
            let cancel_key = format!("{}:{}", slot_name, id);
            self.resampling_cancellation.insert(cancel_key.clone(), Arc::clone(&cancelled));

            let track_clone = Arc::clone(&track);
            let id_clone = id.clone();
            let track_name = format!("{}:{}", slot_name, id);
            let broadcast_tx = self.broadcast_tx.clone();

            tokio::spawn(async move {
                let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<(u32, u32)>(100);

                let progress_forwarder = tokio::spawn(async move {
                    if let Some(tx) = broadcast_tx {
                        let track_name_for_completion = track_name.clone();
                        while let Some((current, total)) = progress_rx.recv().await {
                            let _ = tx.send(SseEvent::ResamplingProgress {
                                current,
                                total,
                                active: true,
                                track_name: track_name.clone(),
                                from_rate: original_rate,
                                to_rate: device_rate,
                            });
                        }
                        let _ = tx.send(SseEvent::ResamplingProgress {
                            current: 0,
                            total: 0,
                            active: false,
                            track_name: track_name_for_completion,
                            from_rate: 0,
                            to_rate: 0,
                        });
                    }
                });

                let cancelled_clone = Arc::clone(&cancelled);
                let result = tokio::task::spawn_blocking(move || {
                    let callback = move |current: u32, total: u32| -> bool {
                        let _ = progress_tx.blocking_send((current, total));
                        !cancelled_clone.load(Ordering::Relaxed)
                    };
                    track_clone.ensure_resampled_with_options(device_rate, quality, Some(callback))
                }).await;

                let _ = progress_forwarder.await;

                match result {
                    Ok(Ok(())) => eprintln!("[AudioEngine] Resampling complete for '{}'", id_clone),
                    Ok(Err(e)) if e.contains("cancelled") => eprintln!("[AudioEngine] Resampling cancelled for '{}'", id_clone),
                    Ok(Err(e)) => eprintln!("[AudioEngine] Failed to resample '{}': {}", id_clone, e),
                    Err(e) => eprintln!("[AudioEngine] Resample task failed for '{}': {}", id_clone, e),
                }
            });
        }
    }

    pub async fn load_track(&mut self, id: String, track: LoadedTrack) {
        self.load_slot_track(SlotId::Backing, id, track).await;
    }

    pub async fn load_guide_track(&mut self, id: String, track: LoadedTrack) {
        self.load_slot_track(SlotId::Guide, id, track).await;
    }

    pub async fn resample_all_tracks(&self, new_rate: u32) -> Result<(), String> {
        if new_rate == 0 {
            return Ok(());
        }

        let mut tracks_to_resample: Vec<(String, Arc<LoadedTrack>)> = Vec::new();
        for slot_tracks in &self.slot_tracks {
            for (id, track) in slot_tracks.iter() {
                if track.original_rate != new_rate {
                    tracks_to_resample.push((id.clone(), Arc::clone(track)));
                }
            }
        }

        if tracks_to_resample.is_empty() {
            return Ok(());
        }

        let total = tracks_to_resample.len() as u32;
        self.resampling_progress.start(total);
        self.broadcast_resampling_progress(0, total, true, "Multiple tracks", 0, new_rate);

        let mut errors = Vec::new();
        for (id, track) in tracks_to_resample {
            let from_rate = track.original_rate;
            let track_name = id.clone();
            let result = tokio::task::spawn_blocking(move || {
                track.ensure_resampled(new_rate)
            })
            .await
            .map_err(|e| format!("Task failed: {}", e))?;

            self.resampling_progress.increment();
            let (current, _) = self.resampling_progress.get();
            self.broadcast_resampling_progress(current, total, true, &track_name, from_rate, new_rate);

            if let Err(e) = result {
                errors.push(format!("{}: {}", id, e));
            }
        }

        self.resampling_progress.finish();
        self.broadcast_resampling_progress(total, total, false, "", 0, 0);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!("Failed to resample some tracks: {}", errors.join(", ")))
        }
    }

    pub fn get_slot_track(&self, slot: SlotId, id: &str) -> Option<Arc<LoadedTrack>> {
        self.slot_tracks[slot as usize].get(id).cloned()
    }

    pub fn get_track(&self, id: &str) -> Option<Arc<LoadedTrack>> {
        self.get_slot_track(SlotId::Backing, id)
    }

    pub fn get_guide_track(&self, id: &str) -> Option<Arc<LoadedTrack>> {
        self.get_slot_track(SlotId::Guide, id)
    }

    pub fn get_track_readiness(&self) -> Vec<(String, bool)> {
        self.slot_tracks[SlotId::Backing as usize]
            .iter()
            .map(|(id, track)| (id.clone(), track.is_ready_for_rate(self.device_sample_rate)))
            .collect()
    }

    pub fn loaded_track_ids(&self) -> Vec<String> {
        self.slot_tracks[SlotId::Backing as usize].keys().cloned().collect()
    }

    pub fn unload_slot_track(&mut self, slot: SlotId, id: &str) -> bool {
        let slot_name = slot.name();
        let cancel_key = format!("{}:{}", slot_name, id);
        if let Some(cancelled) = self.resampling_cancellation.remove(&cancel_key) {
            cancelled.store(true, Ordering::Relaxed);
            eprintln!("[AudioEngine] Cancelled resampling for '{}'", cancel_key);
        }
        if let Some(track) = self.slot_tracks[slot as usize].remove(id) {
            let memory_mb = track.memory_usage() as f64 / 1024.0 / 1024.0;
            eprintln!("[AudioEngine] Unloaded {} track '{}' - freed {:.2} MB", slot_name, id, memory_mb);
            let (count, total) = self.memory_usage();
            eprintln!("[AudioEngine] Remaining: {} tracks, {:.2} MB total", count, total as f64 / 1024.0 / 1024.0);
            true
        } else {
            eprintln!("[AudioEngine] {} track '{}' not found", slot_name, id);
            false
        }
    }

    pub fn unload_track(&mut self, id: &str) -> bool {
        self.unload_slot_track(SlotId::Backing, id)
    }

    pub fn unload_guide_track(&mut self, id: &str) -> bool {
        self.unload_slot_track(SlotId::Guide, id)
    }

    pub async fn clear_slot(&self, slot: SlotId) {
        let _ = self.command_tx.send(PlaybackCommand::ClearSlot(slot)).await;
    }

    pub fn memory_usage(&self) -> (usize, usize) {
        let mut total_tracks = 0;
        let mut total_bytes = 0;
        for slot_tracks in &self.slot_tracks {
            total_tracks += slot_tracks.len();
            total_bytes += slot_tracks.values().map(|t| t.memory_usage()).sum::<usize>();
        }
        (total_tracks, total_bytes)
    }

    pub async fn play(&mut self, track_id: &str, start_sample: Option<u64>) -> bool {
        self.play_with_guide(track_id, None, start_sample).await
    }

    pub async fn play_with_guide(&mut self, track_id: &str, guide_id: Option<&str>, start_sample: Option<u64>) -> bool {
        let backing_tracks = &self.slot_tracks[SlotId::Backing as usize];
        if let Some(track) = backing_tracks.get(track_id).cloned() {
            let start = start_sample.unwrap_or(0);
            self.position.store(start, Ordering::SeqCst);

            if let Some(gid) = guide_id {
                let guide_tracks = &self.slot_tracks[SlotId::Guide as usize];
                if let Some(guide_track) = guide_tracks.get(gid).cloned() {
                    let _ = self.command_tx.send(PlaybackCommand::LoadSlot {
                        slot: SlotId::Guide,
                        track: guide_track
                    }).await;
                    self.current_slot_ids[SlotId::Guide as usize] = Some(gid.to_string());
                    eprintln!("[AudioEngine] Loaded guide track '{}' for playback", gid);
                } else {
                    eprintln!("[AudioEngine] Guide track '{}' not found", gid);
                }
            } else {
                let _ = self.command_tx.send(PlaybackCommand::ClearSlot(SlotId::Guide)).await;
                self.current_slot_ids[SlotId::Guide as usize] = None;
            }

            if self.command_tx.send(PlaybackCommand::Play(track)).await.is_ok() {
                if start > 0 {
                    let _ = self.command_tx.send(PlaybackCommand::Seek(start)).await;
                }
                self.state = PlaybackState::Playing;
                self.current_slot_ids[SlotId::Backing as usize] = Some(track_id.to_string());
                return true;
            }
        }
        false
    }

    pub async fn stop(&mut self) {
        let _ = self.command_tx.send(PlaybackCommand::Stop).await;
        self.state = PlaybackState::Stopped;
        self.current_slot_ids = Default::default();
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

    pub async fn set_device(&mut self, device_id: String, new_sample_rate: u32) {
        if new_sample_rate != self.device_sample_rate {
            self.set_device_sample_rate(new_sample_rate);
            if let Err(e) = self.resample_all_tracks(new_sample_rate).await {
                eprintln!("[AudioEngine] Warning: {}", e);
            }
        }
        let _ = self.command_tx.send(PlaybackCommand::SetDevice(device_id)).await;
    }

    pub async fn update_routing(&self, routing: RoutingConfig) {
        let _ = self.command_tx.send(PlaybackCommand::UpdateRouting(routing)).await;
    }

    pub async fn set_mute(&self, slot: SlotId, muted: bool) {
        let _ = self.command_tx.send(PlaybackCommand::SetMute { slot, muted }).await;
    }

    pub fn get_state(&self) -> PlaybackState {
        self.state
    }

    pub fn get_position(&self) -> u64 {
        self.position.load(Ordering::SeqCst)
    }

    pub fn get_current_track_id(&self) -> Option<&str> {
        self.current_slot_ids[SlotId::Backing as usize].as_deref()
    }

    pub fn get_current_track(&self) -> Option<Arc<LoadedTrack>> {
        self.current_slot_ids[SlotId::Backing as usize]
            .as_ref()
            .and_then(|id| self.slot_tracks[SlotId::Backing as usize].get(id).cloned())
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct TrackMemoryInfo {
    pub id: String,
    pub original_rate: u32,
    pub cached_rates: Vec<u32>,
    pub total_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct HealthStats {
    pub callback_count: u64,
    pub underrun_count: u64,
    pub buffer_size: u32,
    pub samples_delivered: u64,
    pub silence_frames: u64,
    pub max_callback_interval_us: u64,
    pub late_callbacks: u64,
}
