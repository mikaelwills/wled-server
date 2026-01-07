use std::collections::HashMap;
use crate::effects::Effect;
use crate::transport::E131RawTransport;
use rand::Rng;

struct ActivePuddle {
    position: usize,
    size: usize,
    age: f64,
}

struct PuddlesState {
    leds: Vec<[u8; 3]>,
    next_puddle_time: f64,
    active_puddles: Vec<ActivePuddle>,
}

impl Default for PuddlesState {
    fn default() -> Self {
        Self {
            leds: vec![[0, 0, 0]; 512],
            next_puddle_time: 0.0,
            active_puddles: Vec::new(),
        }
    }
}

pub struct Puddles {
    color: [u8; 3],
    fade_rate: u8,
    puddle_size: usize,
    fade_in_duration: f64,
    states: HashMap<u16, PuddlesState>,
}

impl Puddles {
    pub fn new(color: [u8; 3], _bpm: f64) -> Self {
        Self {
            color,
            fade_rate: 240,
            puddle_size: 8,
            fade_in_duration: 0.15,
            states: HashMap::new(),
        }
    }
}

impl Effect for Puddles {
    fn tick(&mut self, elapsed: f64, transport: &mut E131RawTransport, led_count: usize) {
        let universe = transport.universe();
        let state = self.states.entry(universe).or_default();

        let fade = self.fade_rate as u16;
        for led in &mut state.leds {
            led[0] = (led[0] as u16 * fade / 256) as u8;
            led[1] = (led[1] as u16 * fade / 256) as u8;
            led[2] = (led[2] as u16 * fade / 256) as u8;
        }

        if elapsed >= state.next_puddle_time {
            let mut rng = rand::rng();
            let pos = rng.random_range(0..led_count);
            let size = rng.random_range(1..self.puddle_size + 1);
            state.active_puddles.push(ActivePuddle {
                position: pos,
                size,
                age: 0.0,
            });
            state.next_puddle_time = elapsed + rng.random_range(0.03..0.12);
        }

        let dt = 0.025;
        for puddle in &mut state.active_puddles {
            puddle.age += dt;

            let brightness = if puddle.age < self.fade_in_duration {
                (puddle.age / self.fade_in_duration).powi(2)
            } else {
                1.0
            };

            for i in 0..puddle.size {
                let idx = puddle.position + i;
                if idx < led_count {
                    let r = (self.color[0] as f64 * brightness) as u8;
                    let g = (self.color[1] as f64 * brightness) as u8;
                    let b = (self.color[2] as f64 * brightness) as u8;
                    state.leds[idx][0] = state.leds[idx][0].max(r);
                    state.leds[idx][1] = state.leds[idx][1].max(g);
                    state.leds[idx][2] = state.leds[idx][2].max(b);
                }
            }
        }
        state.active_puddles.retain(|p| p.age < self.fade_in_duration + 0.05);

        let mut dmx_data = [0u8; 512];
        let count = led_count.min(128);
        for i in 0..count {
            let offset = i * 4;
            dmx_data[offset] = state.leds[i][0];
            dmx_data[offset + 1] = state.leds[i][1];
            dmx_data[offset + 2] = state.leds[i][2];
            dmx_data[offset + 3] = 0;
        }

        let _ = transport.send_dmx_packet(&dmx_data);
    }
}
