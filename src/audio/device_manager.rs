use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub output_channels: u16,
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
        let default_name = default_device
            .as_ref()
            .and_then(|d| d.name().ok());

        let mut devices = Vec::new();

        if let Ok(output_devices) = host.output_devices() {
            for device in output_devices {
                if let Ok(name) = device.name() {
                    let config = device.default_output_config().ok();
                    let channels = config.map(|c| c.channels()).unwrap_or(2);
                    let is_default = default_name.as_ref() == Some(&name);

                    devices.push(AudioDevice {
                        id: name.clone(),
                        name: name.clone(),
                        output_channels: channels,
                        is_default,
                    });
                }
            }
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

        if let Some(ref device_id) = *selected {
            if let Ok(devices) = host.output_devices() {
                for device in devices {
                    if let Ok(name) = device.name() {
                        if &name == device_id {
                            return Some(device);
                        }
                    }
                }
            }
        }

        host.default_output_device()
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new()
    }
}
