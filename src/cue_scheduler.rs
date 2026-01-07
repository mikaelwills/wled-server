use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::PatternType;
use crate::effects_engine::{BoardTarget, EffectConfig, EffectsEngine, EngineCommand};
use crate::pattern::generate_sequence;
use crate::pattern_engine::{BoardInfo, PatternCommand, PatternEngine};

const COARSE_THRESHOLD: Duration = Duration::from_millis(100);
const FINE_THRESHOLD: Duration = Duration::from_millis(10);
const COARSE_SLEEP: Duration = Duration::from_millis(50);
const FINE_SLEEP: Duration = Duration::from_millis(5);

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
    pub fire_at: Duration,
    pub label: String,
    pub cue_type: CueType,
}

pub enum SchedulerCommand {
    Start {
        cues: Vec<ScheduledCue>,
        playback_start: Instant,
    },
}

pub struct CueScheduler {
    command_tx: mpsc::Sender<SchedulerCommand>,
    stop_flag: Arc<AtomicBool>,
}

impl CueScheduler {
    pub fn new(effects_engine: Arc<EffectsEngine>, pattern_engine: Arc<PatternEngine>) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        thread::spawn(move || {
            Self::run_scheduler(command_rx, effects_engine, pattern_engine, stop_flag_clone);
        });

        Self {
            command_tx,
            stop_flag,
        }
    }

    pub fn start(&self, cues: Vec<ScheduledCue>, playback_start: Instant) -> Result<(), String> {
        self.stop_flag.store(false, Ordering::Relaxed);
        self.command_tx
            .send(SchedulerCommand::Start {
                cues,
                playback_start,
            })
            .map_err(|e| e.to_string())
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    fn run_scheduler(
        command_rx: mpsc::Receiver<SchedulerCommand>,
        effects_engine: Arc<EffectsEngine>,
        pattern_engine: Arc<PatternEngine>,
        stop_flag: Arc<AtomicBool>,
    ) {
        loop {
            match command_rx.recv() {
                Ok(SchedulerCommand::Start {
                    cues,
                    playback_start,
                }) => {
                    let mut sorted_cues = cues;
                    sorted_cues.sort_by_key(|c| c.fire_at);

                    println!(
                        "🎬 Cue scheduler: {} cues loaded (anchor age: {:.1}ms)",
                        sorted_cues.len(),
                        playback_start.elapsed().as_secs_f64() * 1000.0
                    );

                    'cue_loop: for cue in sorted_cues {
                        if stop_flag.load(Ordering::Relaxed) {
                            break;
                        }

                        let target_time = playback_start + cue.fire_at;

                        loop {
                            let now = Instant::now();
                            if now >= target_time {
                                break;
                            }

                            let remaining = target_time - now;

                            if remaining > COARSE_THRESHOLD {
                                thread::sleep(COARSE_SLEEP);
                                if stop_flag.load(Ordering::Relaxed) {
                                    break 'cue_loop;
                                }
                            } else if remaining > FINE_THRESHOLD {
                                thread::sleep(FINE_SLEEP);
                                if stop_flag.load(Ordering::Relaxed) {
                                    break 'cue_loop;
                                }
                            } else {
                                break;
                            }
                        }

                        while Instant::now() < target_time {
                            std::hint::spin_loop();
                        }

                        if stop_flag.load(Ordering::Relaxed) {
                            break 'cue_loop;
                        }

                        let drift_ms = (Instant::now() - target_time).as_secs_f64() * 1000.0;

                        match &cue.cue_type {
                            CueType::Pattern(pcfg) => {
                                println!(
                                    "🌊 PATTERN '{}' fired @ {:.2}s (drift: {:.1}ms)",
                                    cue.label,
                                    cue.fire_at.as_secs_f64(),
                                    drift_ms
                                );

                                let _ = effects_engine.send_command(EngineCommand::Stop);

                                let sequence = generate_sequence(
                                    &pcfg.member_ids,
                                    &pcfg.pattern_type,
                                    pcfg.bpm,
                                    pcfg.sync_rate,
                                );

                                let is_random = pcfg.pattern_type == PatternType::Random;

                                let _ = pattern_engine.send_command(PatternCommand::Start {
                                    sequence,
                                    color: pcfg.color,
                                    boards: pcfg.board_info.clone(),
                                    is_random,
                                });
                            }
                            CueType::Effect { config, boards } => {
                                println!(
                                    "🎯 CUE '{}' fired @ {:.2}s (drift: {:.1}ms)",
                                    cue.label,
                                    cue.fire_at.as_secs_f64(),
                                    drift_ms
                                );

                                let _ = pattern_engine.send_command(PatternCommand::Stop);

                                let _ = effects_engine.send_command(EngineCommand::Start {
                                    config: config.clone(),
                                    boards: boards.clone(),
                                });
                            }
                        }
                    }

                    if stop_flag.load(Ordering::Relaxed) {
                        println!("⏹️ Cue scheduler: stopped");
                    } else {
                        println!("✅ All cues fired");
                    }
                }
                Err(_) => break,
            }
        }
    }
}
