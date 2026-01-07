use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

use crate::config::{Config, PatternType};
use crate::cue_scheduler::{CueScheduler, CueType, PatternCueConfig, ScheduledCue};
use crate::effects::EffectType;
use crate::effects_engine::{BoardTarget, EffectConfig, EffectsEngine, EngineCommand};
use crate::pattern_engine::{BoardInfo, PatternCommand, PatternEngine};
use crate::program::Program;

pub type AudioPlayCallback = Arc<dyn Fn(&str) + Send + Sync>;
pub type AudioStopCallback = Arc<dyn Fn(&str) + Send + Sync>;

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
}

#[derive(Debug, Clone)]
pub struct ActiveTarget {
    pub boards: Vec<BoardTarget>,
}

#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub audio_track: Option<String>,
    pub active_targets: Vec<ActiveTarget>,
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
        config: Arc<tokio::sync::Mutex<Config>>,
        effects_engine: Arc<EffectsEngine>,
        pattern_engine: Arc<PatternEngine>,
        performance_mode: Arc<AtomicBool>,
        on_audio_play: Option<AudioPlayCallback>,
        on_audio_stop: Option<AudioStopCallback>,
        connected_ips: Arc<RwLock<HashSet<String>>>,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel(32);
        let state = Arc::new(RwLock::new(PlaybackState {
            audio_track: None,
            active_targets: Vec::new(),
        }));

        let cue_scheduler = CueScheduler::new(effects_engine.clone(), pattern_engine.clone());

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
            on_audio_stop,
            connected_ips,
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
        config: Arc<tokio::sync::Mutex<Config>>,
        effects_engine: Arc<EffectsEngine>,
        pattern_engine: Arc<PatternEngine>,
        cue_scheduler: CueScheduler,
        state: Arc<RwLock<PlaybackState>>,
        performance_mode: Arc<AtomicBool>,
        on_audio_play: Option<AudioPlayCallback>,
        on_audio_stop: Option<AudioStopCallback>,
        connected_ips: Arc<RwLock<HashSet<String>>>,
    ) {
        loop {
            match command_rx.recv().await {
                Some(PlaybackCommand::Play {
                    program,
                    start_time,
                }) => {
                    println!("▶️ Program engine: Play {} @ {}s", program.id, start_time);

                    cue_scheduler.stop();
                    let _ = effects_engine.send_command(EngineCommand::Stop);
                    let _ = pattern_engine.send_command(PatternCommand::Stop);

                    performance_mode.store(true, Ordering::SeqCst);
                    println!("🎭 Performance mode: ON (WebSocket reconnection paused)");

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
                                let mut board_info_by_id: HashMap<String, BoardInfo> = HashMap::new();
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
                                    TargetInfo { boards, board_info_by_id, member_ids },
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

                    let playback_start = if audio_sync_delay_ms < 0 {
                        let delay_ms = audio_sync_delay_ms.unsigned_abs();
                        println!("⏱️ Audio sync: -{}ms (delaying lights)", delay_ms);
                        Instant::now() + Duration::from_millis(delay_ms)
                    } else {
                        Instant::now()
                    };

                    {
                        let mut s = state.write().await;
                        s.audio_track = Some(program.loopy_pro_track.clone());
                        s.active_targets = target_map
                            .values()
                            .map(|t| ActiveTarget {
                                boards: t.boards.clone(),
                            })
                            .collect();
                    }

                    let _ = cue_scheduler.start(scheduled_cues, playback_start);

                    if audio_sync_delay_ms > 0 {
                        println!("⏱️ Audio sync: +{}ms (delaying audio)", audio_sync_delay_ms);
                        tokio::time::sleep(Duration::from_millis(audio_sync_delay_ms as u64)).await;
                    }

                    if let Some(ref callback) = on_audio_play {
                        println!("🎵 Triggering audio playback: {}", program.loopy_pro_track);
                        callback(&program.loopy_pro_track);
                    }
                }

                Some(PlaybackCommand::Stop) => {
                    println!("⏹️ Program engine: Stop command received");

                    cue_scheduler.stop();

                    println!("  → Sending Stop to pattern engine...");
                    let _ = pattern_engine.send_command(PatternCommand::Stop);
                    println!("  ✓ Stop sent to pattern engine");

                    let (audio_track, active_targets) = {
                        let s = state.read().await;
                        (s.audio_track.clone(), s.active_targets.clone())
                    };

                    println!(
                        "  → Sending blackout to {} targets...",
                        active_targets.len()
                    );
                    for target in &active_targets {
                        send_blackout(&effects_engine, target.boards.clone());
                    }
                    println!("  ✓ Blackout sent to all targets");

                    let _ = effects_engine.send_command(EngineCommand::Stop);

                    if let (Some(ref callback), Some(ref track)) = (&on_audio_stop, &audio_track) {
                        println!("🎵 Stopping audio playback: {}", track);
                        callback(track);
                    }

                    performance_mode.store(false, Ordering::SeqCst);
                    println!("🎭 Performance mode: OFF (WebSocket reconnection resumed)");

                    {
                        let mut s = state.write().await;
                        s.audio_track = None;
                        s.active_targets.clear();
                    }
                }

                None => break,
            }
        }
    }
}
