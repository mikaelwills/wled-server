use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::audio::PlaybackState;

use crate::config::PatternType;
use crate::effects_engine::{BoardTarget, EffectConfig, EffectsEngine, EngineCommand};
use crate::pattern::generate_sequence;
use crate::pattern_engine::{BoardInfo, PatternCommand, PatternEngine};
use crate::timing_metrics::TimingMetrics;

const COARSE_THRESHOLD_SAMPLES: u64 = 4410;
const FINE_THRESHOLD_SAMPLES: u64 = 441;
const COARSE_SLEEP: Duration = Duration::from_millis(50);
const FINE_SLEEP: Duration = Duration::from_millis(5);
const POLL_INTERVAL: Duration = Duration::from_micros(500);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const POSITION_STALL_TIMEOUT: Duration = Duration::from_secs(10);
const AUDIO_END_DETECTION: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub struct AudioTimingConfig {
    pub position: Arc<AtomicU64>,
    pub playback_state: Arc<AtomicU8>,
    pub sample_rate: u32,
    pub channels: u32,
}

impl AudioTimingConfig {
    pub fn samples_to_seconds(&self, samples: u64) -> f64 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        samples as f64 / (self.sample_rate as f64 * self.channels as f64)
    }

    pub fn current_position(&self) -> u64 {
        self.position.load(Ordering::Acquire)
    }

    pub fn is_paused(&self) -> bool {
        self.playback_state.load(Ordering::Acquire) == PlaybackState::PAUSED
    }
}

#[derive(Debug, Clone)]
pub struct PatternCueConfig {
    pub pattern_type: PatternType,
    pub color: [u8; 3],
    pub member_ids: Vec<String>,
    pub board_info: HashMap<String, BoardInfo>,
    pub bpm: f64,
    pub sync_rate: f64,
}

#[derive(Debug, Clone)]
pub enum CueType {
    Effect {
        config: EffectConfig,
        boards: Vec<BoardTarget>,
    },
    Pattern(PatternCueConfig),
}

#[derive(Debug, Clone)]
pub struct ScheduledCue {
    pub fire_at_samples: u64,
    pub label: String,
    pub cue_type: CueType,
}

pub enum SchedulerCommand {
    Start {
        cues: Vec<ScheduledCue>,
        audio_timing: AudioTimingConfig,
    },
}

pub struct CueScheduler {
    command_tx: mpsc::Sender<SchedulerCommand>,
    stop_flag: Arc<AtomicBool>,
}

impl CueScheduler {
    pub fn new(
        effects_engine: Arc<EffectsEngine>,
        pattern_engine: Arc<PatternEngine>,
        timing_metrics: Option<Arc<TimingMetrics>>,
        on_complete: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();
        let on_complete_clone = on_complete.clone();

        thread::spawn(move || {
            Self::run_scheduler(command_rx, effects_engine, pattern_engine, stop_flag_clone, timing_metrics, on_complete_clone);
        });

        Self {
            command_tx,
            stop_flag,
        }
    }

