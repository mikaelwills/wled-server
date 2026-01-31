use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub output_channels: u16,
    pub sample_rate: u32,
    pub is_default: bool,
}

pub struct DeviceManager {
    selected_device_id: RwLock<Option<String>>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            selected_device_id: RwLock::new(None),
        }
    }

    pub fn list_devices(&self) -> Vec<AudioDevice> {
        let host = cpal::default_host();
        let default_device = host.default_output_device();
        let default_name = default_device.as_ref().and_then(|d| d.name().ok());

        let mut devices = Vec::new();

        let Ok(output_devices) = host.output_devices() else {
            return devices;
        };

        for device in output_devices {
            let Ok(name) = device.name() else { continue };
            let config = device.default_output_config().ok();
            let channels = config.as_ref().map(|c| c.channels()).unwrap_or(2);
            let sample_rate = config.as_ref().map(|c| c.sample_rate().0).unwrap_or(48000);
            let is_default = default_name.as_ref() == Some(&name);

            devices.push(AudioDevice {
                id: name.clone(),
                name: name.clone(),
                output_channels: channels,
                sample_rate,
                is_default,
            });
        }

        devices
    }

    pub fn select_device(&self, device_id: Option<String>) {
        let mut selected = self.selected_device_id.write().unwrap();
        *selected = device_id;
    }

    pub fn get_selected_device(&self) -> Option<String> {
        self.selected_device_id.read().unwrap().clone()
    }

    pub fn get_output_device(&self) -> Option<cpal::Device> {
        let host = cpal::default_host();
        let selected = self.selected_device_id.read().unwrap();

        let Some(ref device_id) = *selected else {
            return host.default_output_device();
        };

        let Ok(devices) = host.output_devices() else {
            return host.default_output_device();
        };

        for device in devices {
            let Ok(name) = device.name() else { continue };
            if &name == device_id {
                return Some(device);
            }
        }

        host.default_output_device()
    }

    pub fn get_selected_sample_rate(&self) -> u32 {
        let Some(device) = self.get_output_device() else {
            return 48000;
        };
        let Ok(config) = device.default_output_config() else {
            return 48000;
        };
        config.sample_rate().0
    }

    pub fn get_device_sample_rate(&self, device_id: &str) -> Option<u32> {
        let host = cpal::default_host();

        let devices = host.output_devices().ok()?;

        for device in devices {
            let Some(name) = device.name().ok() else {
                continue;
            };
            if name != device_id {
                continue;
            }
            let config = device.default_output_config().ok()?;
            return Some(config.sample_rate().0);
        }

        None
    }

    pub fn get_device_output_channels(&self, device_id: &str) -> Option<u16> {
        let host = cpal::default_host();

        let devices = host.output_devices().ok()?;

        for device in devices {
            let Some(name) = device.name().ok() else {
                continue;
            };
            if name != device_id {
                continue;
            }
            let config = device.default_output_config().ok()?;
            return Some(config.channels());
        }
        None
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new()
    }
}
