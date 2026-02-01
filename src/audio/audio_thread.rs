use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use arc_swap::{ArcSwap, ArcSwapOption};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use tokio::sync::mpsc;

use super::{DeviceManager, LoadedTrack, PlaybackCommand, RoutingConfig, SlotId, SLOT_COUNT};

const STREAM_SWITCH_DELAY_MS: u64 = 50;

pub struct PlaybackHealth {
    pub callback_count: AtomicU64,
    pub underrun_count: AtomicU64,
    pub last_buffer_size: AtomicU32,
    pub samples_delivered: AtomicU64,
    pub silence_frames: AtomicU64,
    pub last_callback_us: AtomicU64,
    pub max_callback_interval_us: AtomicU64,
    pub late_callbacks: AtomicU64,
    start_time: std::time::Instant,
}

impl PlaybackHealth {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            callback_count: AtomicU64::new(0),
            underrun_count: AtomicU64::new(0),
            last_buffer_size: AtomicU32::new(0),
            samples_delivered: AtomicU64::new(0),
            silence_frames: AtomicU64::new(0),
            last_callback_us: AtomicU64::new(0),
            max_callback_interval_us: AtomicU64::new(0),
            late_callbacks: AtomicU64::new(0),
            start_time: std::time::Instant::now(),
        })
    }

    pub fn now_us(&self) -> u64 {
        self.start_time.elapsed().as_micros() as u64
    }

    pub fn reset_all(&self) {
        self.callback_count.store(0, Ordering::Relaxed);
        self.underrun_count.store(0, Ordering::Relaxed);
        self.last_buffer_size.store(0, Ordering::Relaxed);
        self.samples_delivered.store(0, Ordering::Relaxed);
        self.silence_frames.store(0, Ordering::Relaxed);
        self.last_callback_us.store(0, Ordering::Relaxed);
        self.max_callback_interval_us.store(0, Ordering::Relaxed);
        self.late_callbacks.store(0, Ordering::Relaxed);
    }
}

pub struct TrackSlot {
    pub samples: ArcSwapOption<Vec<f32>>,
    pub channels: AtomicUsize,
    pub sample_count: AtomicUsize,
}

impl TrackSlot {
    pub fn new() -> Self {
        Self {
            samples: ArcSwapOption::new(None),
            channels: AtomicUsize::new(2),
            sample_count: AtomicUsize::new(0),
        }
    }

    pub fn load(&self, samples: Arc<Vec<f32>>, channels: usize) {
        let sample_len = samples.len();
        self.channels.store(channels, Ordering::Relaxed);
        self.samples.store(Some(samples));
        std::sync::atomic::fence(Ordering::Release);
        self.sample_count.store(sample_len, Ordering::Release);
    }

    pub fn clear(&self) {
        self.sample_count.store(0, Ordering::Release);
        self.samples.store(None);
    }
}

impl Default for TrackSlot {
    fn default() -> Self {
        Self::new()
    }
}

pub struct InternalPlaybackState {
    pub playing: AtomicBool,
    pub sample_index: AtomicUsize,
    pub device_sample_rate: AtomicU32,
    pub routing: ArcSwap<RoutingConfig>,
    pub slots: [TrackSlot; SLOT_COUNT],
}

impl InternalPlaybackState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            playing: AtomicBool::new(false),
            sample_index: AtomicUsize::new(0),
            device_sample_rate: AtomicU32::new(0),
            routing: ArcSwap::from_pointee(RoutingConfig::default()),
            slots: Default::default(),
        })
    }

    pub fn update_routing(&self, routing: RoutingConfig) {
        self.routing.store(Arc::new(routing));
    }

    pub fn set_mute(&self, slot: SlotId, muted: bool) {
        let mut current = (**self.routing.load()).clone();
        current.set_mute(slot, muted);
        self.routing.store(Arc::new(current));
    }

    pub fn load_slot(&self, slot: SlotId, track: Arc<LoadedTrack>) {
        let device_rate = self.device_sample_rate.load(Ordering::Acquire);
        let samples = track.get_samples_for_rate(device_rate);
        self.slots[slot as usize].load(samples, track.channels as usize);
    }

    pub fn clear_slot(&self, slot: SlotId) {
        self.slots[slot as usize].clear();
    }

    pub fn clear_all_slots(&self) {
        for slot in &self.slots {
            slot.clear();
        }
    }

    pub fn set_device_sample_rate(&self, rate: u32) {
        self.device_sample_rate.store(rate, Ordering::Release);
    }
}

