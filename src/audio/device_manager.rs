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
    pub is_selected: bool,
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

#[cfg(target_os = "linux")]
struct ProcCard {
    name: String,
    friendly: String,
}

#[cfg(target_os = "linux")]
struct ProcPcm {
    card_num: u32,
    dev_num: u32,
    name: String,
    has_playback: bool,
}

#[cfg(target_os = "linux")]
fn discover_proc_asound_devices() -> Vec<(ProcCard, ProcPcm)> {
    let Ok(cards_text) = std::fs::read_to_string("/proc/asound/cards") else {
        return Vec::new();
    };
    let Ok(pcm_text) = std::fs::read_to_string("/proc/asound/pcm") else {
        return Vec::new();
    };

    let mut cards: HashMap<u32, ProcCard> = HashMap::new();
    for line in cards_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(first_char) = trimmed.chars().next() else { continue };
        if !first_char.is_ascii_digit() {
            continue;
        }
        let Some((num_str, rest)) = trimmed.split_once(' ') else { continue };
        let Ok(card_num) = num_str.trim().parse::<u32>() else { continue };
        let card_name = rest.split('[').nth(1)
            .and_then(|s| s.split(']').next())
            .unwrap_or("")
            .trim()
            .to_string();
        let friendly = rest.split(" - ").nth(1)
            .unwrap_or(&card_name)
            .trim()
            .to_string();
        if !card_name.is_empty() {
            cards.insert(card_num, ProcCard { name: card_name, friendly });
        }
    }

    let mut pcms: Vec<ProcPcm> = Vec::new();
    for line in pcm_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((id_part, rest)) = trimmed.split_once(':') else { continue };
        let Some((card_str, dev_str)) = id_part.split_once('-') else { continue };
        let Ok(card_num) = card_str.trim().parse::<u32>() else { continue };
        let Ok(dev_num) = dev_str.trim().parse::<u32>() else { continue };
        let parts: Vec<&str> = rest.splitn(3, ':').collect();
        let name = parts.first().unwrap_or(&"").trim().to_string();
        let has_playback = rest.contains("playback");
        pcms.push(ProcPcm { card_num, dev_num, name, has_playback });
    }

    let mut results = Vec::new();
    for pcm in pcms {
        if !pcm.has_playback {
            continue;
        }
        if let Some(card) = cards.get(&pcm.card_num) {
            results.push((card.clone(), pcm));
        }
    }
    results
}

#[cfg(target_os = "linux")]
impl Clone for ProcCard {
    fn clone(&self) -> Self {
        Self { name: self.name.clone(), friendly: self.friendly.clone() }
    }
}

#[cfg(not(target_os = "linux"))]
fn build_alsa_friendly_names() -> HashMap<String, String> {
    HashMap::new()
}

