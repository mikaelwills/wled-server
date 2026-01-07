use std::collections::HashMap;
use crate::transport::E131RawTransport;
use super::Effect;

pub struct Pulse {
    color: [u8; 3],
    beat_duration: f64,
    last_brightness_per_universe: HashMap<u16, u8>,
}

impl Pulse {
    pub fn new(color: [u8; 3], bpm: f64) -> Self {
        Self {
            color,
            beat_duration: 60.0 / bpm,
            last_brightness_per_universe: HashMap::new(),
        }
    }
}

impl Effect for Pulse {
    fn tick(&mut self, elapsed: f64, transport: &mut E131RawTransport, led_count: usize) {
        let beat_position = (elapsed % self.beat_duration) / self.beat_duration;

        let decay_rate = 8.0;
        let brightness = ((-decay_rate * beat_position).exp() * 255.0) as u8;

        let universe = transport.universe();
        let last_brightness = self.last_brightness_per_universe.get(&universe).copied().unwrap_or(255);

        if brightness == last_brightness {
            return;
        }
        self.last_brightness_per_universe.insert(universe, brightness);

        let r = ((self.color[0] as u16 * brightness as u16) / 255) as u8;
        let g = ((self.color[1] as u16 * brightness as u16) / 255) as u8;
        let b = ((self.color[2] as u16 * brightness as u16) / 255) as u8;

        let _ = transport.send_raw_leds(led_count, r, g, b);
    }
}
