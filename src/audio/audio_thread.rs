use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use arc_swap::{ArcSwap, ArcSwapOption};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use super::{DeviceManager, LoadedTrack, PlaybackCommand, RoutingConfig, SlotId, SLOT_COUNT};
use crate::sse::SseEvent;

const STREAM_SWITCH_DELAY_MS: u64 = 50;
const CLICK_LEAD_MS: usize = 20;
const LOOP_XFADE_MS: usize = 100;
// The resampler (rubato SincFixedIn) has a finite-impulse edge artefact at the
// start and end of the file: the windowed sinc convolves the first and last
// samples against an implicit zero-pad, producing transients that meet at the
// loop wrap as a click. Skip a few hundred frames on each side of the splice
// so the wrap-fade splices between clean samples. sinc_len=128 (Balanced) frames
// of input + post-resample expansion → ~256 output frames of edge tainting at
// 96k. 512 gives margin.
const LOOP_EDGE_TRIM_FRAMES: usize = 512;

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
    pub looping: AtomicBool,
    pub sample_index: AtomicUsize,
    pub device_sample_rate: AtomicU32,
    pub routing: ArcSwap<RoutingConfig>,
    pub slots: [TrackSlot; SLOT_COUNT],
    pub crossfade_active: AtomicBool,
    pub crossfade_total_samples: AtomicU64,
    pub crossfade_remaining_samples: AtomicU64,
    pub aux_sample_index: AtomicUsize,
    pub fade_out_active: AtomicBool,
    pub fade_out_total_samples: AtomicU64,
    pub fade_out_remaining_samples: AtomicU64,
    pub log_post_swap: AtomicBool,
    pub loop_wrap_log_remaining: AtomicUsize,
    pub log_post_wrap: AtomicBool,
}

