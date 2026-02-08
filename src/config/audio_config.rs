use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeviceRouting {
    pub device_id: String,
    #[serde(default = "default_channel_1")]
    pub backing_left: u16,
    #[serde(default = "default_channel_2")]
    pub backing_right: u16,
    #[serde(default = "default_channel_3")]
    pub guide: u16,
    #[serde(default = "default_channel_4")]
    pub click: u16,
}

fn default_channel_1() -> u16 { 1 }
fn default_channel_2() -> u16 { 2 }
fn default_channel_3() -> u16 { 3 }
fn default_channel_4() -> u16 { 4 }

impl DeviceRouting {
    pub fn new_default(device_id: String) -> Self {
        Self {
            device_id,
            backing_left: 1,
            backing_right: 2,
            guide: 3,
            click: 4,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResamplingQuality {
    Fast,
    #[default]
    Balanced,
    High,
}

impl ResamplingQuality {
    pub fn sinc_len(&self) -> usize {
        match self {
            ResamplingQuality::Fast => 16,
            ResamplingQuality::Balanced => 128,
            ResamplingQuality::High => 256,
        }
    }

    pub fn oversampling_factor(&self) -> usize {
        match self {
            ResamplingQuality::Fast => 16,
            ResamplingQuality::Balanced => 128,
            ResamplingQuality::High => 256,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AudioConfig {
    #[serde(default)]
    pub preferred_device_id: Option<String>,
    #[serde(default)]
    pub device_routings: Vec<DeviceRouting>,
    #[serde(default)]
    pub resampling_quality: ResamplingQuality,
}

impl AudioConfig {
    pub fn get_routing_for_device(&self, device_id: &str) -> Option<&DeviceRouting> {
        self.device_routings.iter().find(|r| r.device_id == device_id)
    }

    pub fn set_routing_for_device(&mut self, routing: DeviceRouting) {
        if let Some(existing) = self.device_routings.iter_mut().find(|r| r.device_id == routing.device_id) {
            *existing = routing;
        } else {
            self.device_routings.push(routing);
        }
    }
}
