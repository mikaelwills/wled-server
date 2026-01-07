use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::info;

use crate::effects::{Effect, EffectType};
use crate::transport::E131RawTransport;

#[derive(Debug, Clone)]
pub struct BoardTarget {
    pub ip: String,
    pub universe: u16,
    pub led_count: usize,
}

#[derive(Debug, Clone)]
pub struct EffectConfig {
    pub effect_type: EffectType,
    pub bpm: f64,
    pub color: [u8; 3],
}

#[derive(Debug)]
pub enum EngineCommand {
    Start {
        config: EffectConfig,
        boards: Vec<BoardTarget>,
    },
    Stop,
}

pub struct EffectsEngine {
    command_tx: mpsc::Sender<EngineCommand>,
}

impl EffectsEngine {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel();

        thread::spawn(move || Self::run_loop(command_rx));

        Self { command_tx }
    }

    pub fn send_command(
        &self,
        cmd: EngineCommand,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.command_tx.send(cmd)?;
        Ok(())
    }

    fn run_loop(command_rx: mpsc::Receiver<EngineCommand>) {
        let mut state: Option<EngineState> = None;
        let tick_duration = Duration::from_millis(25);

        loop {
            match command_rx.try_recv() {
                Ok(cmd) => match cmd {
                    EngineCommand::Start { config, boards } => {
                        info!(
                            effect = ?config.effect_type,
                            bpm = config.bpm,
                            boards = boards.len(),
                            "Effects engine START"
                        );
                        state = Some(EngineState::new(config, boards));
                    }
                    EngineCommand::Stop => {
                        info!("Effects engine STOP");
                        if let Some(ref mut s) = state {
                            s.blackout();
                        }
                        state = None;
                    }
                },
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => break,
            }

            if let Some(ref mut s) = state {
                s.tick();
            }

            thread::sleep(tick_duration);
        }
    }
}

struct EngineState {
    effect: Box<dyn Effect>,
    start_time: Instant,
    start_system_time: f64,
    transports: Vec<(E131RawTransport, usize)>,
    tick_count: u64,
}

impl EngineState {
    fn new(config: EffectConfig, boards: Vec<BoardTarget>) -> Self {
        let mut transports = Vec::new();

        for board in &boards {
            match E131RawTransport::new(vec![board.ip.clone()], board.universe) {
                Ok(t) => {
                    info!(
                        ip = %board.ip,
                        universe = board.universe,
                        "E1.31 transport created"
                    );
                    transports.push((t, board.led_count));
                }
                Err(e) => {
                    info!(ip = %board.ip, error = %e, "Failed to create E1.31 transport");
                }
            }
        }

        let start_system_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        let effect = config.effect_type.create(config.color, config.bpm);

        Self {
            effect,
            start_time: Instant::now(),
            start_system_time,
            transports,
            tick_count: 0,
        }
    }

    fn tick(&mut self) {
        self.tick_count += 1;
        let elapsed = self.start_time.elapsed().as_secs_f64();

        if self.tick_count % 500 == 0 {
            let system_now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            let system_elapsed = system_now - self.start_system_time;
            let clock_drift = elapsed - system_elapsed;
            info!(
                tick = self.tick_count,
                elapsed = format!("{:.3}", elapsed),
                drift = format!("{:+.4}", clock_drift),
                "Effects engine stats"
            );
        }

        for (transport, led_count) in &mut self.transports {
            self.effect.tick(elapsed, transport, *led_count);
        }
    }

    fn blackout(&mut self) {
        for (transport, led_count) in &mut self.transports {
            for _ in 0..5 {
                let _ = transport.send_raw_leds(*led_count, 0, 0, 0);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        info!(transports = self.transports.len(), "E1.31 blackout sent to all boards");
    }
}
