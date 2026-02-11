use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing;

use super::resampler::{spawn_resampling, ResamplingJob};
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
    pub volume: f32,
    pub is_stereo: bool,
}

impl Default for SlotRouting {
    fn default() -> Self {
        Self {
            left_channel: 0,
            right_channel: 1,
            muted: false,
            volume: 1.0,
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
                SlotRouting {
                    left_channel: 0,
                    right_channel: 1,
                    muted: false,
                    volume: 1.0,
                    is_stereo: true,
                },
                SlotRouting {
                    left_channel: 2,
                    right_channel: 2,
                    muted: false,
                    volume: 1.0,
                    is_stereo: false,
                },
                SlotRouting {
                    left_channel: 3,
                    right_channel: 3,
                    muted: false,
                    volume: 1.0,
                    is_stereo: false,
                },
                SlotRouting {
                    left_channel: 4,
                    right_channel: 4,
                    muted: false,
                    volume: 1.0,
                    is_stereo: false,
                },
            ],
        }
    }
}

impl RoutingConfig {
    pub fn is_stereo_mode(&self) -> bool {
        self.output_channels <= 2
    }

    pub fn from_device_routing(
        routing: &crate::config::DeviceRouting,
        output_channels: usize,
    ) -> Self {
        Self {
            output_channels,
            slots: [
                SlotRouting {
                    left_channel: (routing.backing_left as usize).saturating_sub(1),
                    right_channel: (routing.backing_right as usize).saturating_sub(1),
                    muted: false,
                    volume: 1.0,
                    is_stereo: true,
                },
                SlotRouting {
                    left_channel: (routing.guide as usize).saturating_sub(1),
                    right_channel: (routing.guide as usize).saturating_sub(1),
                    muted: false,
                    volume: 1.0,
                    is_stereo: false,
                },
                SlotRouting {
                    left_channel: (routing.click as usize).saturating_sub(1),
                    right_channel: (routing.click as usize).saturating_sub(1),
                    muted: false,
                    volume: 1.0,
                    is_stereo: false,
                },
                SlotRouting {
                    left_channel: 4,
                    right_channel: 4,
                    muted: false,
                    volume: 1.0,
                    is_stereo: false,
                },
            ],
        }
    }

    pub fn set_mute(&mut self, slot: SlotId, muted: bool) {
        self.slots[slot as usize].muted = muted;
    }

    pub fn set_volume(&mut self, slot: SlotId, volume: f32) {
        self.slots[slot as usize].volume = volume.clamp(0.0, 2.0);
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
    SetMute {
        slot: SlotId,
        muted: bool,
    },
    SetVolume {
        slot: SlotId,
        volume: f32,
    },
    LoadSlot {
        slot: SlotId,
        track: Arc<LoadedTrack>,
    },
    ClearSlot(SlotId),
}

#[derive(PartialEq)]
struct ClickCacheKey {
    bpm_bits: u64,
    grid_offset_bits: u64,
    click_rate_bits: u64,
    duration_bits: u64,
    sample_rate: u32,
}

pub struct AudioEngine {
    state: PlaybackState,
    slot_tracks: [HashMap<String, Arc<LoadedTrack>>; SLOT_COUNT],
    current_slot_ids: [Option<String>; SLOT_COUNT],
    position: Arc<AtomicU64>,
    health: Arc<PlaybackHealth>,
    resampling_progress: Arc<ResamplingProgress>,
    resampling_cancellation: HashMap<String, Arc<AtomicBool>>,
    device_change_cancellation: Option<Arc<AtomicBool>>,
    command_tx: mpsc::Sender<PlaybackCommand>,
    command_rx: Option<mpsc::Receiver<PlaybackCommand>>,
    device_sample_rate: u32,
    broadcast_tx: Option<Arc<broadcast::Sender<SseEvent>>>,
    resampling_quality: ResamplingQuality,
    click_cache: HashMap<String, ClickCacheKey>,
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
            device_change_cancellation: None,
            command_tx: tx,
            command_rx: Some(rx),
            device_sample_rate: 0,
            broadcast_tx: None,
            resampling_quality: ResamplingQuality::default(),
            click_cache: HashMap::new(),
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
        tracing::info!("Resampling quality set to {:?}", quality);
    }

    pub fn cancel_device_change_resampling(&mut self) {
        if let Some(ref token) = self.device_change_cancellation {
            token.store(true, Ordering::Relaxed);
            tracing::info!("Cancelled previous device-change resampling");
        }
        self.device_change_cancellation = None;
    }

