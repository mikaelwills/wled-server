use std::collections::HashMap;
use crate::transport::E131RawTransport;
use super::Effect;

pub struct Strobe {
    color: [u8; 3],
    beat_duration: f64,
    last_state_per_universe: HashMap<u16, bool>,
}

impl Strobe {
    pub fn new(color: [u8; 3], bpm: f64) -> Self {
        Self {
            color,
            beat_duration: 60.0 / bpm,
            last_state_per_universe: HashMap::new(),
        }
    }
}

impl Effect for Strobe {
    fn tick(&mut self, elapsed: f64, transport: &mut E131RawTransport, led_count: usize) {
        let beat_position = (elapsed % self.beat_duration) / self.beat_duration;

        let min_on_duration = 0.025;
        let on_threshold = (min_on_duration / self.beat_duration).max(0.05).min(0.15);
        let strobe_on = beat_position < on_threshold;

        let universe = transport.universe();
        let last_state = self.last_state_per_universe.get(&universe).copied().unwrap_or(false);

        if strobe_on == last_state {
            return;
        }
        self.last_state_per_universe.insert(universe, strobe_on);

        let (r, g, b) = if strobe_on {
            (self.color[0], self.color[1], self.color[2])
        } else {
            (0, 0, 0)
        };

        for _ in 0..3 {
            let _ = transport.send_raw_leds(led_count, r, g, b);
        }
    }
}
