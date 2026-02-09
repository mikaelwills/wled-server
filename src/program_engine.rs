use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};

use crate::audio::AudioEngine;
use crate::config::{AudioSource, Config, PatternType};
use crate::cue_scheduler::{
    AudioTimingConfig, CueScheduler, CueType, PatternCueConfig, ScheduledCue,
};
use crate::effects::EffectType;
use crate::effects_engine::{BoardTarget, EffectConfig, EffectsEngine, EngineCommand};
use crate::pattern_engine::{BoardInfo, PatternCommand, PatternEngine};
use crate::playback_history::PlaybackHistory;
use crate::program::Program;
use crate::routes::send_osc_sync;
use crate::sse::SseEvent;
use crate::timing_metrics::TimingMetrics;

pub type AudioPlayCallback = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Debug, Clone)]
struct TargetInfo {
    boards: Vec<BoardTarget>,
    board_info_by_id: HashMap<String, BoardInfo>,
    member_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct PresetInfo {
    effect_type: EffectType,
    color: [u8; 3],
}

#[derive(Debug, Clone)]
struct PatternPresetInfo {
    pattern_type: PatternType,
    color: [u8; 3],
}

pub enum PlaybackCommand {
    Play { program: Program, start_time: f64 },
    Stop,
    CuesCompleted,
}

#[derive(Debug, Clone)]
pub struct ActiveTarget {
    pub boards: Vec<BoardTarget>,
}

#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub audio_track: Option<String>,
    pub active_targets: Vec<ActiveTarget>,
    pub current_session_id: Option<String>,
}

pub struct ProgramEngine {
    command_tx: mpsc::Sender<PlaybackCommand>,
}

fn send_blackout(effects_engine: &EffectsEngine, boards: Vec<BoardTarget>) {
    let _ = effects_engine.send_command(EngineCommand::Start {
        config: EffectConfig {
            effect_type: EffectType::Solid,
            bpm: 0.0,
            color: [0, 0, 0],
        },
        boards,
    });
}

impl ProgramEngine {
    pub fn new(
        config: Arc<Mutex<Config>>,
        effects_engine: Arc<EffectsEngine>,
        pattern_engine: Arc<PatternEngine>,
        performance_mode: Arc<AtomicBool>,
        on_audio_play: Option<AudioPlayCallback>,
        connected_ips: Arc<RwLock<HashSet<String>>>,
        timing_metrics: Option<Arc<TimingMetrics>>,
        playback_history: Option<Arc<PlaybackHistory>>,
        audio_engine: Option<Arc<Mutex<AudioEngine>>>,
        broadcast_tx: Arc<broadcast::Sender<SseEvent>>,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel(32);
        let state = Arc::new(RwLock::new(PlaybackState {
            audio_track: None,
            active_targets: Vec::new(),
            current_session_id: None,
        }));

        let completion_tx = command_tx.clone();
        let on_cues_complete: Option<Arc<dyn Fn() + Send + Sync>> = Some(Arc::new(move || {
            let _ = completion_tx.blocking_send(PlaybackCommand::CuesCompleted);
        }));

        let cue_scheduler = CueScheduler::new(
            effects_engine.clone(),
            pattern_engine.clone(),
            timing_metrics.clone(),
            on_cues_complete,
        );

        let state_clone = state.clone();
        let performance_mode_clone = performance_mode.clone();
        tokio::spawn(Self::run_loop(
            command_rx,
            config,
            effects_engine,
            pattern_engine,
            cue_scheduler,
            state_clone,
            performance_mode_clone,
            on_audio_play,
            connected_ips,
            timing_metrics,
            playback_history,
            audio_engine,
            broadcast_tx,
        ));

        Self { command_tx }
    }

