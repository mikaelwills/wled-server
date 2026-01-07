use std::collections::HashMap;
use crate::effects::Effect;
use crate::transport::E131RawTransport;
use rand::Rng;

struct Spark {
    position: usize,
    brightness: f64,
}

#[derive(Default)]
struct SparkleState {
    sparks: Vec<Spark>,
    last_spawn_beat: i32,
}

pub struct Sparkle {
    color: [u8; 3],
    beat_duration: f64,
    states: HashMap<u16, SparkleState>,
}

impl Sparkle {
    pub fn new(color: [u8; 3], bpm: f64) -> Self {
        Self {
            color,
            beat_duration: 60.0 / bpm,
            states: HashMap::new(),
        }
    }
}

impl Effect for Sparkle {
    fn tick(&mut self, elapsed: f64, transport: &mut E131RawTransport, led_count: usize) {
        let universe = transport.universe();
        let state = self.states.entry(universe).or_default();

        let mut rng = rand::rng();

        let subdivisions = 4.0;
        let sub_beat_duration = self.beat_duration / subdivisions;
        let current_sub_beat = (elapsed / sub_beat_duration) as i32;

        if current_sub_beat != state.last_spawn_beat {
            state.last_spawn_beat = current_sub_beat;

            let spawn_count = rng.random_range(1..=3);
            for _ in 0..spawn_count {
                let pos = rng.random_range(0..led_count);
                state.sparks.push(Spark {
                    position: pos,
                    brightness: 1.0,
                });
            }
        }

        let decay_rate = 0.15;
        for spark in &mut state.sparks {
            spark.brightness -= decay_rate;
        }
        state.sparks.retain(|s| s.brightness > 0.05);

        let mut led_buffer: Vec<[u8; 3]> = vec![[0, 0, 0]; led_count];

        for spark in &state.sparks {
            if spark.position >= led_count {
                continue;
            }
            let bright = spark.brightness.powi(2);
            let r = (self.color[0] as f64 * bright) as u8;
            let g = (self.color[1] as f64 * bright) as u8;
            let b = (self.color[2] as f64 * bright) as u8;

            led_buffer[spark.position] = [r, g, b];

            let glow_bright = bright * 0.3;
            let gr = (self.color[0] as f64 * glow_bright) as u8;
            let gg = (self.color[1] as f64 * glow_bright) as u8;
            let gb = (self.color[2] as f64 * glow_bright) as u8;

            if spark.position > 0 {
                let idx = spark.position - 1;
                led_buffer[idx][0] = led_buffer[idx][0].saturating_add(gr);
                led_buffer[idx][1] = led_buffer[idx][1].saturating_add(gg);
                led_buffer[idx][2] = led_buffer[idx][2].saturating_add(gb);
            }
            if spark.position < led_count - 1 {
                let idx = spark.position + 1;
                led_buffer[idx][0] = led_buffer[idx][0].saturating_add(gr);
                led_buffer[idx][1] = led_buffer[idx][1].saturating_add(gg);
                led_buffer[idx][2] = led_buffer[idx][2].saturating_add(gb);
            }
        }

        let _ = transport.send_led_buffer(&led_buffer);
    }
}
