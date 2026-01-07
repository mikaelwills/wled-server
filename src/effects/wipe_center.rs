use crate::effects::Effect;
use crate::transport::E131RawTransport;

pub struct WipeCenter {
    color: [u8; 3],
    beat_duration: f64,
    trail_length: usize,
}

impl WipeCenter {
    pub fn new(color: [u8; 3], bpm: f64) -> Self {
        Self {
            color,
            beat_duration: 60.0 / bpm,
            trail_length: 20,
        }
    }
}

impl Effect for WipeCenter {
    fn tick(&mut self, elapsed: f64, transport: &mut E131RawTransport, led_count: usize) {
        let beat_position = (elapsed % self.beat_duration) / self.beat_duration;

        let eased = 1.0 - (1.0 - beat_position).powi(2);

        let at_peak = beat_position > 0.92;
        let peak_brightness = if at_peak { 1.5_f64.min(1.0 + (beat_position - 0.92) * 6.0) } else { 1.0 };

        let half = led_count / 2;
        let fill_distance = (eased * (half + self.trail_length) as f64) as usize;

        let mut led_buffer: Vec<[u8; 3]> = Vec::with_capacity(led_count);

        for i in 0..led_count {
            let dist_from_center = if i < half {
                half - i
            } else {
                i - half
            };

            let brightness = if dist_from_center < fill_distance {
                let distance_from_head = fill_distance - dist_from_center;
                if distance_from_head < self.trail_length {
                    let fade = 1.0 - (distance_from_head as f64 / self.trail_length as f64);
                    fade * fade * peak_brightness
                } else {
                    0.0
                }
            } else {
                0.0
            };

            let r = ((self.color[0] as f64 * brightness).min(255.0)) as u8;
            let g = ((self.color[1] as f64 * brightness).min(255.0)) as u8;
            let b = ((self.color[2] as f64 * brightness).min(255.0)) as u8;
            led_buffer.push([r, g, b]);
        }

        let _ = transport.send_led_buffer(&led_buffer);
    }
}
