use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use rand::Rng;
use tracing::info;

use crate::pattern::PatternSequence;
use crate::transport::E131RawTransport;

#[derive(Debug, Clone)]
pub struct BoardInfo {
    pub ip: String,
    pub universe: u16,
    pub led_count: usize,
}

struct PatternState {
    sequence: PatternSequence,
    color: [u8; 3],
    transports: HashMap<String, (E131RawTransport, usize)>,
    cycle_count: u64,
    is_random: bool,
    is_ping_pong: bool,
    prev_chosen: Option<String>,
}

#[derive(Debug)]
pub enum PatternCommand {
    Start {
        sequence: PatternSequence,
        color: [u8; 3],
        boards: HashMap<String, BoardInfo>,
        is_random: bool,
        is_ping_pong: bool,
    },
    Stop,
}

pub struct PatternEngine {
    command_tx: mpsc::Sender<PatternCommand>,
}

impl PatternEngine {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        thread::spawn(move || Self::run_loop(command_rx));
        Self { command_tx }
    }

    pub fn send_command(
        &self,
        cmd: PatternCommand,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.command_tx.send(cmd)?;
        Ok(())
    }

    fn run_loop(command_rx: mpsc::Receiver<PatternCommand>) {
        let mut active: Option<PatternState> = None;

        loop {
            match command_rx.try_recv() {
                Ok(PatternCommand::Start { sequence, color, boards, is_random, is_ping_pong }) => {
                    let mut transports = HashMap::new();
                    for (board_id, info) in boards {
                        if let Ok(transport) = E131RawTransport::new(vec![info.ip.clone()], info.universe) {
                            transports.insert(board_id, (transport, info.led_count));
                        }
                    }
                    if !transports.is_empty() {
                        active = Some(PatternState {
                            sequence,
                            color,
                            transports,
                            cycle_count: 0,
                            is_random,
                            is_ping_pong,
                            prev_chosen: None,
                        });
                    }
                }
                Ok(PatternCommand::Stop) => {
                    if let Some(ref mut state) = active {
                        for (transport, led_count) in state.transports.values_mut() {
                            let _ = transport.send_raw_leds(*led_count, 0, 0, 0);
                        }
                    }
                    active = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => break,
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if let Some(ref mut state) = active {
                let stopped = if state.is_random {
                    Self::run_random_beat(&state.sequence, state.color, &mut state.transports, &command_rx, &mut state.prev_chosen)
                } else {
                    Self::run_one_cycle(&state.sequence, state.color, &mut state.transports, &command_rx, state.cycle_count, state.is_ping_pong)
                };
                if stopped {
                    for (transport, led_count) in state.transports.values_mut() {
                        let _ = transport.send_raw_leds(*led_count, 0, 0, 0);
                    }
                    active = None;
                } else {
                    state.cycle_count += 1;
                }
            } else {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    fn run_random_beat(
        seq: &PatternSequence,
        color: [u8; 3],
        transports: &mut HashMap<String, (E131RawTransport, usize)>,
        command_rx: &mpsc::Receiver<PatternCommand>,
        prev_chosen: &mut Option<String>,
    ) -> bool {
        const FLASH_DURATION_MS: u64 = 120;
        const FRAME_MS: u64 = 20;

        let board_ids: Vec<String> = transports.keys().cloned().collect();
        if board_ids.is_empty() {
            return false;
        }

        let beat_duration_ms = seq.total_duration_ms;
        let beat_start = Instant::now();

        let available: Vec<&String> = board_ids.iter()
            .filter(|id| Some((*id).clone()) != *prev_chosen)
            .collect();

        let chosen = if available.is_empty() {
            board_ids[0].clone()
        } else {
            let idx = rand::rng().random_range(0..available.len());
            available[idx].clone()
        };

        for (transport, led_count) in transports.values_mut() {
            let _ = transport.send_raw_leds(*led_count, 0, 0, 0);
        }

        if let Some((transport, led_count)) = transports.get_mut(&chosen) {
            let _ = transport.send_raw_leds(*led_count, color[0], color[1], color[2]);
        }

        let fade_frames = FLASH_DURATION_MS / FRAME_MS;
        for frame in 1..=fade_frames {
            thread::sleep(Duration::from_millis(FRAME_MS));

            if let Ok(PatternCommand::Stop) = command_rx.try_recv() {
                return true;
            }

            let brightness = 1.0 - (frame as f64 / fade_frames as f64);
            let r = (color[0] as f64 * brightness) as u8;
            let g = (color[1] as f64 * brightness) as u8;
            let b = (color[2] as f64 * brightness) as u8;

            if let Some((transport, led_count)) = transports.get_mut(&chosen) {
                let _ = transport.send_raw_leds(*led_count, r, g, b);
            }
        }

        if let Some((transport, led_count)) = transports.get_mut(&chosen) {
            let _ = transport.send_raw_leds(*led_count, 0, 0, 0);
        }

        *prev_chosen = Some(chosen);

        let end_time = beat_start + Duration::from_millis(beat_duration_ms);
        while Instant::now() < end_time {
            if let Ok(PatternCommand::Stop) = command_rx.try_recv() {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }

        false
    }

    fn run_one_cycle(
        seq: &PatternSequence,
        color: [u8; 3],
        transports: &mut HashMap<String, (E131RawTransport, usize)>,
        command_rx: &mpsc::Receiver<PatternCommand>,
        cycle_count: u64,
        is_ping_pong: bool,
    ) -> bool {
        const WAVE_PORTION: f64 = 0.5;
        const TRAIL_BRIGHTNESS: [f64; 3] = [1.0, 0.4, 0.1];

        for (transport, led_count) in transports.values_mut() {
            let _ = transport.send_raw_leds(*led_count, 0, 0, 0);
        }

        let cycle_start = Instant::now();
        let total_ms = seq.total_duration_ms as f64;
        let wave_duration_ms = total_ms * WAVE_PORTION;
        let num_steps = seq.steps.len();

        let step_interval_ms = if num_steps > 1 {
            let max_step_ms = wave_duration_ms / (num_steps as f64);
            let mut subdivision_ms = total_ms;
            let mut division = 1;
            while subdivision_ms > max_step_ms && subdivision_ms > 1.0 {
                subdivision_ms /= 2.0;
                division *= 2;
            }
            info!(
                beat_ms = total_ms as u64,
                division = division,
                step_ms = subdivision_ms as u64,
                boards = num_steps,
                "Pattern wave timing"
            );
            subdivision_ms
        } else {
            0.0
        };

        let mut trail: Vec<Vec<String>> = Vec::new();

        let reverse_direction = is_ping_pong && cycle_count % 2 == 1;
        let steps: Vec<_> = if reverse_direction {
            seq.steps.iter().rev().collect()
        } else {
            seq.steps.iter().collect()
        };

        for (step_idx, step) in steps.iter().enumerate() {
            let step_delay_ms = (step_idx as f64 * step_interval_ms) as u64;
            let target_time = cycle_start + Duration::from_millis(step_delay_ms);

            loop {
                let now = Instant::now();
                if now >= target_time {
                    break;
                }
                let remaining = target_time - now;

                if remaining > Duration::from_millis(10) {
                    if let Ok(PatternCommand::Stop) = command_rx.try_recv() {
                        return true;
                    }
                    thread::sleep(Duration::from_millis(5));
                } else if remaining > Duration::from_millis(1) {
                    thread::sleep(Duration::from_micros(500));
                } else {
                    std::hint::spin_loop();
                }
            }

            trail.insert(0, step.board_ids.clone());
            if trail.len() > TRAIL_BRIGHTNESS.len() {
                trail.pop();
            }

            for (trail_idx, boards) in trail.iter().enumerate() {
                let brightness = TRAIL_BRIGHTNESS[trail_idx];
                let r = (color[0] as f64 * brightness) as u8;
                let g = (color[1] as f64 * brightness) as u8;
                let b = (color[2] as f64 * brightness) as u8;

                for board_id in boards {
                    if let Some((transport, led_count)) = transports.get_mut(board_id) {
                        let _ = transport.send_raw_leds(*led_count, r, g, b);
                    }
                }
            }

        }

        for fade_step in 0..3 {
            thread::sleep(Duration::from_millis(30));
            let fade_mult = 0.6_f64.powi(fade_step + 1);

            for (trail_idx, boards) in trail.iter().enumerate() {
                let brightness = TRAIL_BRIGHTNESS.get(trail_idx).unwrap_or(&0.0) * fade_mult;
                let r = (color[0] as f64 * brightness) as u8;
                let g = (color[1] as f64 * brightness) as u8;
                let b = (color[2] as f64 * brightness) as u8;

                for board_id in boards {
                    if let Some((transport, led_count)) = transports.get_mut(board_id) {
                        let _ = transport.send_raw_leds(*led_count, r, g, b);
                    }
                }
            }
        }

        for (transport, led_count) in transports.values_mut() {
            let _ = transport.send_raw_leds(*led_count, 0, 0, 0);
        }

        let end_time = cycle_start + Duration::from_millis(seq.total_duration_ms);
        while Instant::now() < end_time {
            if let Ok(PatternCommand::Stop) = command_rx.try_recv() {
                return true;
            }
            thread::sleep(Duration::from_millis(1));
        }
        false
    }
}
