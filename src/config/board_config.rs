use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BoardConfig {
    pub id: String,
    pub ip: String,
    #[serde(default = "default_transition")]
    pub transition: Option<u8>,
    #[serde(default)]
    pub led_count: Option<u16>,
    #[serde(default)]
    pub universe: Option<u16>,
}

fn default_transition() -> Option<u8> {
    None
}

pub async fn configure_board_universe(
    board_ip: &str,
    universe: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let cfg_url = format!("http://{}/json/cfg", board_ip);
    let cfg_payload = serde_json::json!({
        "hw": {
            "led": {
                "fps": 60
            }
        },
        "if": {
            "live": {
                "en": true,
                "mc": false,
                "rlm": false,
                "dmx": {
                    "uni": universe,
                    "mode": 6,
                    "addr": 1
                },
                "timeout": 65535
            }
        }
    });

    let response = client
        .post(&cfg_url)
        .json(&cfg_payload)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("Config update failed: HTTP {}", response.status()).into());
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let reboot_url = format!("http://{}/json/state", board_ip);
    let reboot_payload = serde_json::json!({"rb": true});

    let reboot_response = client
        .post(&reboot_url)
        .json(&reboot_payload)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    if reboot_response.status().is_success() {
        Ok(())
    } else {
        Err(format!("Reboot failed: HTTP {}", reboot_response.status()).into())
    }
}
