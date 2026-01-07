mod bursts;
mod flash;
mod lightning;
mod puddles;
mod pulse;
mod solid;
mod sparkle;
mod strobe;
mod wipe_center;
mod wipe_up;

pub use bursts::Bursts;
pub use flash::Flash;
pub use lightning::Lightning;
pub use puddles::Puddles;
pub use pulse::Pulse;
pub use solid::Solid;
pub use sparkle::Sparkle;
pub use strobe::Strobe;
pub use wipe_center::WipeCenter;
pub use wipe_up::WipeUp;

use std::str::FromStr;

use crate::transport::E131RawTransport;

pub trait Effect: Send {
    fn tick(&mut self, elapsed: f64, transport: &mut E131RawTransport, led_count: usize);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectType {
    Strobe,
    Solid,
    Pulse,
    Bursts,
    Flash,
    WipeUp,
    WipeCenter,
    Lightning,
    Puddles,
    Sparkle,
}

impl EffectType {
    pub fn create(&self, color: [u8; 3], bpm: f64) -> Box<dyn Effect> {
        match self {
            EffectType::Strobe => Box::new(Strobe::new(color, bpm)),
            EffectType::Solid => Box::new(Solid::new(color)),
            EffectType::Pulse => Box::new(Pulse::new(color, bpm)),
            EffectType::Bursts => Box::new(Bursts::new(color, bpm)),
            EffectType::Flash => Box::new(Flash::new(color)),
            EffectType::WipeUp => Box::new(WipeUp::new(color, bpm)),
            EffectType::WipeCenter => Box::new(WipeCenter::new(color, bpm)),
            EffectType::Lightning => Box::new(Lightning::new(color, bpm)),
            EffectType::Puddles => Box::new(Puddles::new(color, bpm)),
            EffectType::Sparkle => Box::new(Sparkle::new(color, bpm)),
        }
    }
}

impl FromStr for EffectType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "strobe" => Ok(EffectType::Strobe),
            "solid" => Ok(EffectType::Solid),
            "pulse" => Ok(EffectType::Pulse),
            "bursts" => Ok(EffectType::Bursts),
            "flash" => Ok(EffectType::Flash),
            "wipe_up" => Ok(EffectType::WipeUp),
            "wipe_center" => Ok(EffectType::WipeCenter),
            "lightning" => Ok(EffectType::Lightning),
            "puddles" => Ok(EffectType::Puddles),
            "sparkle" => Ok(EffectType::Sparkle),
            _ => Err(format!("Unknown effect type: {}", s)),
        }
    }
}
