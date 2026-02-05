use std::net::UdpSocket;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tracing::info;

use crate::effects::{Effect, EffectType};
use crate::timing_metrics::TimingMetrics;
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
    pub fn new(timing_metrics: Option<Arc<TimingMetrics>>) -> Self {
        let (command_tx, command_rx) = mpsc::channel();

        thread::spawn(move || Self::run_loop(command_rx, timing_metrics));

        Self { command_tx }
    }

    pub fn send_command(
        &self,
        cmd: EngineCommand,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.command_tx.send(cmd)?;
        Ok(())
    }

    fn run_loop(
        command_rx: mpsc::Receiver<EngineCommand>,
        timing_metrics: Option<Arc<TimingMetrics>>,
    ) {
        let mut state: Option<EngineState> = None;
        let tick_duration = Duration::from_micros(16_667);
        let spin_threshold = Duration::from_millis(2);
        let mut next_tick = Instant::now() + tick_duration;

        let shared_socket = Arc::new(
            E131RawTransport::create_socket()
                .unwrap_or_else(|_| UdpSocket::bind("0.0.0.0:0").expect("socket bind")),
        );

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
                        state = Some(EngineState::new(
                            config,
                            shared_socket.clone(),
                            boards,
                            timing_metrics.clone(),
                        ));
                        next_tick = Instant::now() + tick_duration;
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
                let tick_start = Instant::now();
                s.tick();
                let work_ms = tick_start.elapsed().as_secs_f64() * 1000.0;

                if let Some(ref metrics) = timing_metrics {
                    metrics.record_frame_tick(work_ms);
                }
            }

            let now = Instant::now();
            if next_tick > now {
                let remaining = next_tick - now;
                if remaining > spin_threshold {
                    thread::sleep(remaining - spin_threshold);
                }
                while Instant::now() < next_tick {
                    std::hint::spin_loop();
                }
            }
            next_tick += tick_duration;
        }
    }
}

struct EngineState {
    effect: Box<dyn Effect>,
    transports: Vec<(E131RawTransport, usize)>,
    tick_count: u64,
    start_time: Instant,
}

impl EngineState {
    fn new(
        config: EffectConfig,
        socket: Arc<UdpSocket>,
        boards: Vec<BoardTarget>,
        timing_metrics: Option<Arc<TimingMetrics>>,
    ) -> Self {
        let mut transports = Vec::new();

        for board in &boards {
            match E131RawTransport::with_socket(
                socket.clone(),
                std::slice::from_ref(&board.ip),
                board.universe,
            ) {
                Ok(mut t) => {
                    info!(
                        ip = %board.ip,
                        universe = board.universe,
                        "E1.31 transport created (shared socket)"
                    );
                    if let Some(ref metrics) = timing_metrics {
                        t.set_timing_metrics(metrics.clone());
                    }
                    transports.push((t, board.led_count));
                }
                Err(e) => {
                    info!(ip = %board.ip, error = %e, "Failed to create E1.31 transport");
                }
            }
        }

        let effect = config.effect_type.create(config.color, config.bpm);

        Self {
            effect,
            transports,
            tick_count: 0,
            start_time: Instant::now(),
        }
    }

    fn tick(&mut self) {
        self.tick_count += 1;
        let elapsed = self.start_time.elapsed().as_secs_f64();

        for (transport, led_count) in &mut self.transports {
            self.effect.tick(elapsed, transport, *led_count);
        }
    }

    fn blackout(&mut self) {
        for (transport, led_count) in &mut self.transports {
            let _ = transport.send_raw_leds(*led_count, 0, 0, 0);
        }
    }
}
