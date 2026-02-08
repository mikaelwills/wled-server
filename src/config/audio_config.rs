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

    /// Resolve the preferred audio device and routing on startup.
    /// Falls back to the default device if the preferred one isn't available.
    pub fn resolve_startup_routing(
        &self,
        device_manager: &crate::audio::DeviceManager,
    ) -> Option<crate::audio::RoutingConfig> {
        let preferred_device = self.preferred_device_id.as_ref()?;
        let available_devices = device_manager.list_devices();
        let device_exists = available_devices.iter().any(|d| &d.id == preferred_device);

        if device_exists {
            device_manager.select_device(Some(preferred_device.clone()));
            tracing::info!("Restored preferred audio device: {}", preferred_device);

            let output_channels = device_manager
                .get_device_output_channels(preferred_device)
                .unwrap_or(2) as usize;
            let routing_config = self
                .get_routing_for_device(preferred_device)
                .cloned()
                .unwrap_or_else(|| DeviceRouting::new_default(preferred_device.clone()));
            let routing = crate::audio::RoutingConfig::from_device_routing(&routing_config, output_channels);
            tracing::info!("Restored audio routing for {}: backing={}/{}, guide={}, click={}",
                preferred_device,
                routing_config.backing_left, routing_config.backing_right,
                routing_config.guide, routing_config.click);
            Some(routing)
        } else {
            let fallback = available_devices.iter().find(|d| d.is_default).or(available_devices.first());
            if let Some(dev) = fallback {
                tracing::warn!("Preferred audio device '{}' not found, falling back to '{}'", preferred_device, dev.name);
                device_manager.select_device(Some(dev.id.clone()));
                let output_channels = dev.output_channels as usize;
                let routing_config = self
                    .get_routing_for_device(&dev.id)
                    .cloned()
                    .unwrap_or_else(|| DeviceRouting::new_default(dev.id.clone()));
                let routing = crate::audio::RoutingConfig::from_device_routing(&routing_config, output_channels);
                Some(routing)
            } else {
                tracing::warn!("Preferred audio device '{}' not found, no audio devices available", preferred_device);
                None
            }
        }
    }
}