    pub fn start(&self, cues: Vec<ScheduledCue>, audio_timing: AudioTimingConfig) -> Result<(), String> {
        self.stop_flag.store(false, Ordering::Relaxed);
        self.command_tx
            .send(SchedulerCommand::Start {
                cues,
                audio_timing,
            })
            .map_err(|e| e.to_string())
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    fn find_next_cue(cues: &[ScheduledCue], pos: u64) -> Option<usize> {
        match cues.binary_search_by_key(&pos, |c| c.fire_at_samples) {
            Ok(i) => Some(i),
            Err(i) => {
                if i < cues.len() {
                    Some(i)
                } else {
                    None
                }
            }
        }
    }

    fn fire_cue(
        cue: &ScheduledCue,
        audio_timing: &AudioTimingConfig,
        effects_engine: &EffectsEngine,
        pattern_engine: &PatternEngine,
        timing_metrics: &Option<Arc<TimingMetrics>>,
    ) {
        let actual_position = audio_timing.current_position();
        let drift_samples = actual_position.saturating_sub(cue.fire_at_samples) as i64;
        let drift_ms = (drift_samples as f64 / (audio_timing.sample_rate as f64 * audio_timing.channels as f64)) * 1000.0;

        if let Some(ref metrics) = timing_metrics {
            metrics.record_cue_drift(drift_ms, &cue.label);
        }

        match &cue.cue_type {
            CueType::Pattern(pcfg) => {
                println!(
                    "🌊 PATTERN '{}' fired @ {:.2}s (drift: {:.1}ms, pos: {})",
                    cue.label,
                    audio_timing.samples_to_seconds(cue.fire_at_samples),
                    drift_ms,
                    actual_position
                );

                let _ = effects_engine.send_command(EngineCommand::Stop);

                let sequence = generate_sequence(
                    &pcfg.member_ids,
                    &pcfg.pattern_type,
                    pcfg.bpm,
                    pcfg.sync_rate,
                );

                let is_random = pcfg.pattern_type == PatternType::Random;
                let is_ping_pong = pcfg.pattern_type == PatternType::PingPong;

                let _ = pattern_engine.send_command(PatternCommand::Start {
                    sequence,
                    color: pcfg.color,
                    boards: pcfg.board_info.clone(),
                    is_random,
                    is_ping_pong,
                });
            }
            CueType::Effect { config, boards } => {
                println!(
                    "🎯 CUE '{}' fired @ {:.2}s (drift: {:.1}ms, pos: {})",
                    cue.label,
                    audio_timing.samples_to_seconds(cue.fire_at_samples),
                    drift_ms,
                    actual_position
                );

                let _ = pattern_engine.send_command(PatternCommand::Stop);

                let _ = effects_engine.send_command(EngineCommand::Start {
                    config: config.clone(),
                    boards: boards.clone(),
                });
            }
        }
    }

    fn run_scheduler(
        command_rx: mpsc::Receiver<SchedulerCommand>,
        effects_engine: Arc<EffectsEngine>,
        pattern_engine: Arc<PatternEngine>,
        stop_flag: Arc<AtomicBool>,
        timing_metrics: Option<Arc<TimingMetrics>>,
        on_complete: Option<Arc<dyn Fn() + Send + Sync>>,
    ) {
        loop {
            match command_rx.recv() {
                Ok(SchedulerCommand::Start {
                    cues,
                    audio_timing,
                }) => {
                    let mut sorted_cues = cues;
                    sorted_cues.sort_by_key(|c| c.fire_at_samples);

                    let initial_position = audio_timing.current_position();
                    println!(
                        "🎬 Cue scheduler: {} cues loaded (audio-synced @ {}Hz x {} ch, current_pos: {})",
                        sorted_cues.len(),
                        audio_timing.sample_rate,
                        audio_timing.channels,
                        initial_position
                    );

                    for (i, cue) in sorted_cues.iter().take(5).enumerate() {
                        println!(
                            "   cue[{}] '{}' @ {} samples ({:.2}s)",
                            i, cue.label, cue.fire_at_samples,
                            audio_timing.samples_to_seconds(cue.fire_at_samples)
                        );
                    }

                    let startup_start = std::time::Instant::now();
                    let mut position_started = initial_position > 0;
                    if !position_started {
                        println!("⏳ Waiting for audio position to start advancing...");
                        while !position_started && startup_start.elapsed() < STARTUP_TIMEOUT {
                            if stop_flag.load(Ordering::Relaxed) {
                                println!("⏹️ Cue scheduler: stopped during startup wait");
                                break;
                            }
                            thread::sleep(Duration::from_millis(10));
                            let pos = audio_timing.current_position();
                            if pos > 0 {
                                position_started = true;
                                println!("✅ Audio position started (pos: {})", pos);
                            }
                        }
                        if !position_started {
                            println!("⚠️ Timeout waiting for audio to start - proceeding anyway");
                        }
                    }

                    let mut last_position = audio_timing.current_position();
                    let mut last_position_change = std::time::Instant::now();
                    let mut cursor: usize = match Self::find_next_cue(&sorted_cues, last_position) {
                        Some(idx) => idx,
                        None => sorted_cues.len(),
                    };
                    if cursor > 0 {
                        println!("⏩ Starting at cursor {} (skipped {} already-passed cues)", cursor, cursor);
                    }

                    'main_loop: loop {
                        if stop_flag.load(Ordering::Relaxed) {
                            break;
                        }

                        let current_position = audio_timing.current_position();

                        if current_position != last_position {
                            let jump = if current_position > last_position {
                                current_position - last_position
                            } else {
                                last_position - current_position
                            };
                            let seek_threshold = audio_timing.sample_rate as u64 * audio_timing.channels as u64 / 2;
                            let is_seek = current_position < last_position || jump > seek_threshold;

                            if is_seek {
                                let new_cursor = match Self::find_next_cue(&sorted_cues, current_position) {
                                    Some(idx) => idx,
                                    None => sorted_cues.len(),
                                };
                                let direction = if current_position < last_position { "⏪" } else { "⏩" };
                                println!("{} Seek detected ({:.2}s → {:.2}s), cursor {} → {}",
                                    direction,
                                    audio_timing.samples_to_seconds(last_position),
                                    audio_timing.samples_to_seconds(current_position),
                                    cursor, new_cursor);
                                cursor = new_cursor;
                            }
                            last_position = current_position;
                            last_position_change = std::time::Instant::now();
                        } else if audio_timing.is_paused() {
                            last_position_change = std::time::Instant::now();
                        } else if last_position_change.elapsed() > POSITION_STALL_TIMEOUT {
                            println!("⚠️ Position stalled for {:?} - audio may have stopped", POSITION_STALL_TIMEOUT);
                            break;
                        }

                        if cursor >= sorted_cues.len() {
                            break;
                        }

                        let target_samples = sorted_cues[cursor].fire_at_samples;
                        let remaining_samples = target_samples.saturating_sub(current_position);

                        if remaining_samples > COARSE_THRESHOLD_SAMPLES {
                            thread::sleep(COARSE_SLEEP);
                            continue;
                        } else if remaining_samples > FINE_THRESHOLD_SAMPLES {
                            thread::sleep(FINE_SLEEP);
                            continue;
                        } else if remaining_samples > 0 {
                            thread::sleep(POLL_INTERVAL);
                        }

                        let spin_start = std::time::Instant::now();
                        let spin_timeout = Duration::from_millis(100);
                        while audio_timing.current_position() < target_samples {
                            std::hint::spin_loop();
                            if stop_flag.load(Ordering::Relaxed) {
                                break 'main_loop;
                            }
                            if spin_start.elapsed() > spin_timeout {
                                break;
                            }
                        }

                        if stop_flag.load(Ordering::Relaxed) {
                            break;
                        }

                        if audio_timing.current_position() >= target_samples {
                            Self::fire_cue(
                                &sorted_cues[cursor],
                                &audio_timing,
                                &effects_engine,
                                &pattern_engine,
                                &timing_metrics,
                            );
                            cursor += 1;
                        }
                    }

                    if stop_flag.load(Ordering::Relaxed) {
                        println!("⏹️ Cue scheduler: stopped");
                        continue;
                    }

                    let final_pos = audio_timing.current_position();
                    println!("✅ All cues fired @ {:.2}s - waiting for audio to end...",
                        audio_timing.samples_to_seconds(final_pos));

                    let mut last_pos = final_pos;
                    let mut last_change = std::time::Instant::now();
                    let wait_start = std::time::Instant::now();

                    loop {
                        if stop_flag.load(Ordering::Relaxed) {
                            println!("⏹️ Cue scheduler: stopped while waiting for audio end");
                            break;
                        }

                        let current_pos = audio_timing.current_position();
                        if current_pos != last_pos {
                            last_pos = current_pos;
                            last_change = std::time::Instant::now();
                        } else if audio_timing.is_paused() {
                            last_change = std::time::Instant::now();
                        } else if last_change.elapsed() > AUDIO_END_DETECTION {
                            println!("🏁 Audio ended @ {:.2}s (waited {:.1}s after last cue)",
                                audio_timing.samples_to_seconds(current_pos),
                                wait_start.elapsed().as_secs_f64());
                            break;
                        }

                        thread::sleep(COARSE_SLEEP);
                    }

                    if stop_flag.load(Ordering::Relaxed) { continue; }

                    let Some(ref callback) = on_complete else { continue; };
                    callback();
                }
                Err(_) => break,
            }
        }
    }
}
