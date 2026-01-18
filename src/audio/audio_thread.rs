use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use arc_swap::ArcSwapOption;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tokio::sync::mpsc;

use super::{DeviceManager, LoadedTrack, PlaybackCommand};

pub struct InternalPlaybackState {
    pub playing: AtomicBool,
    pub sample_index: AtomicUsize,
    pub position: AtomicU64,
    pub track: ArcSwapOption<LoadedTrack>,
    pub track_channels: AtomicUsize,
    pub track_sample_count: AtomicUsize,
}

impl InternalPlaybackState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            playing: AtomicBool::new(false),
            sample_index: AtomicUsize::new(0),
            position: AtomicU64::new(0),
            track: ArcSwapOption::new(None),
            track_channels: AtomicUsize::new(2),
            track_sample_count: AtomicUsize::new(0),
        })
    }

    pub fn load_track(&self, track: Arc<LoadedTrack>) {
        self.track_channels.store(track.channels as usize, Ordering::Release);
        self.track_sample_count.store(track.samples.len(), Ordering::Release);
        self.track.store(Some(track));
    }

    pub fn clear_track(&self) {
        self.track_sample_count.store(0, Ordering::Release);
        self.track.store(None);
    }
}

pub struct AudioThread {
    _audio_thread: JoinHandle<()>,
}

impl AudioThread {
    pub fn new(
        command_rx: mpsc::Receiver<PlaybackCommand>,
        position: Arc<AtomicU64>,
        _device_manager: Arc<DeviceManager>,
    ) -> Result<Self, String> {
        let state = InternalPlaybackState::new();
        let state_for_stream = Arc::clone(&state);
        let position_for_stream = Arc::clone(&position);

        let handle = thread::Builder::new()
            .name("audio-playback".into())
            .spawn(move || {
                let host = cpal::default_host();
                let device = match host.default_output_device() {
                    Some(d) => d,
                    None => {
                        eprintln!("No audio output device available");
                        return;
                    }
                };

                let config = match device.default_output_config() {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed to get output config: {}", e);
                        return;
                    }
                };

                let output_channels = config.channels() as usize;
                let config: cpal::StreamConfig = config.into();

                let stream_state = Arc::clone(&state_for_stream);
                let stream_position = Arc::clone(&position_for_stream);

                let stream = match device.build_output_stream(
                    &config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        if !stream_state.playing.load(Ordering::Relaxed) {
                            data.fill(0.0);
                            return;
                        }

                        let guard = stream_state.track.load();
                        let track = match guard.as_ref() {
                            Some(t) => t,
                            None => {
                                data.fill(0.0);
                                return;
                            }
                        };

                        let samples = &track.samples;
                        let track_channels = stream_state.track_channels.load(Ordering::Relaxed);
                        let sample_count = stream_state.track_sample_count.load(Ordering::Relaxed);

                        if track_channels == 0 || sample_count == 0 {
                            data.fill(0.0);
                            return;
                        }

                        let mut idx = stream_state.sample_index.load(Ordering::Relaxed);

                        for frame in data.chunks_mut(output_channels) {
                            if idx >= sample_count {
                                frame.fill(0.0);
                                continue;
                            }

                            for (ch, out_sample) in frame.iter_mut().enumerate() {
                                let src_ch = ch % track_channels;
                                let src_idx = idx + src_ch;
                                *out_sample = samples.get(src_idx).copied().unwrap_or(0.0);
                            }

                            idx += track_channels;
                        }

                        stream_state.sample_index.store(idx, Ordering::Relaxed);
                        stream_position.store(idx as u64, Ordering::Relaxed);
                    },
                    |err| {
                        eprintln!("Audio stream error: {}", err);
                    },
                    None,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to build stream: {}", e);
                        return;
                    }
                };

                if let Err(e) = stream.play() {
                    eprintln!("Failed to start stream: {}", e);
                    return;
                }

                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create tokio runtime");

                rt.block_on(Self::process_commands(command_rx, state));

                drop(stream);
            })
            .map_err(|e| format!("Failed to spawn audio thread: {}", e))?;

        Ok(Self {
            _audio_thread: handle,
        })
    }

    async fn process_commands(
        mut command_rx: mpsc::Receiver<PlaybackCommand>,
        state: Arc<InternalPlaybackState>,
    ) {
        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                PlaybackCommand::Play(track) => {
                    state.sample_index.store(0, Ordering::Release);
                    state.load_track(track);
                    state.playing.store(true, Ordering::Release);
                }
                PlaybackCommand::Stop => {
                    state.playing.store(false, Ordering::Release);
                    state.sample_index.store(0, Ordering::Release);
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
            }
        }
    }
}
