use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub output_channels: u16,
    pub sample_rate: u32,
    pub is_default: bool,
}

#[cfg(target_os = "linux")]
fn build_alsa_friendly_names() -> HashMap<String, String> {
    let mut names = HashMap::new();
    let Ok(output) = std::process::Command::new("aplay").arg("-l").output() else {
        return names;
    };
    let Ok(text) = String::from_utf8(output.stdout) else {
        return names;
    };

    for line in text.lines() {
        if !line.starts_with("card ") {
            continue;
        }
        let Some(card_name) = line.split('[').nth(0)
            .and_then(|s| s.split(':').last())
            .map(|s| s.trim())
        else {
            continue;
        };

        let Some(device_part) = line.split(", device ").nth(1) else {
            continue;
        };
        let Some(dev_num) = device_part.split(':').next().map(|s| s.trim()) else {
            continue;
        };

        let card_friendly = line.split('[').nth(1)
            .and_then(|s| s.split(']').next())
            .unwrap_or(card_name)
            .to_string();

        let dev_name = line.rsplit('[').next()
            .and_then(|s| s.split(']').next())
            .unwrap_or(dev_num)
            .to_string();

        let display = if dev_name == card_friendly || dev_name.starts_with("USB ") {
            card_friendly.clone()
        } else {
            format!("{} - {}", card_friendly, dev_name)
        };

        names.insert(
            format!("hw:CARD={},DEV={}", card_name, dev_num),
            display.clone(),
        );
        names.insert(
            format!("plughw:CARD={},DEV={}", card_name, dev_num),
            format!("{} (plug)", display),
        );
        names.insert(
            format!("sysdefault:CARD={}", card_name),
            format!("{} (default)", display),
        );
        names.insert(
            format!("dmix:CARD={},DEV={}", card_name, dev_num),
            format!("{} (dmix)", display),
        );
    }
    names
}

#[cfg(not(target_os = "linux"))]
fn build_alsa_friendly_names() -> HashMap<String, String> {
    HashMap::new()
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
        let friendly_names = build_alsa_friendly_names();

        let mut devices = Vec::new();

        let Ok(output_devices) = host.output_devices() else {
            return devices;
        };

        for device in output_devices {
            let Ok(raw_name) = device.name() else { continue };
            let config = device.default_output_config().ok();
            let channels = config.as_ref().map(|c| c.channels()).unwrap_or(2);
            let sample_rate = config.as_ref().map(|c| c.sample_rate().0).unwrap_or(48000);
            let is_default = default_name.as_ref() == Some(&raw_name);

            let display_name = friendly_names.get(&raw_name)
                .cloned()
                .unwrap_or_else(|| raw_name.clone());

            devices.push(AudioDevice {
                id: raw_name,
                name: display_name,
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