pub struct DeviceManager {
    selected_device_id: RwLock<Option<String>>,
    active_device_info: RwLock<Option<(u16, u32)>>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            selected_device_id: RwLock::new(None),
            active_device_info: RwLock::new(None),
        }
    }

    pub fn list_devices(&self) -> Vec<AudioDevice> {
        let host = cpal::default_host();
        let default_device = host.default_output_device();
        let default_name = default_device.as_ref().and_then(|d| d.name().ok());
        let friendly_names = build_alsa_friendly_names();
        let selected_id = self.selected_device_id.read().unwrap().clone();

        let mut devices = Vec::new();
        let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        let active_info = self.active_device_info.read().unwrap().clone();

        if let Ok(output_devices) = host.output_devices() {
            for device in output_devices {
                let Ok(raw_name) = device.name() else { continue };
                let is_default = default_name.as_ref() == Some(&raw_name);
                let is_selected = selected_id.as_deref() == Some(&raw_name);

                let (channels, sample_rate) = if is_selected {
                    if let Some((ch, sr)) = active_info {
                        (ch, sr)
                    } else {
                        let config = device.default_output_config().ok();
                        (
                            config.as_ref().map(|c| c.channels()).unwrap_or(2),
                            config.as_ref().map(|c| c.sample_rate().0).unwrap_or(48000),
                        )
                    }
                } else {
                    let config = device.default_output_config().ok();
                    (
                        config.as_ref().map(|c| c.channels()).unwrap_or(2),
                        config.as_ref().map(|c| c.sample_rate().0).unwrap_or(48000),
                    )
                };

                let display_name = friendly_names.get(&raw_name)
                    .cloned()
                    .unwrap_or_else(|| raw_name.clone());

                seen_ids.insert(raw_name.clone());
                devices.push(AudioDevice {
                    id: raw_name,
                    name: display_name,
                    output_channels: channels,
                    sample_rate,
                    is_default,
                    is_selected,
                });
            }
        }

        #[cfg(target_os = "linux")]
        {
            for (card, pcm) in discover_proc_asound_devices() {
                let hw_id = format!("hw:CARD={},DEV={}", card.name, pcm.dev_num);
                if seen_ids.contains(&hw_id) {
                    continue;
                }

                let display = if pcm.name.starts_with("USB ") || pcm.name == card.friendly {
                    card.friendly.clone()
                } else {
                    format!("{} - {}", card.friendly, pcm.name)
                };

                let is_selected = selected_id.as_deref() == Some(hw_id.as_str());
                let (channels, sample_rate) = if is_selected {
                    if let Some((ch, sr)) = active_info {
                        (ch, sr)
                    } else {
                        Self::probe_hw_device(&host, &hw_id)
                    }
                } else {
                    Self::probe_hw_device(&host, &hw_id)
                };

                seen_ids.insert(hw_id.clone());
                devices.push(AudioDevice {
                    id: hw_id,
                    name: display,
                    output_channels: channels,
                    sample_rate,
                    is_default: false,
                    is_selected,
                });
            }
        }

        devices.sort_by(|a, b| b.is_selected.cmp(&a.is_selected));
        devices
    }

    #[cfg(target_os = "linux")]
    fn probe_hw_device(host: &cpal::Host, device_id: &str) -> (u16, u32) {
        if let Some(device) = Self::find_device_by_name(host, device_id) {
            if let Ok(config) = device.default_output_config() {
                return (config.channels(), config.sample_rate().0);
            }
            if let Ok(mut configs) = device.supported_output_configs() {
                if let Some(range) = configs.next() {
                    return (range.channels(), range.max_sample_rate().0);
                }
            }
        }
        (2, 48000)
    }

    pub fn select_device(&self, device_id: Option<String>) {
        let mut selected = self.selected_device_id.write().unwrap();
        *self.active_device_info.write().unwrap() = None;
        *selected = device_id;
    }

    pub fn set_active_device_info(&self, channels: u16, sample_rate: u32) {
        *self.active_device_info.write().unwrap() = Some((channels, sample_rate));
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

        Self::find_device_by_name(&host, device_id)
            .or_else(|| host.default_output_device())
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

    fn find_device_by_name(host: &cpal::Host, device_id: &str) -> Option<cpal::Device> {
        if let Ok(devices) = host.output_devices() {
            for d in devices {
                if let Ok(name) = d.name() {
                    if name == device_id { return Some(d); }
                }
            }
        }
        if let Ok(devices) = host.devices() {
            for d in devices {
                if let Ok(name) = d.name() {
                    if name == device_id { return Some(d); }
                }
            }
        }
        None
    }

    pub fn get_device_sample_rate(&self, device_id: &str) -> Option<u32> {
        let host = cpal::default_host();
        let device = Self::find_device_by_name(&host, device_id)?;
        if let Ok(config) = device.default_output_config() {
            return Some(config.sample_rate().0);
        }
        if let Ok(mut configs) = device.supported_output_configs() {
            if let Some(range) = configs.next() {
                return Some(range.max_sample_rate().0);
            }
        }
        None
    }

    pub fn get_device_output_channels(&self, device_id: &str) -> Option<u16> {
        let host = cpal::default_host();
        let device = Self::find_device_by_name(&host, device_id)?;
        if let Ok(config) = device.default_output_config() {
            return Some(config.channels());
        }
        if let Ok(mut configs) = device.supported_output_configs() {
            if let Some(range) = configs.next() {
                return Some(range.channels());
            }
        }
        None
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new()
    }
}