impl InternalPlaybackState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            playing: AtomicBool::new(false),
            looping: AtomicBool::new(false),
            sample_index: AtomicUsize::new(0),
            device_sample_rate: AtomicU32::new(0),
            routing: ArcSwap::from_pointee(RoutingConfig::default()),
            slots: Default::default(),
            crossfade_active: AtomicBool::new(false),
            crossfade_total_samples: AtomicU64::new(0),
            crossfade_remaining_samples: AtomicU64::new(0),
            aux_sample_index: AtomicUsize::new(0),
            fade_out_active: AtomicBool::new(false),
            fade_out_total_samples: AtomicU64::new(0),
            fade_out_remaining_samples: AtomicU64::new(0),
            log_post_swap: AtomicBool::new(false),
            loop_wrap_log_remaining: AtomicUsize::new(0),
            log_post_wrap: AtomicBool::new(false),
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

    pub fn set_volume(&self, slot: SlotId, volume: f32) {
        let mut current = (**self.routing.load()).clone();
        current.set_volume(slot, volume);
        self.routing.store(Arc::new(current));
    }

    pub fn load_slot(&self, slot: SlotId, track: Arc<LoadedTrack>) {
        let device_rate = self.device_sample_rate.load(Ordering::Acquire);
        let samples = track.get_samples_for_rate(device_rate);
        debug!(
            "load_slot: slot={:?}, device_rate={}, samples_len={}, channels={}",
            slot,
            device_rate,
            samples.len(),
            track.channels
        );
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
                        error!("Thread panicked during shutdown: {:?}", e);
                    }
                    debug!("Thread joined successfully");
                    break;
                }
                if start.elapsed() > Duration::from_millis(THREAD_JOIN_TIMEOUT_MS) {
                    warn!("Thread join timed out after {}ms", THREAD_JOIN_TIMEOUT_MS);
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

enum CommandResult {
    RebuildStream(String),
    StreamError,
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

fn resolve_device(
    device_manager: &DeviceManager,
    override_name: &Option<String>,
) -> Option<cpal::Device> {
    if let Some(ref name) = override_name {
        if let Some(d) = find_device_by_name(name) {
            return Some(d);
        }
        warn!("Device '{}' not found, falling back to default", name);
    } else if let Some(ref name) = device_manager.get_selected_device() {
        if let Some(d) = find_device_by_name(name) {
            return Some(d);
        }
        warn!("Selected device '{}' not found, using default", name);
    }
    cpal::default_host().default_output_device()
}

fn build_stream(
    device: &cpal::Device,
    state: Arc<InternalPlaybackState>,
    position: Arc<AtomicU64>,
    health: Arc<PlaybackHealth>,
    stream_error: Arc<AtomicBool>,
    device_manager: &DeviceManager,
) -> Result<Stream, String> {
    let config = device
        .default_output_config()
        .map_err(|e| format!("Failed to get output config: {}", e))?;

    let sample_rate = config.sample_rate().0;
    state.set_device_sample_rate(sample_rate);
    info!("Device sample rate: {}Hz", sample_rate);

    let output_channels = config.channels() as usize;
    info!("Device output channels: {}", output_channels);

    device_manager.set_active_device_info(output_channels as u16, sample_rate);

    {
        let mut current_routing = (**state.routing.load()).clone();
        current_routing.output_channels = output_channels;
        state.routing.store(Arc::new(current_routing));
    }

    let sample_format = config.sample_format();
    debug!("Device sample format: {:?}", sample_format);
    let config: cpal::StreamConfig = config.into();

    macro_rules! build_output_stream {
        ($sample_type:ty, $zero:expr, $convert:expr) => {{
            device
                .build_output_stream(
                    &config,
                    move |data: &mut [$sample_type], _: &cpal::OutputCallbackInfo| {
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
                            data.fill($zero);
                            return;
                        }

                        let backing_slot = &state.slots[SlotId::Backing as usize];
                        let backing_guard = backing_slot.samples.load();
                        if backing_guard.is_none() {
                            health.underrun_count.fetch_add(1, Ordering::Relaxed);
                            data.fill($zero);
                            return;
                        }

                        let backing_sample_count =
                            backing_slot.sample_count.load(Ordering::Acquire);
                        let backing_channels = backing_slot.channels.load(Ordering::Relaxed);

                        if backing_channels == 0 || backing_sample_count == 0 {
                            health.underrun_count.fetch_add(1, Ordering::Relaxed);
                            data.fill($zero);
                            return;
                        }

                        let mut idx = state.sample_index.load(Ordering::Relaxed);
                        let mut samples_written = 0u64;
                        let mut silence_written = 0u64;

                        if state.log_post_swap.swap(false, Ordering::AcqRel) {
                            let delay_frames = (sample_rate as usize * CLICK_LEAD_MS) / 1000;
                            tracing::info!(
                                "[xfade-post-swap] callback start: idx={} backing_ch={} backing_samples={} computed_first_frame={} delay_frames={}",
                                idx,
                                backing_channels,
                                backing_sample_count,
                                if backing_channels > 0 { (idx / backing_channels).saturating_sub(delay_frames) } else { 0 },
                                delay_frames,
                            );
                        }

                        let routing = state.routing.load();

                        let slot_guards: [_; SLOT_COUNT] =
                            std::array::from_fn(|i| state.slots[i].samples.load());
                        let slot_sample_counts: [usize; SLOT_COUNT] = std::array::from_fn(|i| {
                            state.slots[i].sample_count.load(Ordering::Acquire)
                        });
                        let slot_channels: [usize; SLOT_COUNT] = std::array::from_fn(|i| {
                            state.slots[i].channels.load(Ordering::Relaxed)
                        });

                        let convert = $convert;

                        let looping = state.looping.load(Ordering::Relaxed);
                        let lead_idx_offset = backing_channels
                            * ((sample_rate as usize * CLICK_LEAD_MS) / 1000);
                        let loop_xfade_samples = backing_channels
                            * ((sample_rate as usize * LOOP_XFADE_MS) / 1000);
                        let delay_frames_static = (sample_rate as usize * CLICK_LEAD_MS) / 1000;

                        let mut xfade_active = state.crossfade_active.load(Ordering::Acquire);
                        let xfade_total = state.crossfade_total_samples.load(Ordering::Relaxed) as usize;
                        let mut xfade_remaining = state
                            .crossfade_remaining_samples
                            .load(Ordering::Relaxed) as usize;
                        let mut aux_idx = state.aux_sample_index.load(Ordering::Relaxed);
                        let aux_channels = slot_channels[SlotId::Aux as usize];
                        let aux_sample_count = slot_sample_counts[SlotId::Aux as usize];
                        let mut xfade_completed_this_buffer = false;
                        let xfade_step_per_frame = backing_channels;
                        let mut post_swap_in_buffer = false;
                        let mut post_fade_out_in_buffer = false;

                        let mut fade_out_active = state.fade_out_active.load(Ordering::Acquire);
                        let fade_out_total = state.fade_out_total_samples.load(Ordering::Relaxed) as usize;
                        let mut fade_out_remaining = state
                            .fade_out_remaining_samples
                            .load(Ordering::Relaxed) as usize;
                        let mut fade_out_completed_this_buffer = false;
                        let fade_out_step_per_frame = backing_channels;

                        // Loop-aware effective end: skip the resampler's edge artefacts on the
                        // trailing side. The wrap fires LOOP_EDGE_TRIM_FRAMES short of the
                        // raw end so tail_frame never reaches the tainted region.
                        let edge_trim_samples = LOOP_EDGE_TRIM_FRAMES * backing_channels;
                        let effective_end = backing_sample_count.saturating_sub(edge_trim_samples);

                        for frame in data.chunks_mut(output_channels) {
                            if looping && idx >= effective_end {
                                // After the wrap-fade the head read advanced by exactly the
                                // wrap-window width (effective_end - wrap_window_start_local).
                                // Set idx so the next normal Backing read (which applies
                                // -delay_frames) lands at LOOP_EDGE_TRIM_FRAMES + that width.
                                let lead_offset_samples = delay_frames_static * backing_channels;
                                let wrap_window_start_local = effective_end
                                    .saturating_sub(loop_xfade_samples)
                                    .saturating_add(lead_offset_samples);
                                let xfade_window_samples = effective_end
                                    .saturating_sub(wrap_window_start_local)
                                    .max(1);
                                let head_end_frame = LOOP_EDGE_TRIM_FRAMES
                                    + (xfade_window_samples / backing_channels.max(1));
                                let new_idx = (head_end_frame + delay_frames_static) * backing_channels;
                                let next_read_frame = (new_idx / backing_channels).saturating_sub(delay_frames_static);
                                let backing_slot = SlotId::Backing as usize;
                                let sample_at_next = slot_guards[backing_slot]
                                    .as_ref()
                                    .and_then(|s| s.get(next_read_frame * backing_channels).copied())
                                    .unwrap_or(0.0);
                                tracing::info!(
                                    "[loop-wrap] WRAP fire: old_idx={} effective_end={} backing_samples={} new_idx={} next_read_frame={} sample_at_next_read_frame={:.5}",
                                    idx,
                                    effective_end,
                                    backing_sample_count,
                                    new_idx,
                                    next_read_frame,
                                    sample_at_next,
                                );
                                state.log_post_wrap.store(true, Ordering::Relaxed);
                                idx = new_idx;
                            } else if idx >= backing_sample_count {
                                if looping {
                                    let head_end_frame = LOOP_EDGE_TRIM_FRAMES
                                        + (loop_xfade_samples / backing_channels.max(1));
                                    idx = (head_end_frame + delay_frames_static) * backing_channels;
                                } else {
                                    frame.fill($zero);
                                    silence_written += 1;
                                    continue;
                                }
                            }

                            // Equal-power crossfade gains. cos(0)=1, cos(π/2)=0; sin mirrors.
                            let (outgoing_gain, incoming_gain) = if xfade_active && xfade_total > 0 {
                                let t = 1.0
                                    - (xfade_remaining as f32 / xfade_total as f32).clamp(0.0, 1.0);
                                let angle = t * std::f32::consts::FRAC_PI_2;
                                (angle.cos(), angle.sin())
                            } else {
                                (1.0, 0.0)
                            };

                            // Fade-out gain (applied to ALL slots equally on top of any
                            // crossfade gain, so a fade-out triggered mid-crossfade scales the
                            // already-summed mix down to silence).
                            let fade_out_gain = if fade_out_active && fade_out_total > 0 {
                                let t = 1.0
                                    - (fade_out_remaining as f32 / fade_out_total as f32).clamp(0.0, 1.0);
                                (t * std::f32::consts::FRAC_PI_2).cos()
                            } else {
                                1.0
                            };

                            frame.fill($zero);

                            let mut mix_buf = [0.0f32; 128];
                            let ch = output_channels.min(128);

                            for (slot_idx, slot_routing) in routing.slots.iter().enumerate() {
                                if slot_routing.muted {
                                    continue;
                                }
                                if post_fade_out_in_buffer {
                                    continue;
                                }
                                let is_aux = slot_idx == SlotId::Aux as usize;
                                if post_swap_in_buffer && !is_aux {
                                    continue;
                                }

                                let sample_count = slot_sample_counts[slot_idx];
                                let channels = slot_channels[slot_idx];
                                if channels == 0 {
                                    continue;
                                }

                                let aux_active_read = is_aux && (xfade_active || post_swap_in_buffer) && aux_channels > 0;
                                let slot_frame_number = if aux_active_read {
                                    aux_idx / aux_channels
                                } else {
                                    idx / backing_channels
                                };

                                let is_backing = slot_idx == SlotId::Backing as usize;
                                // Wrap window covers the file tail with both edge artefacts
                                // skipped: tail read stops at (effective_end-1)/channels (the
                                // last clean frame), head read starts at LOOP_EDGE_TRIM_FRAMES
                                // (skipping the leading edge artefact). Backing read applies
                                // -delay_frames so we offset window start by +lead_offset so
                                // tail_frame reaches effective_end's last clean frame at end of
                                // the wrap window.
                                let lead_offset_samples = delay_frames_static * backing_channels;
                                let wrap_window_start = effective_end
                                    .saturating_sub(loop_xfade_samples)
                                    .saturating_add(lead_offset_samples);
                                let in_loop_wrap = looping
                                    && is_backing
                                    && !xfade_active
                                    && !post_swap_in_buffer
                                    && loop_xfade_samples > 0
                                    && idx >= wrap_window_start
                                    && idx < effective_end;

                                if in_loop_wrap {
                                    if let Some(samples) = slot_guards[slot_idx].as_ref() {
                                        let tail_frame = slot_frame_number.saturating_sub(delay_frames_static);
                                        let head_progress = idx.saturating_sub(wrap_window_start);
                                        let head_frame = LOOP_EDGE_TRIM_FRAMES + head_progress / backing_channels;
                                        let xfade_window_samples = effective_end.saturating_sub(wrap_window_start).max(1);
                                        let t = (head_progress as f32 / xfade_window_samples as f32).clamp(0.0, 1.0);
                                        let angle = t * std::f32::consts::FRAC_PI_2;
                                        let tail_gain = angle.cos();
                                        let head_gain = angle.sin();
                                        let tail_idx = tail_frame * channels;
                                        let head_idx = head_frame * channels;
                                        let prev = state.loop_wrap_log_remaining.load(Ordering::Relaxed);
                                        if prev == 0 {
                                            tracing::info!(
                                                "[loop-wrap] ENTER wrap window: idx={} backing_samples={} loop_xfade_samples={} delay_frames={} head_progress={} t={:.3}",
                                                idx,
                                                backing_sample_count,
                                                loop_xfade_samples,
                                                delay_frames_static,
                                                head_progress,
                                                t,
                                            );
                                            state.loop_wrap_log_remaining.store(xfade_window_samples / backing_channels.max(1), Ordering::Relaxed);
                                        } else if prev == 1 {
                                            let head_sample = samples.get(head_idx).copied().unwrap_or(0.0);
                                            let tail_sample = samples.get(tail_idx).copied().unwrap_or(0.0);
                                            let mixed = tail_sample * tail_gain + head_sample * head_gain;
                                            tracing::info!(
                                                "[loop-wrap] LAST frame: idx={} tail_frame={} head_frame={} t={:.3} tail_gain={:.3} head_gain={:.3} tail_sample={:.5} head_sample={:.5} mixed={:.5}",
                                                idx, tail_frame, head_frame, t, tail_gain, head_gain, tail_sample, head_sample, mixed,
                                            );
                                            state.loop_wrap_log_remaining.store(0, Ordering::Relaxed);
                                        } else {
                                            state.loop_wrap_log_remaining.store(prev - 1, Ordering::Relaxed);
                                        }
                                        let vol = slot_routing.volume * fade_out_gain;
                                        let left = (
                                            samples.get(tail_idx).copied().unwrap_or(0.0) * tail_gain
                                            + samples.get(head_idx).copied().unwrap_or(0.0) * head_gain
                                        ) * vol;
                                        let right = if channels > 1 {
                                            (
                                                samples.get(tail_idx + 1).copied().unwrap_or(0.0) * tail_gain
                                                + samples.get(head_idx + 1).copied().unwrap_or(0.0) * head_gain
                                            ) * vol
                                        } else {
                                            left
                                        };
                                        if routing.is_stereo_mode() {
                                            mix_buf[0] += left;
                                            mix_buf[1] += right;
                                        } else if slot_routing.is_stereo {
                                            if slot_routing.left_channel < ch {
                                                mix_buf[slot_routing.left_channel] += left;
                                            }
                                            if slot_routing.right_channel < ch {
                                                mix_buf[slot_routing.right_channel] += right;
                                            }
                                        } else {
                                            let mono = (left + right) * 0.5;
                                            if slot_routing.left_channel < ch {
                                                mix_buf[slot_routing.left_channel] += mono;
                                            }
                                        }
                                    }
                                    continue;
                                }

                                let slot_frame = if slot_idx == SlotId::Backing as usize || slot_idx == SlotId::Guide as usize {
                                    if slot_frame_number < delay_frames_static { continue; }
                                    slot_frame_number - delay_frames_static
                                } else {
                                    slot_frame_number
                                };
                                let slot_sample_idx = slot_frame * channels;
                                if slot_sample_idx >= sample_count {
                                    continue;
                                }

                                if let Some(samples) = slot_guards[slot_idx].as_ref() {
                                    let crossfade_gain = if post_swap_in_buffer && is_aux {
                                        1.0
                                    } else if !xfade_active {
                                        1.0
                                    } else if slot_idx == SlotId::Backing as usize {
                                        outgoing_gain
                                    } else if is_aux {
                                        incoming_gain
                                    } else {
                                        outgoing_gain
                                    };
                                    let vol = slot_routing.volume * crossfade_gain * fade_out_gain;
                                    let raw_sample = samples.get(slot_sample_idx).copied().unwrap_or(0.0);
                                    let left = raw_sample * vol;
                                    let right = if channels > 1 {
                                        samples.get(slot_sample_idx + 1).copied().unwrap_or(0.0)
                                            * vol
                                    } else {
                                        left
                                    };

                                    if slot_idx == SlotId::Backing as usize
                                        && state.log_post_wrap.swap(false, Ordering::Relaxed)
                                    {
                                        tracing::info!(
                                            "[loop-wrap] FIRST post-slam frame: idx={} read_frame={} raw_sample={:.5} output_left={:.5}",
                                            idx, slot_frame, raw_sample, left,
                                        );
                                    }

                                    if routing.is_stereo_mode() {
                                        mix_buf[0] += left;
                                        mix_buf[1] += right;
                                    } else if slot_routing.is_stereo {
                                        if slot_routing.left_channel < ch {
                                            mix_buf[slot_routing.left_channel] += left;
                                        }
                                        if slot_routing.right_channel < ch {
                                            mix_buf[slot_routing.right_channel] += right;
                                        }
                                    } else {
                                        let mono = (left + right) * 0.5;
                                        if slot_routing.left_channel < ch {
                                            mix_buf[slot_routing.left_channel] += mono;
                                        }
                                    }
                                }
                            }

                            for (i, sample) in frame.iter_mut().enumerate() {
                                *sample = convert(mix_buf[i]);
                            }

                            samples_written += output_channels as u64;
                            idx += backing_channels;

                            if fade_out_active {
                                if fade_out_remaining > fade_out_step_per_frame {
                                    fade_out_remaining -= fade_out_step_per_frame;
                                } else {
                                    fade_out_remaining = 0;
                                    fade_out_active = false;
                                    fade_out_completed_this_buffer = true;
                                    post_fade_out_in_buffer = true;
                                    state.playing.store(false, Ordering::Release);
                                    state.looping.store(false, Ordering::Release);
                                    state.crossfade_active.store(false, Ordering::Release);
                                    state.sample_index.store(0, Ordering::Release);
                                    state.clear_all_slots();
                                }
                            }

                            if post_swap_in_buffer {
                                if aux_channels > 0 && aux_idx + aux_channels <= aux_sample_count {
                                    aux_idx += aux_channels;
                                }
                            }

                            if xfade_active {
                                if aux_channels > 0 && aux_idx + aux_channels <= aux_sample_count {
                                    aux_idx += aux_channels;
                                }
                                if xfade_remaining > xfade_step_per_frame {
                                    xfade_remaining -= xfade_step_per_frame;
                                } else {
                                    xfade_remaining = 0;
                                    xfade_active = false;
                                    xfade_completed_this_buffer = true;
                                    post_swap_in_buffer = true;
                                    // Swap: incoming becomes Backing. Subsequent frames in this
                                    // buffer continue but slot_guards/slot_sample_counts are stale
                                    // until next callback — outgoing Backing will keep playing
                                    // until then, but with outgoing_gain now 1.0 it's audible. To
                                    // avoid a one-buffer of "outgoing at full gain" before swap is
                                    // visible to the next callback's snapshots, point idx at where
                                    // aux_idx had progressed to within the incoming's frame index
                                    // space; the next callback re-reads slots and sees the swapped
                                    // contents.
                                    if let Some(aux_samples) = state.slots[SlotId::Aux as usize].samples.load_full() {
                                        state.slots[SlotId::Backing as usize].channels.store(aux_channels, Ordering::Relaxed);
                                        state.slots[SlotId::Backing as usize].samples.store(Some(aux_samples));
                                        std::sync::atomic::fence(Ordering::Release);
                                        state.slots[SlotId::Backing as usize].sample_count.store(aux_sample_count, Ordering::Release);
                                    }
                                    state.slots[SlotId::Aux as usize].clear();
                                    let delay_frames = (sample_rate as usize * CLICK_LEAD_MS) / 1000;
                                    let new_idx = aux_idx + delay_frames * backing_channels;
                                    tracing::info!(
                                        "[xfade-swap] aux_idx={} aux_ch={} backing_ch={} delay_frames={} new_idx={} aux_frames_played={} new_backing_frame={}",
                                        aux_idx,
                                        aux_channels,
                                        backing_channels,
                                        delay_frames,
                                        new_idx,
                                        if aux_channels > 0 { aux_idx / aux_channels } else { 0 },
                                        if backing_channels > 0 { (new_idx / backing_channels).saturating_sub(delay_frames) } else { 0 },
                                    );
                                    idx = new_idx;
                                    state.log_post_swap.store(true, Ordering::Release);
                                }
                            }
                        }

                        state.sample_index.store(idx, Ordering::Relaxed);
                        position.store(idx as u64, Ordering::Relaxed);
                        if xfade_completed_this_buffer || xfade_active {
                            state.crossfade_active.store(xfade_active, Ordering::Release);
                            state.crossfade_remaining_samples.store(xfade_remaining as u64, Ordering::Relaxed);
                            state.aux_sample_index.store(aux_idx, Ordering::Relaxed);
                        }
                        if fade_out_completed_this_buffer || fade_out_active {
                            state.fade_out_active.store(fade_out_active, Ordering::Release);
                            state.fade_out_remaining_samples.store(fade_out_remaining as u64, Ordering::Relaxed);
                        }
                        health
                            .samples_delivered
                            .fetch_add(samples_written, Ordering::Relaxed);
                        if silence_written > 0 {
                            health
                                .silence_frames
                                .fetch_add(silence_written, Ordering::Relaxed);
                        }
                    },
                    {
                        let stream_error = Arc::clone(&stream_error);
                        move |err| {
                            if !stream_error.swap(true, Ordering::SeqCst) {
                                error!("Stream error (device lost): {}", err);
                            }
                        }
                    },
                    None,
                )
                .map_err(|e| format!("Failed to build stream: {}", e))
        }};
    }

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_output_stream!(f32, 0.0f32, |s: f32| s),
        cpal::SampleFormat::I32 => {
            build_output_stream!(i32, 0i32, |s: f32| (s * i32::MAX as f32) as i32)
        }
        cpal::SampleFormat::I16 => {
            build_output_stream!(i16, 0i16, |s: f32| (s * i16::MAX as f32) as i16)
        }
        _ => return Err(format!("Unsupported sample format: {:?}", sample_format)),
    }?;

    stream
        .play()
        .map_err(|e| format!("Failed to start stream: {}", e))?;

    Ok(stream)
}