    pub fn create_device_change_cancellation(&mut self) -> Arc<AtomicBool> {
        self.cancel_device_change_resampling();
        let token = Arc::new(AtomicBool::new(false));
        self.device_change_cancellation = Some(Arc::clone(&token));
        token
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

    pub async fn set_device_and_routing(
        &mut self,
        device_id: String,
        sample_rate: u32,
        routing: RoutingConfig,
    ) {
        if sample_rate != self.device_sample_rate {
            self.set_device_sample_rate(sample_rate);
        }
        let _ = self
            .command_tx
            .send(PlaybackCommand::SetDevice(device_id))
            .await;
        let _ = self
            .command_tx
            .send(PlaybackCommand::UpdateRouting(routing))
            .await;
    }

    pub fn set_device_sample_rate(&mut self, rate: u32) {
        if rate != self.device_sample_rate {
            tracing::info!(
                "Device sample rate changed: {}Hz -> {}Hz",
                self.device_sample_rate, rate
            );
            self.device_sample_rate = rate;
        }
    }

    pub fn get_device_sample_rate(&self) -> u32 {
        self.device_sample_rate
    }

    pub async fn load_slot_track(&mut self, slot: SlotId, id: String, track: LoadedTrack) {
        let track = Arc::new(track);
        let slot_name = slot.name();

        tracing::debug!(
            "load_slot_track: slot={}, id={}, original_rate={}, device_rate={}",
            slot_name, id, track.original_rate, self.device_sample_rate
        );

        self.slot_tracks[slot as usize].insert(id.clone(), Arc::clone(&track));

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_key = format!("{}:{}", slot_name, id);
        self.resampling_cancellation
            .insert(cancel_key, Arc::clone(&cancelled));

        spawn_resampling(
            vec![ResamplingJob {
                slot,
                id,
                track,
                cancellation: Some(cancelled),
            }],
            self.device_sample_rate,
            self.resampling_quality,
            self.broadcast_tx.clone(),
        );
    }

    pub async fn load_track(&mut self, id: String, track: LoadedTrack) {
        self.load_slot_track(SlotId::Backing, id, track).await;
    }

    pub async fn load_guide_track(&mut self, id: String, track: LoadedTrack) {
        self.load_slot_track(SlotId::Guide, id, track).await;
    }

    pub async fn generate_and_load_click(
        &mut self,
        program_id: &str,
        bpm: f64,
        grid_offset: f64,
        duration: f64,
        click_rate: f64,
    ) {
        if self.device_sample_rate == 0 {
            tracing::error!("Cannot generate click: no device sample rate set");
            return;
        }

        let click_id = format!("{}_click", program_id);
        let new_key = ClickCacheKey {
            bpm_bits: bpm.to_bits(),
            grid_offset_bits: grid_offset.to_bits(),
            click_rate_bits: click_rate.to_bits(),
            duration_bits: duration.to_bits(),
            sample_rate: self.device_sample_rate,
        };

        if self.click_cache.get(&click_id) == Some(&new_key)
            && self.slot_tracks[SlotId::Click as usize].contains_key(&click_id)
        {
            tracing::debug!(
                "Click track cache hit for {} ({}bpm x{})",
                program_id, bpm, click_rate
            );
            return;
        }

        let samples = super::click::generate_click_track(
            bpm,
            grid_offset,
            duration,
            self.device_sample_rate,
            4,
            click_rate,
        );

        let track = LoadedTrack::new(samples, self.device_sample_rate, 1);
        self.click_cache.insert(click_id.clone(), new_key);
        self.load_slot_track(SlotId::Click, click_id, track).await;
        tracing::info!(
            "Generated click track for {} at {}bpm x{} ({}s)",
            program_id, bpm, click_rate, duration
        );
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

    pub fn get_all_tracks(&self) -> Vec<Arc<LoadedTrack>> {
        self.slot_tracks
            .iter()
            .flat_map(|slot| slot.values().cloned())
            .collect()
    }

    pub fn get_all_tracks_with_ids(&self) -> Vec<(SlotId, String, Arc<LoadedTrack>)> {
        SlotId::all()
            .iter()
            .flat_map(|slot| {
                let slot_idx = *slot as usize;
                self.slot_tracks[slot_idx]
                    .iter()
                    .map(move |(id, track)| (*slot, id.clone(), Arc::clone(track)))
            })
            .collect()
    }

    pub fn get_track_readiness(&self) -> Vec<(String, bool)> {
        self.slot_tracks[SlotId::Backing as usize]
            .iter()
            .map(|(id, track)| (id.clone(), track.is_ready_for_rate(self.device_sample_rate)))
            .collect()
    }

    pub fn loaded_track_ids(&self) -> Vec<String> {
        self.slot_tracks[SlotId::Backing as usize]
            .keys()
            .cloned()
            .collect()
    }

    pub fn unload_slot_track(&mut self, slot: SlotId, id: &str) -> bool {
        let slot_name = slot.name();
        let cancel_key = format!("{}:{}", slot_name, id);
        if let Some(cancelled) = self.resampling_cancellation.remove(&cancel_key) {
            cancelled.store(true, Ordering::Relaxed);
            tracing::info!("Cancelled resampling for '{}'", cancel_key);
        }
        if slot == SlotId::Click {
            self.click_cache.remove(id);
        }
        if let Some(track) = self.slot_tracks[slot as usize].remove(id) {
            let memory_mb = track.memory_usage() as f64 / 1024.0 / 1024.0;
            tracing::info!(
                "Unloaded {} track '{}' - freed {:.2} MB",
                slot_name, id, memory_mb
            );
            let (count, total) = self.memory_usage();
            tracing::debug!(
                "Remaining: {} tracks, {:.2} MB total",
                count,
                total as f64 / 1024.0 / 1024.0
            );
            true
        } else {
            tracing::warn!("{} track '{}' not found", slot_name, id);
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
            total_bytes += slot_tracks
                .values()
                .map(|t| t.memory_usage())
                .sum::<usize>();
        }
        (total_tracks, total_bytes)
    }

    pub async fn play(&mut self, track_id: &str, start_sample: Option<u64>) -> bool {
        self.play_with_guide(track_id, None, start_sample).await
    }

    pub async fn play_with_guide(
        &mut self,
        track_id: &str,
        guide_id: Option<&str>,
        start_sample: Option<u64>,
    ) -> bool {
        let backing_tracks = &self.slot_tracks[SlotId::Backing as usize];
        if let Some(track) = backing_tracks.get(track_id).cloned() {
            let start = start_sample.unwrap_or(0);
            self.position.store(start, Ordering::SeqCst);

            if let Some(gid) = guide_id {
                let guide_tracks = &self.slot_tracks[SlotId::Guide as usize];
                if let Some(guide_track) = guide_tracks.get(gid).cloned() {
                    let _ = self
                        .command_tx
                        .send(PlaybackCommand::LoadSlot {
                            slot: SlotId::Guide,
                            track: guide_track,
                        })
                        .await;
                    self.current_slot_ids[SlotId::Guide as usize] = Some(gid.to_string());
                    tracing::debug!("Loaded guide track '{}' for playback", gid);
                } else {
                    tracing::warn!("Guide track '{}' not found", gid);
                }
            } else {
                let _ = self
                    .command_tx
                    .send(PlaybackCommand::ClearSlot(SlotId::Guide))
                    .await;
                self.current_slot_ids[SlotId::Guide as usize] = None;
            }

            let click_id = format!("{}_click", track_id);
            let click_tracks = &self.slot_tracks[SlotId::Click as usize];
            if let Some(click_track) = click_tracks.get(&click_id).cloned() {
                let _ = self
                    .command_tx
                    .send(PlaybackCommand::LoadSlot {
                        slot: SlotId::Click,
                        track: click_track,
                    })
                    .await;
                self.current_slot_ids[SlotId::Click as usize] = Some(click_id.clone());
                tracing::debug!("Loaded click track for playback");
            } else {
                let _ = self
                    .command_tx
                    .send(PlaybackCommand::ClearSlot(SlotId::Click))
                    .await;
                self.current_slot_ids[SlotId::Click as usize] = None;
            }

            if self
                .command_tx
                .send(PlaybackCommand::Play(track))
                .await
                .is_ok()
            {
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

   
    pub async fn update_routing(&self, routing: RoutingConfig) {
        let _ = self
            .command_tx
            .send(PlaybackCommand::UpdateRouting(routing))
            .await;
    }

    pub async fn set_mute(&self, slot: SlotId, muted: bool) {
        let _ = self
            .command_tx
            .send(PlaybackCommand::SetMute { slot, muted })
            .await;
    }

    pub async fn set_volume(&self, slot: SlotId, volume: f32) {
        let _ = self
            .command_tx
            .send(PlaybackCommand::SetVolume { slot, volume })
            .await;
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
