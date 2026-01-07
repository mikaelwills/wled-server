use std::collections::HashSet;
use crate::transport::E131RawTransport;
use super::Effect;

pub struct Solid {
    color: [u8; 3],
    sent_universes: HashSet<u16>,
}

impl Solid {
    pub fn new(color: [u8; 3]) -> Self {
        Self {
            color,
            sent_universes: HashSet::new(),
        }
    }
}

impl Effect for Solid {
    fn tick(&mut self, _elapsed: f64, transport: &mut E131RawTransport, led_count: usize) {
        let universe = transport.universe();
        if self.sent_universes.contains(&universe) {
            return;
        }
        self.sent_universes.insert(universe);

        let _ = transport.send_raw_leds(led_count, self.color[0], self.color[1], self.color[2]);
    }
}