impl AudioThread {

    pub fn is_alive(&self) -> bool {
        self.thread_handle
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }

    pub fn new(
        command_rx: mpsc::Receiver<PlaybackCommand>,
        position: Arc<AtomicU64>,
        health: Arc<PlaybackHealth>,
        device_manager: Arc<DeviceManager>,
        broadcast_tx: Arc<broadcast::Sender<SseEvent>>,
        state: Arc<InternalPlaybackState>,
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
                    broadcast_tx,
                    state,
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
        broadcast_tx: Arc<broadcast::Sender<SseEvent>>,
        state: Arc<InternalPlaybackState>,
    ) {
        let mut current_device_name: Option<String> = None;

        loop {
            let device = match resolve_device(&device_manager, &current_device_name) {
                Some(d) => d,
                None => {
                    warn!("No audio output device available, waiting for device...");
                    let _ = broadcast_tx.send(SseEvent::AudioDeviceLost {
                        device_name: current_device_name
                            .clone()
                            .unwrap_or_else(|| "none".into()),
                    });
                    Self::wait_for_device_recovery(
                        &mut command_rx,
                        &state,
                        &current_device_name,
                        &broadcast_tx,
                    )
                    .await;
                    continue;
                }
            };

            let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
            if current_device_name.is_none() {
                if let Some(ref selected) = device_manager.get_selected_device() {
                    current_device_name = Some(selected.clone());
                }
            }
            info!("Audio output device: {}", device_name);

            let stream_error = Arc::new(AtomicBool::new(false));

            let stream = match build_stream(
                &device,
                Arc::clone(&state),
                Arc::clone(&position),
                Arc::clone(&health),
                Arc::clone(&stream_error),
                &device_manager,
            ) {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to build audio stream: {}, waiting for device...", e);
                    let _ = broadcast_tx.send(SseEvent::AudioDeviceLost {
                        device_name: current_device_name
                            .clone()
                            .unwrap_or_else(|| "unknown".into()),
                    });
                    Self::wait_for_device_recovery(
                        &mut command_rx,
                        &state,
                        &current_device_name,
                        &broadcast_tx,
                    )
                    .await;
                    continue;
                }
            };

            match Self::process_commands(&mut command_rx, &state, &position, &stream_error).await {
                CommandResult::RebuildStream(new_device) => {
                    info!("Switching device to: {}", new_device);

                    if let Err(e) = stream.pause() {
                        warn!("Failed to pause stream during switch: {}", e);
                    }

                    state.clear_all_slots();
                    state.playing.store(false, Ordering::SeqCst);

                    thread::sleep(Duration::from_millis(STREAM_SWITCH_DELAY_MS));
                    drop(stream);
                    debug!("Old stream dropped, rebuilding...");

                    current_device_name = Some(new_device);
                    continue;
                }
                CommandResult::StreamError => {
                    let lost_name = current_device_name
                        .clone()
                        .unwrap_or_else(|| device_name.clone());
                    error!("Device lost: {}", lost_name);

                    let _ = broadcast_tx.send(SseEvent::AudioDeviceLost {
                        device_name: lost_name.clone(),
                    });

                    state.playing.store(false, Ordering::SeqCst);
                    state.clear_all_slots();
                    let _ = stream.pause();
                    drop(stream);

                    Self::wait_for_device_recovery(
                        &mut command_rx,
                        &state,
                        &current_device_name,
                        &broadcast_tx,
                    )
                    .await;

                    continue;
                }
                CommandResult::Shutdown => {
                    info!("Shutting down...");

                    if let Err(e) = stream.pause() {
                        warn!("Failed to pause stream during shutdown: {}", e);
                    }

                    state.clear_all_slots();
                    state.playing.store(false, Ordering::SeqCst);

                    thread::sleep(Duration::from_millis(STREAM_SWITCH_DELAY_MS));
                    drop(stream);

                    info!("Shutdown complete");
                    break;
                }
            }
        }
    }

    async fn wait_for_device_recovery(
        command_rx: &mut mpsc::Receiver<PlaybackCommand>,
        state: &Arc<InternalPlaybackState>,
        device_name: &Option<String>,
        broadcast_tx: &Arc<broadcast::Sender<SseEvent>>,
    ) {
        const RETRY_INTERVAL: Duration = Duration::from_secs(2);
        let mut attempts = 0u32;

        loop {
            let found = if let Some(ref name) = device_name {
                find_device_by_name(name).is_some()
            } else {
                cpal::default_host().default_output_device().is_some()
            };

            if found {
                let name = device_name.clone().unwrap_or_else(|| "default".to_string());
                info!("Device restored: {} (after {} retries)", name, attempts);
                let _ = broadcast_tx.send(SseEvent::AudioDeviceRestored { device_name: name });
                return;
            }

            attempts += 1;
            if attempts % 15 == 1 {
                let name = device_name.clone().unwrap_or_else(|| "default".to_string());
                debug!("Waiting for device '{}' (attempt {})...", name, attempts);
            }

            tokio::select! {
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(PlaybackCommand::SetDevice(new_device)) => {
                            info!("Device switch requested during recovery: {}", new_device);
                            state.playing.store(false, Ordering::Release);
                            return;
                        }
                        Some(_) => {}
                        None => return,
                    }
                }
                _ = tokio::time::sleep(RETRY_INTERVAL) => {}
            }
        }
    }

    async fn process_commands(
        command_rx: &mut mpsc::Receiver<PlaybackCommand>,
        state: &Arc<InternalPlaybackState>,
        position: &Arc<AtomicU64>,
        stream_error: &Arc<AtomicBool>,
    ) -> CommandResult {
        let mut error_check = tokio::time::interval(Duration::from_millis(500));
        error_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                cmd = command_rx.recv() => {
                    let Some(cmd) = cmd else {
                        return CommandResult::Shutdown;
                    };
                    match cmd {
                        PlaybackCommand::Play(track) => {
                            let prev_idx = state.sample_index.load(Ordering::Acquire);
                            state.playing.store(false, Ordering::Release);
                            state.looping.store(false, Ordering::Release);
                            state.crossfade_active.store(false, Ordering::Release);
                            state.crossfade_remaining_samples.store(0, Ordering::Release);
                            state.aux_sample_index.store(0, Ordering::Release);
                            state.fade_out_active.store(false, Ordering::Release);
                            state.fade_out_remaining_samples.store(0, Ordering::Release);
                            state.sample_index.store(0, Ordering::Release);
                            state.load_slot(SlotId::Backing, track);
                            state.playing.store(true, Ordering::Release);
                            tracing::info!("[seek-trace] audio-thread Play: sample_index reset {} -> 0; position atomic={}",
                                prev_idx, position.load(Ordering::SeqCst));

                            let routing = state.routing.load();
                            debug!("PLAY started - output_channels={}, stereo_mode={}",
                                routing.output_channels, routing.is_stereo_mode());
                            for (i, slot_routing) in routing.slots.iter().enumerate() {
                                let slot_samples = state.slots[i].sample_count.load(Ordering::Acquire);
                                let slot_channels = state.slots[i].channels.load(Ordering::Relaxed);
                                debug!("  Slot {}: samples={}, ch={}, route_to={}/{}, muted={}",
                                    i, slot_samples, slot_channels,
                                    slot_routing.left_channel, slot_routing.right_channel, slot_routing.muted);
                            }
                        }
                        PlaybackCommand::Stop => {
                            state.playing.store(false, Ordering::Release);
                            state.looping.store(false, Ordering::Release);
                            state.crossfade_active.store(false, Ordering::Release);
                            state.crossfade_remaining_samples.store(0, Ordering::Release);
                            state.aux_sample_index.store(0, Ordering::Release);
                            state.fade_out_active.store(false, Ordering::Release);
                            state.fade_out_remaining_samples.store(0, Ordering::Release);
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
                            let prev_idx = state.sample_index.load(Ordering::Acquire);
                            state.sample_index.store(pos as usize, Ordering::Release);
                            position.store(pos, Ordering::Release);
                            tracing::info!("[seek-trace] audio-thread Seek: sample_index {} -> {}; position atomic <- {}",
                                prev_idx, pos, pos);
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
                        PlaybackCommand::SetVolume { slot, volume } => {
                            state.set_volume(slot, volume);
                        }
                        PlaybackCommand::LoadSlot { slot, track } => {
                            debug!("LoadSlot command: slot={:?}, track_channels={}, track_samples={}",
                                slot, track.channels, track.original_samples.len());
                            state.load_slot(slot, track);
                        }
                        PlaybackCommand::ClearSlot(slot) => {
                            state.clear_slot(slot);
                        }
                        PlaybackCommand::SetLooping(looping) => {
                            state.looping.store(looping, Ordering::Release);
                        }
                        PlaybackCommand::StopWithFade { fade_samples } => {
                            // Initiate fade-to-silence on whatever is currently playing.
                            // Per-frame mix multiplies all slots by cos(t·π/2). When the
                            // fade completes, the callback itself sets playing=false and
                            // clears slots.
                            state.fade_out_total_samples.store(fade_samples, Ordering::Release);
                            state.fade_out_remaining_samples.store(fade_samples, Ordering::Release);
                            state.fade_out_active.store(true, Ordering::Release);
                        }
                        PlaybackCommand::StartCrossfade { incoming, fade_samples } => {
                            state.load_slot(SlotId::Aux, incoming);
                            state.aux_sample_index.store(0, Ordering::Release);
                            state.crossfade_total_samples.store(fade_samples, Ordering::Release);
                            state.crossfade_remaining_samples.store(fade_samples, Ordering::Release);
                            state.crossfade_active.store(true, Ordering::Release);
                            state.fade_out_active.store(false, Ordering::Release);
                            state.fade_out_remaining_samples.store(0, Ordering::Release);
                            state.looping.store(false, Ordering::Release);
                            state.playing.store(true, Ordering::Release);
                        }
                    }
                }
                _ = error_check.tick() => {
                    if stream_error.load(Ordering::SeqCst) {
                        return CommandResult::StreamError;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod crossfade_curve_tests {
    // The crossfade and stop-fade per-frame math uses equal-power curves:
    //   outgoing_gain = cos(t * π/2)
    //   incoming_gain = sin(t * π/2)
    // For t in [0, 1] this gives cos²(t·π/2) + sin²(t·π/2) = 1, so the
    // total power stays constant across the fade. This test asserts those
    // identities hold within f32 precision at the boundaries and midpoint.

    #[test]
    fn equal_power_holds_at_boundaries_and_midpoint() {
        let half_pi = std::f32::consts::FRAC_PI_2;

        // t = 0: outgoing fully on, incoming fully off
        let t = 0.0_f32;
        let cos_g = (t * half_pi).cos();
        let sin_g = (t * half_pi).sin();
        assert!((cos_g - 1.0).abs() < 1e-6, "cos(0) = {}", cos_g);
        assert!(sin_g.abs() < 1e-6, "sin(0) = {}", sin_g);
        assert!(
            (cos_g * cos_g + sin_g * sin_g - 1.0).abs() < 1e-6,
            "power at t=0 = {}",
            cos_g * cos_g + sin_g * sin_g
        );

        // t = 0.5: midpoint — both at √2/2 ≈ 0.7071, sum of squares still 1
        let t = 0.5_f32;
        let cos_g = (t * half_pi).cos();
        let sin_g = (t * half_pi).sin();
        assert!(
            (cos_g * cos_g + sin_g * sin_g - 1.0).abs() < 1e-6,
            "power at t=0.5 = {} (cos={}, sin={})",
            cos_g * cos_g + sin_g * sin_g,
            cos_g,
            sin_g
        );
        // Both should be near 1/√2.
        let inv_sqrt_2 = 1.0_f32 / 2.0_f32.sqrt();
        assert!((cos_g - inv_sqrt_2).abs() < 1e-6);
        assert!((sin_g - inv_sqrt_2).abs() < 1e-6);

        // t = 1: outgoing fully off, incoming fully on
        let t = 1.0_f32;
        let cos_g = (t * half_pi).cos();
        let sin_g = (t * half_pi).sin();
        assert!(cos_g.abs() < 1e-6, "cos(π/2) = {}", cos_g);
        assert!((sin_g - 1.0).abs() < 1e-6, "sin(π/2) = {}", sin_g);
        assert!(
            (cos_g * cos_g + sin_g * sin_g - 1.0).abs() < 1e-6,
            "power at t=1 = {}",
            cos_g * cos_g + sin_g * sin_g
        );
    }

    #[test]
    fn equal_power_holds_across_sweep() {
        // Sample 100 points across the fade and assert no power dip > tolerance.
        let half_pi = std::f32::consts::FRAC_PI_2;
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            let cos_g = (t * half_pi).cos();
            let sin_g = (t * half_pi).sin();
            let power = cos_g * cos_g + sin_g * sin_g;
            assert!(
                (power - 1.0).abs() < 1e-5,
                "power at t={} = {} (cos={}, sin={})",
                t,
                power,
                cos_g,
                sin_g
            );
        }
    }
}
