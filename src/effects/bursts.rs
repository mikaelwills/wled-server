use std::collections::HashMap;
use super::Effect;
use crate::transport::E131RawTransport;
use rand::Rng;

struct BurstsState {
    leds: Vec<[u8; 3]>,
    last_beat: u64,
}

impl Default for BurstsState {
    fn default() -> Self {
        Self {
            leds: vec![[0, 0, 0]; 512],
            last_beat: u64::MAX,
        }
    }
}

pub struct Bursts {
    color: [u8; 3],
    beat_duration: f64,
    burst_size: usize,
    states: HashMap<u16, BurstsState>,
}

impl Bursts {
    pub fn new(color: [u8; 3], bpm: f64) -> Self {
        Self {
            color,
            beat_duration: 60.0 / bpm,
            burst_size: 8,
            states: HashMap::new(),
        }
    }
}

impl Effect for Bursts {
    fn tick(&mut self, elapsed: f64, transport: &mut E131RawTransport, led_count: usize) {
        let current_beat = (elapsed / self.beat_duration) as u64;
        let beat_position = (elapsed % self.beat_duration) / self.beat_duration;

        let universe = transport.universe();
        let state = self.states.entry(universe).or_default();

        for led in &mut state.leds {
            led[0] = (led[0] as u16 * 230 / 256) as u8;
            led[1] = (led[1] as u16 * 230 / 256) as u8;
            led[2] = (led[2] as u16 * 230 / 256) as u8;
        }

        if current_beat != state.last_beat {
            state.last_beat = current_beat;
            let mut rng = rand::rng();
            for _ in 0..3 {
                let pos = rng.random_range(0..led_count.saturating_sub(self.burst_size).max(1));
                for i in 0..self.burst_size {
                    if pos + i < led_count {
                        state.leds[pos + i] = self.color;
                    }
                }
            }
        }

        let pulse = 1.0 + 0.5 * (1.0 - beat_position).powi(2);

        let mut dmx_data = [0u8; 512];
        let count = led_count.min(128);
        for i in 0..count {
            let offset = i * 4;
            dmx_data[offset] = ((state.leds[i][0] as f64 * pulse).min(255.0)) as u8;
            dmx_data[offset + 1] = ((state.leds[i][1] as f64 * pulse).min(255.0)) as u8;
            dmx_data[offset + 2] = ((state.leds[i][2] as f64 * pulse).min(255.0)) as u8;
            dmx_data[offset + 3] = 0;
        }

        let _ = transport.send_dmx_packet(&dmx_data);
    }
}
