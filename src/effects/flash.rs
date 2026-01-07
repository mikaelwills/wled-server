use std::collections::HashSet;
use crate::effects::Effect;
use crate::transport::E131RawTransport;

pub struct Flash {
    color: [u8; 3],
    flash_duration: f64,
    fade_duration: f64,
    done_universes: HashSet<u16>,
}

impl Flash {
    pub fn new(color: [u8; 3]) -> Self {
        Self {
            color,
            flash_duration: 0.030,
            fade_duration: 0.200,
            done_universes: HashSet::new(),
        }
    }
}

impl Effect for Flash {
    fn tick(&mut self, elapsed: f64, transport: &mut E131RawTransport, led_count: usize) {
        let universe = transport.universe();
        if self.done_universes.contains(&universe) {
            return;
        }

        if elapsed < self.flash_duration {
            let _ = transport.send_raw_leds(led_count, self.color[0], self.color[1], self.color[2]);
        } else if elapsed < self.flash_duration + self.fade_duration {
            let fade_progress = (elapsed - self.flash_duration) / self.fade_duration;
            let brightness = (1.0 - fade_progress).powi(2);
            let r = (self.color[0] as f64 * brightness) as u8;
            let g = (self.color[1] as f64 * brightness) as u8;
            let b = (self.color[2] as f64 * brightness) as u8;
            let _ = transport.send_raw_leds(led_count, r, g, b);
        } else {
            let _ = transport.send_raw_leds(led_count, 0, 0, 0);
            self.done_universes.insert(universe);
        }
    }
}
