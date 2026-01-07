use std::collections::HashMap;
use crate::effects::Effect;
use crate::transport::E131RawTransport;
use rand::Rng;

#[derive(Default)]
struct LightningState {
    flash_count: u8,
    total_flashes: u8,
    flash_start: usize,
    base_len: usize,
    max_len: usize,
    flash_on: bool,
    next_event_time: f64,
    after_leader: bool,
}

pub struct Lightning {
    color: [u8; 3],
    beat_duration: f64,
    states: HashMap<u16, LightningState>,
}

impl Lightning {
    pub fn new(color: [u8; 3], bpm: f64) -> Self {
        Self {
            color,
            beat_duration: 60.0 / bpm.max(1.0),
            states: HashMap::new(),
        }
    }

    fn calculate_pulse(&self, elapsed: f64) -> f64 {
        let beat_progress = (elapsed % self.beat_duration) / self.beat_duration;
        if beat_progress < 0.05 {
            let attack = beat_progress / 0.05;
            attack * 0.5
        } else if beat_progress < 0.15 {
            let decay = (beat_progress - 0.05) / 0.10;
            0.5 * (1.0 - decay * decay)
        } else {
            0.0
        }
    }
}

impl Effect for Lightning {
    fn tick(&mut self, elapsed: f64, transport: &mut E131RawTransport, led_count: usize) {
        let pulse = self.calculate_pulse(elapsed);
        let pulse_color = [
            (self.color[0] as f64 * pulse) as u8,
            (self.color[1] as f64 * pulse) as u8,
            (self.color[2] as f64 * pulse) as u8,
        ];

        let universe = transport.universe();
        let state = self.states.entry(universe).or_default();

        let mut rng = rand::rng();

        if elapsed >= state.next_event_time {
            if state.flash_count == 0 {
                state.base_len = led_count / 10;
                state.max_len = led_count / 2;
                state.flash_start = rng.random_range(0..=(led_count - state.max_len));
                state.total_flashes = rng.random_range(3..6);
                state.flash_count = state.total_flashes;
                state.flash_on = true;
                state.after_leader = true;
                state.next_event_time = elapsed + 0.03;
            } else if state.flash_on {
                state.flash_on = false;
                state.flash_count -= 1;

                if state.after_leader {
                    state.next_event_time = elapsed + 0.15;
                    state.after_leader = false;
                } else if state.flash_count == 0 {
                    state.next_event_time = elapsed + rng.random_range(0.1..0.5);
                } else {
                    state.next_event_time = elapsed + rng.random_range(0.03..0.08);
                }
            } else {
                state.flash_on = true;
                state.flash_count -= 1;
                state.next_event_time = elapsed + rng.random_range(0.02..0.05);
            }
        }

        let mut led_buffer: Vec<[u8; 3]> = vec![pulse_color; led_count];

        if state.flash_on && state.total_flashes > 0 {
            let progress = 1.0 - (state.flash_count as f64 / state.total_flashes as f64);
            let flash_len = state.base_len + ((state.max_len - state.base_len) as f64 * progress) as usize;
            let end = (state.flash_start + flash_len).min(led_count);
            for i in state.flash_start..end {
                led_buffer[i] = self.color;
            }
        }

        let _ = transport.send_led_buffer(&led_buffer);
    }
}