const THREAD_JOIN_TIMEOUT_MS: u64 = 200;

pub struct AudioThread {
    thread_handle: Option<JoinHandle<()>>,
}

impl Drop for AudioThread {
    fn drop(&mut self) {
        if let Some(handle) = self.thread_handle.take() {
            let start = std::time::Instant::now();
            loop {
                if handle.is_finished() {
                    if let Err(e) = handle.join() {
                        eprintln!("[AudioThread] Thread panicked during shutdown: {:?}", e);
                    }
                    eprintln!("[AudioThread] Thread joined successfully");
                    break;
                }
                if start.elapsed() > Duration::from_millis(THREAD_JOIN_TIMEOUT_MS) {
                    eprintln!(
                        "[AudioThread] Warning: Thread join timed out after {}ms",
                        THREAD_JOIN_TIMEOUT_MS
                    );
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

enum CommandResult {
    RebuildStream(String),
    Shutdown,
}

fn find_device_by_name(name: &str) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if let Ok(devices) = host.output_devices() {
        for device in devices {
            if let Ok(dev_name) = device.name() {
                if dev_name == name {
                    return Some(device);
                }
            }
        }
    }
    None
}

fn build_stream(
    device: &cpal::Device,
    state: Arc<InternalPlaybackState>,
    position: Arc<AtomicU64>,
    health: Arc<PlaybackHealth>,
) -> Result<Stream, String> {
    let config = device
        .default_output_config()
        .map_err(|e| format!("Failed to get output config: {}", e))?;

    let sample_rate = config.sample_rate().0;
    state.set_device_sample_rate(sample_rate);
    eprintln!("[AudioThread] Device sample rate: {}Hz", sample_rate);

    let output_channels = config.channels() as usize;
    eprintln!("[AudioThread] Device output channels: {}", output_channels);

    {
        let mut current_routing = (**state.routing.load()).clone();
        current_routing.output_channels = output_channels;
        state.routing.store(Arc::new(current_routing));
    }

    let config: cpal::StreamConfig = config.into();

    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let now_us = health.now_us();
                let last_us = health.last_callback_us.swap(now_us, Ordering::Relaxed);

                if last_us > 0 {
                    let interval = now_us.saturating_sub(last_us);
                    let mut max = health.max_callback_interval_us.load(Ordering::Relaxed);
                    while interval > max {
                        match health.max_callback_interval_us.compare_exchange_weak(
                            max,
                            interval,
                            Ordering::Relaxed,
                            Ordering::Relaxed,
                        ) {
                            Ok(_) => break,
                            Err(current) => max = current,
                        }
                    }
                    let expected_interval_us = (data.len() as u64 * 1_000_000)
                        / (sample_rate as u64 * output_channels as u64);
                    if interval > expected_interval_us * 2 {
                        health.late_callbacks.fetch_add(1, Ordering::Relaxed);
                    }
                }

                health.callback_count.fetch_add(1, Ordering::Relaxed);
                health
                    .last_buffer_size
                    .store(data.len() as u32, Ordering::Relaxed);

                if !state.playing.load(Ordering::Relaxed) {
                    data.fill(0.0);
                    return;
                }

                let backing_slot = &state.slots[SlotId::Backing as usize];
                let backing_guard = backing_slot.samples.load();
                if backing_guard.is_none() {
                    health.underrun_count.fetch_add(1, Ordering::Relaxed);
                    data.fill(0.0);
                    return;
                }

                let backing_sample_count = backing_slot.sample_count.load(Ordering::Acquire);
                let backing_channels = backing_slot.channels.load(Ordering::Relaxed);

                if backing_channels == 0 || backing_sample_count == 0 {
                    health.underrun_count.fetch_add(1, Ordering::Relaxed);
                    data.fill(0.0);
                    return;
                }

                let mut idx = state.sample_index.load(Ordering::Relaxed);
                let mut samples_written = 0u64;
                let mut silence_written = 0u64;

                let routing = state.routing.load();

                let slot_guards: [_; SLOT_COUNT] = std::array::from_fn(|i| {
                    state.slots[i].samples.load()
                });
                let slot_sample_counts: [usize; SLOT_COUNT] = std::array::from_fn(|i| {
                    state.slots[i].sample_count.load(Ordering::Acquire)
                });
                let slot_channels: [usize; SLOT_COUNT] = std::array::from_fn(|i| {
                    state.slots[i].channels.load(Ordering::Relaxed)
                });

                for frame in data.chunks_mut(output_channels) {
                    if idx >= backing_sample_count {
                        frame.fill(0.0);
                        silence_written += 1;
                        continue;
                    }

                    frame.fill(0.0);

                    for (slot_idx, slot_routing) in routing.slots.iter().enumerate() {
                        if slot_routing.muted {
                            continue;
                        }

                        let sample_count = slot_sample_counts[slot_idx];
                        let channels = slot_channels[slot_idx];
                        if channels == 0 {
                            continue;
                        }

                        let frame_number = idx / backing_channels;
                        let slot_sample_idx = frame_number * channels;
                        if slot_sample_idx >= sample_count {
                            continue;
                        }

                        if let Some(samples) = slot_guards[slot_idx].as_ref() {
                            let left = samples.get(slot_sample_idx).copied().unwrap_or(0.0);
                            let right = if channels > 1 {
                                samples.get(slot_sample_idx + 1).copied().unwrap_or(0.0)
                            } else {
                                left
                            };

                            if routing.is_stereo_mode() {
                                frame[0] += left;
                                frame[1] += right;
                            } else if slot_routing.is_stereo {
                                if slot_routing.left_channel < output_channels {
                                    frame[slot_routing.left_channel] += left;
                                }
                                if slot_routing.right_channel < output_channels {
                                    frame[slot_routing.right_channel] += right;
                                }
                            } else {
                                let mono = (left + right) * 0.5;
                                if slot_routing.left_channel < output_channels {
                                    frame[slot_routing.left_channel] += mono;
                                }
                            }
                        }
                    }

                    samples_written += output_channels as u64;
                    idx += backing_channels;
                }

                state.sample_index.store(idx, Ordering::Relaxed);
                position.store(idx as u64, Ordering::Relaxed);
                health
                    .samples_delivered
                    .fetch_add(samples_written, Ordering::Relaxed);
                if silence_written > 0 {
                    health
                        .silence_frames
                        .fetch_add(silence_written, Ordering::Relaxed);
                }
            },
            |err| {
                eprintln!("Audio stream error: {}", err);
            },
            None,
        )
        .map_err(|e| format!("Failed to build stream: {}", e))?;

    stream
        .play()
        .map_err(|e| format!("Failed to start stream: {}", e))?;

    Ok(stream)
}

impl AudioThread {
    pub fn new(
        command_rx: mpsc::Receiver<PlaybackCommand>,
        position: Arc<AtomicU64>,
        health: Arc<PlaybackHealth>,
        device_manager: Arc<DeviceManager>,
    ) -> Result<Self, String> {
        let handle = thread::Builder::new()
            .name("audio-playback".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create tokio runtime");

                rt.block_on(Self::run_audio_loop(
                    command_rx,
                    position,
                    health,
                    device_manager,
                ));
            })
            .map_err(|e| format!("Failed to spawn audio thread: {}", e))?;

        Ok(Self {
            thread_handle: Some(handle),
        })
    }

    async fn run_audio_loop(
        mut command_rx: mpsc::Receiver<PlaybackCommand>,
        position: Arc<AtomicU64>,
        health: Arc<PlaybackHealth>,
        device_manager: Arc<DeviceManager>,
    ) {
        let state = InternalPlaybackState::new();
        let mut current_device_name: Option<String> = None;

        loop {
            let device = if let Some(ref name) = current_device_name {
                match find_device_by_name(name) {
                    Some(d) => d,
                    None => {
                        eprintln!("Device '{}' not found, falling back to default", name);
                        match cpal::default_host().default_output_device() {
                            Some(d) => d,
                            None => {
                                eprintln!("No audio output device available");
                                return;
                            }
                        }
                    }
                }
            } else {
                let selected = device_manager.get_selected_device();
                if let Some(ref name) = selected {
                    current_device_name = Some(name.clone());
                    match find_device_by_name(name) {
                        Some(d) => d,
                        None => {
                            eprintln!("Selected device '{}' not found, using default", name);
                            match cpal::default_host().default_output_device() {
                                Some(d) => d,
                                None => {
                                    eprintln!("No audio output device available");
                                    return;
                                }
                            }
                        }
                    }
                } else {
                    match cpal::default_host().default_output_device() {
                        Some(d) => d,
                        None => {
                            eprintln!("No audio output device available");
                            return;
                        }
                    }
                }
            };

            let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
            eprintln!("Audio output device: {}", device_name);

            let stream = match build_stream(
                &device,
                Arc::clone(&state),
                Arc::clone(&position),
                Arc::clone(&health),
            ) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to build audio stream: {}", e);
                    return;
                }
            };

            match Self::process_commands(&mut command_rx, &state).await {
                CommandResult::RebuildStream(new_device) => {
                    eprintln!("[AudioThread] Switching device to: {}", new_device);

                    if let Err(e) = stream.pause() {
                        eprintln!(
                            "[AudioThread] Warning: Failed to pause stream during switch: {}",
                            e
                        );
                    }

                    state.clear_all_slots();
                    state.playing.store(false, Ordering::SeqCst);

                    thread::sleep(Duration::from_millis(STREAM_SWITCH_DELAY_MS));
                    drop(stream);
                    eprintln!("[AudioThread] Old stream dropped, rebuilding...");

                    current_device_name = Some(new_device);
                    continue;
                }
                CommandResult::Shutdown => {
                    eprintln!("[AudioThread] Shutting down...");

                    if let Err(e) = stream.pause() {
                        eprintln!(
                            "[AudioThread] Warning: Failed to pause stream during shutdown: {}",
                            e
                        );
                    }

                    state.clear_all_slots();
                    state.playing.store(false, Ordering::SeqCst);

                    thread::sleep(Duration::from_millis(STREAM_SWITCH_DELAY_MS));
                    drop(stream);

                    eprintln!("[AudioThread] Shutdown complete");
                    break;
                }
            }
        }
    }

    async fn process_commands(
        command_rx: &mut mpsc::Receiver<PlaybackCommand>,
        state: &Arc<InternalPlaybackState>,
    ) -> CommandResult {
        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                PlaybackCommand::Play(track) => {
                    state.playing.store(false, Ordering::Release);
                    state.sample_index.store(0, Ordering::Release);
                    state.load_slot(SlotId::Backing, track);
                    state.playing.store(true, Ordering::Release);
                }
                PlaybackCommand::Stop => {
                    state.playing.store(false, Ordering::Release);
                    state.sample_index.store(0, Ordering::Release);
                    state.clear_all_slots();
                }
                PlaybackCommand::Pause => {
                    state.playing.store(false, Ordering::Release);
                }
                PlaybackCommand::Resume => {
                    state.playing.store(true, Ordering::Release);
                }
                PlaybackCommand::Seek(pos) => {
                    state.sample_index.store(pos as usize, Ordering::Release);
                }
                PlaybackCommand::SetDevice(device_id) => {
                    state.playing.store(false, Ordering::Release);
                    return CommandResult::RebuildStream(device_id);
                }
                PlaybackCommand::UpdateRouting(routing) => {
                    state.update_routing(routing);
                }
                PlaybackCommand::SetMute { slot, muted } => {
                    state.set_mute(slot, muted);
                }
                PlaybackCommand::LoadSlot { slot, track } => {
                    state.load_slot(slot, track);
                }
                PlaybackCommand::ClearSlot(slot) => {
                    state.clear_slot(slot);
                }
            }
        }
        CommandResult::Shutdown
    }
}