    pub async fn play(&self, program: Program, start_time: f64) -> Result<(), String> {
        self.command_tx
            .send(PlaybackCommand::Play {
                program,
                start_time,
            })
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn stop(&self) -> Result<(), String> {
        self.command_tx
            .send(PlaybackCommand::Stop)
            .await
            .map_err(|e| e.to_string())
    }

    async fn run_loop(
        mut command_rx: mpsc::Receiver<PlaybackCommand>,
        config: Arc<Mutex<Config>>,
        effects_engine: Arc<EffectsEngine>,
        pattern_engine: Arc<PatternEngine>,
        cue_scheduler: CueScheduler,
        state: Arc<RwLock<PlaybackState>>,
        performance_mode: Arc<AtomicBool>,
        on_audio_play: Option<AudioPlayCallback>,
        connected_ips: Arc<RwLock<HashSet<String>>>,
        timing_metrics: Option<Arc<TimingMetrics>>,
        playback_history: Option<Arc<PlaybackHistory>>,
        audio_engine: Option<Arc<Mutex<AudioEngine>>>,
        broadcast_tx: Arc<broadcast::Sender<SseEvent>>,
    ) {
        let mut position_task: Option<tokio::task::JoinHandle<()>> = None;
        let mut current_program_id: Option<String> = None;

        loop {
            match command_rx.recv().await {
                Some(PlaybackCommand::Play {
                    program,
                    start_time,
                }) => {
                    let play_t0 = std::time::Instant::now();
                    println!("[TIMING] engine: Play received program={} start={}s", program.id, start_time);

                    cue_scheduler.stop();
                    let _ = effects_engine.send_command(EngineCommand::Stop);
                    let _ = pattern_engine.send_command(PatternCommand::Stop);

                    performance_mode.store(true, Ordering::SeqCst);
                    println!("🎭 Performance mode: ON (WebSocket reconnection paused)");

                    if let Some(ref metrics) = timing_metrics {
                        metrics.reset();
                    }

                    let session_id = if let Some(ref history) = playback_history {
                        let id = history.start_session(&program.id, &program.song_name);
                        println!("📊 Started playback session: {}", id);
                        Some(id)
                    } else {
                        None
                    };

                    let bpm = program.bpm.unwrap_or(120) as f64;

                    let (target_map, scheduled_cues, audio_sync_delay_ms) = {
                        let cfg = config.lock().await;
                        let audio_sync_delay_ms = cfg.loopy_pro.audio_sync_delay_ms;
                        let online_ips = connected_ips.read().await;

                        let unique_targets: HashSet<String> = program
                            .cues
                            .iter()
                            .flat_map(|c| c.targets.iter().cloned())
                            .collect();

                        let mut target_map: HashMap<String, TargetInfo> = HashMap::new();
                        for target in &unique_targets {
                            let target_boards = cfg.get_target_boards(target);
                            if !target_boards.is_empty() {
                                let mut boards: Vec<BoardTarget> = Vec::new();
                                let mut board_info_by_id: HashMap<String, BoardInfo> =
                                    HashMap::new();
                                let mut member_ids: Vec<String> = Vec::new();

                                for b in &target_boards {
                                    if online_ips.contains(&b.ip) {
                                        boards.push(BoardTarget {
                                            ip: b.ip.clone(),
                                            universe: b.universe.unwrap_or(1),
                                            led_count: b.led_count.unwrap_or(60) as usize,
                                        });
                                        board_info_by_id.insert(
                                            b.id.clone(),
                                            BoardInfo {
                                                ip: b.ip.clone(),
                                                universe: b.universe.unwrap_or(1),
                                                led_count: b.led_count.unwrap_or(60) as usize,
                                            },
                                        );
                                        member_ids.push(b.id.clone());
                                    }
                                }

                                if boards.is_empty() {
                                    println!(
                                        "⚠️ Target '{}' has no online boards (0/{} online)",
                                        target,
                                        target_boards.len()
                                    );
                                    continue;
                                }
                                println!(
                                    "🎯 Target '{}': {}/{} boards online",
                                    target,
                                    boards.len(),
                                    target_boards.len()
                                );
                                target_map.insert(
                                    target.clone(),
                                    TargetInfo {
                                        boards,
                                        board_info_by_id,
                                        member_ids,
                                    },
                                );
                            }
                        }

                        let mut preset_map: HashMap<String, PresetInfo> = HashMap::new();
                        for preset in &cfg.effect_presets {
                            let effect_type = match preset.effect_type.parse::<EffectType>() {
                                Ok(t) => t,
                                Err(_) => continue,
                            };
                            preset_map.insert(
                                preset.name.clone(),
                                PresetInfo {
                                    effect_type,
                                    color: preset.color,
                                },
                            );
                        }

                        let mut pattern_preset_map: HashMap<String, PatternPresetInfo> =
                            HashMap::new();
                        for preset in &cfg.pattern_presets {
                            pattern_preset_map.insert(
                                preset.name.clone(),
                                PatternPresetInfo {
                                    pattern_type: preset.pattern.clone(),
                                    color: preset.colour,
                                },
                            );
                        }

                        let mut scheduled_cues: Vec<ScheduledCue> = Vec::new();
                        for cue in program.cues.iter().filter(|c| c.time >= start_time) {
                            let preset_name = &cue.preset_name;
                            let fire_at = Duration::from_secs_f64((cue.time - start_time).max(0.0));

                            if cue.targets.is_empty() {
                                eprintln!("⚠️ Skipping cue '{}': no targets", cue.label);
                                continue;
                            }

                            for target in &cue.targets {
                                if let Some(pattern_preset) = pattern_preset_map.get(preset_name) {
                                    let target_info = match target_map.get(target) {
                                        Some(t) => t,
                                        None => {
                                            eprintln!(
                                                "⚠️ Skipping pattern cue '{}': target '{}' not found or offline",
                                                cue.label, target
                                            );
                                            continue;
                                        }
                                    };

                                    scheduled_cues.push(ScheduledCue {
                                        fire_at,
                                        label: cue.label.clone(),
                                        cue_type: CueType::Pattern(PatternCueConfig {
                                            pattern_type: pattern_preset.pattern_type.clone(),
                                            color: pattern_preset.color,
                                            member_ids: target_info.member_ids.clone(),
                                            board_info: target_info.board_info_by_id.clone(),
                                            bpm,
                                            sync_rate: cue.sync_rate,
                                        }),
                                    });
                                } else if let Some(preset) = preset_map.get(preset_name) {
                                    let target_info = match target_map.get(target) {
                                        Some(t) => t,
                                        None => {
                                            eprintln!(
                                                "⚠️ Skipping cue '{}': target '{}' not found",
                                                cue.label, target
                                            );
                                            continue;
                                        }
                                    };

                                    let effective_bpm = bpm * cue.sync_rate;

                                    scheduled_cues.push(ScheduledCue {
                                        fire_at,
                                        label: cue.label.clone(),
                                        cue_type: CueType::Effect {
                                            config: EffectConfig {
                                                effect_type: preset.effect_type,
                                                bpm: effective_bpm,
                                                color: preset.color,
                                            },
                                            boards: target_info.boards.clone(),
                                        },
                                    });
                                } else {
                                    eprintln!("⚠️ Skipping cue '{}': preset '{}' not found in effects or patterns", cue.label, preset_name);
                                }
                            }
                        }

                        (target_map, scheduled_cues, audio_sync_delay_ms)
                    };

                    println!(
                        "🔌 Sending Off to {} targets before playback",
                        target_map.len()
                    );
                    for (_, target_info) in &target_map {
                        send_blackout(&effects_engine, target_info.boards.clone());
                    }

                    println!(
                        "📍 Scheduling {} cues from {}s",
                        scheduled_cues.len(),
                        start_time
                    );

                    {
                        let mut s = state.write().await;
                        s.audio_track = Some(program.loopy_pro_track.clone());
                        s.active_targets = target_map
                            .values()
                            .map(|t| ActiveTarget {
                                boards: t.boards.clone(),
                            })
                            .collect();
                        s.current_session_id = session_id.clone();
                    }

                    let audio_source = {
                        let cfg = config.lock().await;
                        cfg.loopy_pro.audio_source.clone()
                    };

                    let mut broadcast_position: Option<Arc<AtomicU64>> = None;
                    let mut broadcast_rate: u32 = 44100;
                    let mut broadcast_channels: u32 = 2;

                    println!("[TIMING] engine: pre-audio dt={:.1}ms", play_t0.elapsed().as_secs_f64() * 1000.0);

                    match audio_source {
                        AudioSource::LoopyPro => {
                            let simulated_position = Arc::new(AtomicU64::new(0));
                            let simulated_sample_rate = 44100u32;
                            let simulated_channels = 2u32;

                            let light_delay_samples = if audio_sync_delay_ms < 0 {
                                let delay_ms = audio_sync_delay_ms.unsigned_abs();
                                println!("⏱️ Audio sync: -{}ms (delaying lights)", delay_ms);
                                (delay_ms as f64 / 1000.0
                                    * simulated_sample_rate as f64
                                    * simulated_channels as f64)
                                    as u64
                            } else {
                                0
                            };

                            if audio_sync_delay_ms > 0 {
                                println!(
                                    "⏱️ Audio sync: +{}ms (delaying audio)",
                                    audio_sync_delay_ms
                                );
                                tokio::time::sleep(Duration::from_millis(
                                    audio_sync_delay_ms as u64,
                                ))
                                .await;
                            }

                            let perf_mode_clone = performance_mode.clone();
                            let position_clone = simulated_position.clone();
                            std::thread::spawn(move || {
                                let start = std::time::Instant::now();
                                let samples_per_second =
                                    (simulated_sample_rate * simulated_channels) as f64;
                                while perf_mode_clone.load(Ordering::Relaxed) {
                                    let elapsed_secs = start.elapsed().as_secs_f64();
                                    let current_sample = (elapsed_secs * samples_per_second) as u64;
                                    position_clone.store(current_sample, Ordering::Release);
                                    std::thread::sleep(Duration::from_millis(1));
                                }
                            });

                            std::thread::sleep(Duration::from_millis(5));

                            let audio_timing = AudioTimingConfig {
                                position: simulated_position.clone(),
                                sample_rate: simulated_sample_rate,
                                channels: simulated_channels,
                                start_sample: light_delay_samples,
                            };

                            let _ = cue_scheduler.start(scheduled_cues, audio_timing);

                            if let Some(ref callback) = on_audio_play {
                                println!(
                                    "🎵 Triggering Loopy Pro playback: {}",
                                    program.loopy_pro_track
                                );
                                callback(&program.loopy_pro_track);
                            }

                            broadcast_position = Some(simulated_position.clone());
                            broadcast_rate = simulated_sample_rate;
                            broadcast_channels = simulated_channels;
                        }
                        AudioSource::AudioEngine => {
                            if let Some(ref engine) = audio_engine {
                                let track_id = &program.id;
                                let guide_id = program.guide_audio_file.as_ref().and_then(|f| {
                                    std::path::Path::new(f)
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .map(|s| s.to_string())
                                });
                                println!("[TIMING] engine: acquiring engine lock dt={:.1}ms", play_t0.elapsed().as_secs_f64() * 1000.0);
                                let mut eng = engine.lock().await;
                                println!("[TIMING] engine: lock acquired dt={:.1}ms", play_t0.elapsed().as_secs_f64() * 1000.0);

                                if let Some(bpm) = program.bpm {
                                    if let Some(track) = eng.get_track(track_id) {
                                        let grid_offset = program.grid_offset.unwrap_or(0.0);
                                        let click_rate = program.click_rate.unwrap_or(1.0);
                                        eng.generate_and_load_click(
                                            track_id,
                                            bpm as f64,
                                            grid_offset,
                                            track.duration_secs,
                                            click_rate,
                                        )
                                        .await;
                                        println!("[TIMING] engine: click generated dt={:.1}ms", play_t0.elapsed().as_secs_f64() * 1000.0);
                                    }
                                }

                                let device_rate = eng.get_device_sample_rate();
                                let track_info = eng.get_track(track_id);
                                let (playback_rate, channels) = track_info
                                    .map(|t| {
                                        let rate = if device_rate > 0 {
                                            device_rate
                                        } else {
                                            t.original_rate
                                        };
                                        (rate, t.channels as u32)
                                    })
                                    .unwrap_or((44100, 2));

                                let start_sample = if start_time > 0.0 {
                                    (start_time * playback_rate as f64 * channels as f64) as u64
                                } else {
                                    0
                                };

                                let light_delay_samples = if audio_sync_delay_ms < 0 {
                                    let delay_ms = audio_sync_delay_ms.unsigned_abs();
                                    println!("⏱️ Audio sync: -{}ms (delaying lights)", delay_ms);
                                    (delay_ms as f64 / 1000.0
                                        * playback_rate as f64
                                        * channels as f64)
                                        as u64
                                } else {
                                    0
                                };

                                let position_arc = eng.get_position_arc();
                                position_arc.store(0, Ordering::SeqCst);

                                let audio_timing = AudioTimingConfig {
                                    position: position_arc,
                                    sample_rate: playback_rate,
                                    channels,
                                    start_sample: start_sample + light_delay_samples,
                                };

                                let _ = cue_scheduler.start(scheduled_cues, audio_timing);

                                if audio_sync_delay_ms > 0 {
                                    println!(
                                        "⏱️ Audio sync: +{}ms (delaying audio)",
                                        audio_sync_delay_ms
                                    );
                                    tokio::time::sleep(Duration::from_millis(
                                        audio_sync_delay_ms as u64,
                                    ))
                                    .await;
                                }

                                let start_sample_opt = if start_sample > 0 {
                                    Some(start_sample)
                                } else {
                                    None
                                };
                                println!("[TIMING] engine: pre-play_with_guide dt={:.1}ms", play_t0.elapsed().as_secs_f64() * 1000.0);
                                if eng
                                    .play_with_guide(
                                        track_id,
                                        guide_id.as_deref(),
                                        start_sample_opt,
                                    )
                                    .await
                                {
                                    println!("[TIMING] engine: play_with_guide returned dt={:.1}ms", play_t0.elapsed().as_secs_f64() * 1000.0);
                                    if guide_id.is_some() {
                                        println!("🔊 Playing audio + guide via local engine: {} @ {:?} samples", track_id, start_sample_opt);
                                    } else {
                                        println!(
                                            "🔊 Playing audio via local engine: {} @ {:?} samples",
                                            track_id, start_sample_opt
                                        );
                                    }
                                } else {
                                    println!("⚠️ Track not loaded in audio engine: {}", track_id);
                                    cue_scheduler.stop();
                                }

                                  broadcast_position = Some(eng.get_position_arc());
                            broadcast_rate = playback_rate;
                            broadcast_channels = channels;

                            }
                        }
                       
                    }

                    println!("[TIMING] engine: post-audio dt={:.1}ms", play_t0.elapsed().as_secs_f64() * 1000.0);

                    if let Some(handle) = position_task.take() {
                        handle.abort();
                    }

                    current_program_id = Some(program.id.clone());
                    let duration_secs = program.audio_duration.unwrap_or(0.0);

                    let _ = broadcast_tx.send(SseEvent::PlaybackStarted {
                        program_id: program.id.clone(),
                        duration_secs,
                    });

                    if let Some(pos_arc) = broadcast_position {
                        let pos_tx = broadcast_tx.clone();
                        let pos_id = program.id.clone();
                        let pos_flag = performance_mode.clone();
                        position_task = Some(tokio::spawn(async move {
                            let mut interval = tokio::time::interval(Duration::from_millis(100));
                            loop {
                                interval.tick().await;
                                if !pos_flag.load(Ordering::Relaxed) {
                                    break;
                                }
                                let samples = pos_arc.load(Ordering::Acquire);
                                let position_secs = samples as f64 / (broadcast_rate as f64 * broadcast_channels as f64);
                                let _ = pos_tx.send(SseEvent::PlaybackPosition {
                                    program_id: pos_id.clone(),
                                    position_secs,
                                    duration_secs,
                                });
                            }
                        }));
                    }

                    println!("[TIMING] engine: Play fully done dt={:.1}ms", play_t0.elapsed().as_secs_f64() * 1000.0);
                }

                Some(PlaybackCommand::Stop) => {
                    println!("⏹️ Program engine: Stop command received");

                    if let Some(handle) = position_task.take() {
                        handle.abort();
                    }
                    if let Some(ref pid) = current_program_id {
                        let _ = broadcast_tx.send(SseEvent::PlaybackStopped {
                            program_id: pid.clone(),
                            reason: "manual".to_string(),
                        });
                    }
                    current_program_id = None;

                    cue_scheduler.stop();

                    println!("  → Sending Stop to pattern engine...");
                    let _ = pattern_engine.send_command(PatternCommand::Stop);
                    println!("  ✓ Stop sent to pattern engine");

                    let (active_targets, session_id, audio_track) = {
                        let s = state.read().await;
                        (
                            s.active_targets.clone(),
                            s.current_session_id.clone(),
                            s.audio_track.clone(),
                        )
                    };

                    let audio_source = {
                        let cfg = config.lock().await;
                        cfg.loopy_pro.audio_source.clone()
                    };

                    match audio_source {
                        AudioSource::LoopyPro => {
                            if let Some(track) = &audio_track {
                                let cfg = config.lock().await;
                                let loopy = &cfg.loopy_pro;
                                let stop_address = format!("/Stop/0:{}", track);
                                println!(
                                    "  → Sending OSC stop to Loopy Pro ({}:{}) - {}",
                                    loopy.ip, loopy.port, stop_address
                                );
                                if let Err(e) = send_osc_sync(&loopy.ip, loopy.port, &stop_address)
                                {
                                    eprintln!("  ✗ Failed to send OSC stop: {}", e);
                                } else {
                                    println!("  ✓ OSC stop sent to Loopy Pro");
                                }
                            }
                        }
                        AudioSource::AudioEngine => {
                            if let Some(ref engine) = audio_engine {
                                let mut eng = engine.lock().await;
                                eng.stop().await;
                                println!("  ✓ Stopped local audio engine");
                            }
                        }
                    }

                    if let (Some(ref history), Some(ref sid), Some(ref metrics)) =
                        (&playback_history, &session_id, &timing_metrics)
                    {
                        let snapshot = metrics.snapshot();
                        history.end_session(sid, &snapshot, false);
                        println!("📊 Ended playback session: {}", sid);
                    }

                    println!(
                        "  → Sending blackout to {} targets...",
                        active_targets.len()
                    );
                    for target in &active_targets {
                        send_blackout(&effects_engine, target.boards.clone());
                    }
                    println!("  ✓ Blackout sent to all targets");

                    let _ = effects_engine.send_command(EngineCommand::Stop);

                    performance_mode.store(false, Ordering::SeqCst);
                    println!("🎭 Performance mode: OFF (WebSocket reconnection resumed)");

                    {
                        let mut s = state.write().await;
                        s.audio_track = None;
                        s.active_targets.clear();
                        s.current_session_id = None;
                    }
                }

                Some(PlaybackCommand::CuesCompleted) => {
                    println!("✅ Program engine: All cues completed naturally");

                    if let Some(handle) = position_task.take() {
                        handle.abort();
                    }
                    if let Some(ref pid) = current_program_id {
                        let _ = broadcast_tx.send(SseEvent::PlaybackStopped {
                            program_id: pid.clone(),
                            reason: "completed".to_string(),
                        });
                    }
                    current_program_id = None;

                    let _ = pattern_engine.send_command(PatternCommand::Stop);

                    let (active_targets, session_id) = {
                        let s = state.read().await;
                        (s.active_targets.clone(), s.current_session_id.clone())
                    };

                    if let (Some(ref history), Some(ref sid), Some(ref metrics)) =
                        (&playback_history, &session_id, &timing_metrics)
                    {
                        let snapshot = metrics.snapshot();
                        history.end_session(sid, &snapshot, true);
                        println!("📊 Ended playback session (completed): {}", sid);
                    }

                    for target in &active_targets {
                        send_blackout(&effects_engine, target.boards.clone());
                    }

                    let _ = effects_engine.send_command(EngineCommand::Stop);

                    performance_mode.store(false, Ordering::SeqCst);
                    println!("🎭 Performance mode: OFF (WebSocket reconnection resumed)");

                    {
                        let mut s = state.write().await;
                        s.active_targets.clear();
                        s.current_session_id = None;
                    }
                }

                None => break,
            }
        }
    }
}
