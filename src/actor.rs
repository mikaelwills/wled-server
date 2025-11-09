use crate::board::{BoardCommand, BoardState};
use crate::config::E131Config;
use crate::sse::SseEvent;
use crate::transport::E131Transport;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

pub struct BoardActor {
    pub id: String,
    pub ip: String,
    pub state: BoardState,
    broadcast_tx: Arc<broadcast::Sender<SseEvent>>,
    e131_transport: Option<E131Transport>,
}

impl BoardActor {
    pub fn new(
        id: String,
        ip: String,
        broadcast_tx: Arc<broadcast::Sender<SseEvent>>,
        e131_config: Option<E131Config>,
    ) -> Self {
        // Initialize E1.31 transport if configured
        let e131_transport = e131_config.and_then(|config| {
            match E131Transport::new(&ip, config.universe) {
                Ok(transport) => {
                    info!(board_id = %id, universe = config.universe, "E1.31 transport enabled");
                    Some(transport)
                }
                Err(e) => {
                    error!(board_id = %id, error = %e, "Failed to initialize E1.31 transport, falling back to WebSocket-only");
                    None
                }
            }
        });

        Self {
            id: id.clone(),
            ip: ip.clone(),
            state: BoardState::new(id, ip),
            broadcast_tx,
            e131_transport,
        }
    }

    fn broadcast_state(&self) {
        let event = SseEvent::StateUpdate {
            board_id: self.id.clone(),
            state: self.state.clone(),
        };
        let _ = self.broadcast_tx.send(event);
    }

    fn broadcast_connection_status(&self) {
        let event = SseEvent::ConnectionStatus {
            board_id: self.id.clone(),
            connected: self.state.connected,
        };
        let _ = self.broadcast_tx.send(event);
    }

    pub async fn run(
        mut self,
        mut cmd_rx: mpsc::Receiver<BoardCommand>,
    ) -> Result<(), Box<dyn Error>> {
        loop {
            let url = format!("ws://{}/ws", self.ip);

            // Try to connect, but don't crash if it fails
            let ws_stream = match connect_async(url).await {
                Ok((stream, _)) => stream,
                Err(_) => {
                    // Temporarily commented out to reduce noise in logs
                    // warn!(board_id = %self.id, "Failed to connect: {}, retrying in 5 seconds...", e);
                    self.state.connected = false;
                    self.broadcast_connection_status();

                    // Handle commands while disconnected
                    tokio::select! {
                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(3)) => {},
                        cmd = cmd_rx.recv() => {
                            match cmd {
                                Some(BoardCommand::GetState(response_tx)) => {
                                    let _ = response_tx.send(self.state.clone());
                                }
                                Some(BoardCommand::SetPower(target_state, _transition)) => {
                                    // Cache the state change, will be sent when reconnected
                                    self.state.on = target_state;
                                    self.broadcast_state();
                                }
                                Some(BoardCommand::SetBrightness(bri, _transition)) => {
                                    // Cache the state change
                                    self.state.brightness = bri;
                                    self.broadcast_state();
                                }
                                Some(BoardCommand::SetColor { r, g, b, transition: _ }) => {
                                    // Cache the state change
                                    self.state.color = [r, g, b];
                                    self.broadcast_state();
                                }
                                Some(BoardCommand::SetEffect(effect, _transition)) => {
                                    // Cache the state change
                                    self.state.effect = effect;
                                    self.broadcast_state();
                                }
                                Some(BoardCommand::SetSpeed(speed, _transition)) => {
                                    // Cache the state change
                                    self.state.speed = speed;
                                    self.broadcast_state();
                                }
                                Some(BoardCommand::SetIntensity(intensity, _transition)) => {
                                    // Cache the state change
                                    self.state.intensity = intensity;
                                    self.broadcast_state();
                                }
                                Some(BoardCommand::SetLedCount(led_count)) => {
                                    // Cache the state change
                                    self.state.led_count = Some(led_count);
                                    self.broadcast_state();
                                }
                                Some(BoardCommand::Shutdown) => {
                                    info!(board_id = %self.id, "Shutting down actor");
                                    return Ok(());
                                }
                                None => return Ok(()),
                                _ => {} // Ignore preset and segment reset commands while disconnected
                            }
                        }
                    }
                    continue;
                }
            };

            info!(board_id = %self.id, "Connected to WLED WebSocket");

            let (mut write, mut read) = ws_stream.split();

            // First, read the board's current state to sync with reality
            // Wait for first message from WLED (it sends state on connection)
            match timeout(tokio::time::Duration::from_millis(500), read.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(json) = serde_json::from_str::<Value>(&text) {
                        self.update_state_from_json(&json);
                        info!(board_id = %self.id, "Initial state synced from board");
                    }
                }
                Ok(Some(Ok(_))) => {
                    // Got some other message type, ignore it
                }
                Ok(Some(Err(e))) => {
                    warn!(board_id = %self.id, "Error reading initial state: {}", e);
                }
                Ok(None) | Err(_) => {
                    warn!(board_id = %self.id, "Timeout waiting for initial state");
                }
            }

            // NOW mark as connected and broadcast the real state
            self.state.connected = true;
            self.broadcast_connection_status();
            self.broadcast_state();

            // Create ping interval for keepalive (5 seconds)
            let mut ping_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

            loop {
                tokio::select! {
                  _ = ping_interval.tick() => {
                      write.send(Message::Ping(vec![])).await?;
                  }
                  msg = timeout(tokio::time::Duration::from_secs(5), read.next()) => {
                      match msg {
                          Err(_) => {
                              // Timeout - no message received in 12 seconds
                              warn!(board_id = %self.id, "Read timeout, connection dead");
                              self.state.connected = false;
                              self.broadcast_connection_status();
                              break;
                          }
                          Ok(Some(Ok(Message::Text(text)))) => {
                              if let Ok(json) = serde_json::from_str::<Value>(&text) {
                                  self.update_state_from_json(&json);
                                  self.broadcast_state();
                              }
                          }
                          Ok(Some(Ok(Message::Pong(_)))) => {
                              // Connection alive - keep going
                          }
                          Ok(Some(Ok(Message::Close(_)))) => {
                              info!(board_id = %self.id, "Connection closed by remote");
                              self.state.connected = false;
                              self.broadcast_connection_status();
                              break;
                          }
                          Ok(Some(Err(e))) => {
                              error!(board_id = %self.id, "Connection lost: {:?}", e);
                              self.state.connected = false;
                              self.broadcast_connection_status();
                              break;
                          }
                          Ok(None) => {
                              info!(board_id = %self.id, "Connection closed");
                              self.state.connected = false;
                              self.broadcast_connection_status();
                              break;
                          }
                          _ => {
                              // Other message types (Binary, Ping, etc.) - ignore
                          }
                      }
                  }
                  cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(BoardCommand::SetPower(target_state, _transition)) => {
                                self.state.on = target_state;

                                // Route via E1.31 if available (faster, no buffer overflow)
                                if let Some(ref mut e131) = self.e131_transport {
                                    if let Err(e) = e131.send_power(target_state, self.state.brightness) {
                                        warn!(board_id = %self.id, error = %e, "E1.31 send failed, falling back to WebSocket");
                                    } else {
                                        self.broadcast_state();
                                        continue; // Skip WebSocket send
                                    }
                                }

                                // WebSocket fallback
                                let msg = Message::Text(format!(r#"{{"on":{},"tt":0}}"#, target_state));
                                timeout(tokio::time::Duration::from_secs(2), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetBrightness(bri, _transition)) => {
                                self.state.brightness = bri;

                                // Route via E1.31 if available (faster, no buffer overflow)
                                if let Some(ref mut e131) = self.e131_transport {
                                    let current_preset = self.state.preset.unwrap_or(0);
                                    if let Err(e) = e131.send_brightness(bri, current_preset) {
                                        warn!(board_id = %self.id, error = %e, "E1.31 send failed, falling back to WebSocket");
                                    } else {
                                        self.broadcast_state();
                                        continue; // Skip WebSocket send
                                    }
                                }

                                // WebSocket fallback
                                let msg = Message::Text(format!(r#"{{"bri":{},"tt":0}}"#, bri));
                                timeout(tokio::time::Duration::from_secs(2), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetColor { r, g, b, transition: _ }) => {
                                self.state.color = [r, g, b];
                                let msg = Message::Text(format!(
                                    r#"{{"seg":[{{"col":[[{},{},{}]]}}],"tt":0}}"#,
                                    r, g, b
                                ));
                                timeout(tokio::time::Duration::from_secs(2), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetEffect(effect, _transition)) => {
                                self.state.effect = effect;
                                let msg = Message::Text(format!(r#"{{"seg":[{{"fx":{}}}],"tt":0}}"#, effect));
                                timeout(tokio::time::Duration::from_secs(2), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetSpeed(speed, _transition)) => {
                                self.state.speed = speed;
                                let msg = Message::Text(format!(r#"{{"seg":[{{"sx":{}}}],"tt":0}}"#, speed));
                                timeout(tokio::time::Duration::from_secs(2), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetIntensity(intensity, _transition)) => {
                                self.state.intensity = intensity;
                                let msg = Message::Text(format!(r#"{{"seg":[{{"ix":{}}}],"tt":0}}"#, intensity));
                                timeout(tokio::time::Duration::from_secs(2), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetPreset(preset, _transition)) => {
                                self.state.preset = Some(preset);

                                // Route via E1.31 if available (faster, no buffer overflow)
                                if let Some(ref mut e131) = self.e131_transport {
                                    let before_e131_send = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_millis();
                                    println!("📤 [{}ms] Sending via E1.31: board='{}' preset={}", before_e131_send, self.id, preset);

                                    if let Err(e) = e131.send_preset(preset, self.state.brightness) {
                                        warn!(board_id = %self.id, error = %e, "E1.31 send failed, falling back to WebSocket");
                                    } else {
                                        let after_e131_send = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_millis();
                                        println!("✅ [{}ms] Sent via E1.31: board='{}' preset={}", after_e131_send, self.id, preset);
                                        self.broadcast_state();
                                        continue; // Skip WebSocket send
                                    }
                                }

                                // WebSocket fallback
                                let before_ws_send = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis();
                                println!("📤 [{}ms] Sending via WS: board='{}' preset={}", before_ws_send, self.id, preset);

                                let msg = Message::Text(format!(r#"{{"ps":{},"tt":0}}"#, preset));
                                timeout(tokio::time::Duration::from_secs(2), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;

                                let after_ws_send = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis();
                                println!("✅ [{}ms] Sent via WS: board='{}' preset={}", after_ws_send, self.id, preset);

                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetLedCount(led_count)) => {
                                let msg = Message::Text(format!(r#"{{"seg":[{{"len":{}}}]}}"#, led_count));
                                timeout(tokio::time::Duration::from_secs(2), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.state.led_count = Some(led_count);
                                self.broadcast_state();
                            }
                            Some(BoardCommand::ResetSegment) => {
                                // Reset segment to defaults: grp=1, spc=0, of=0
                                let msg = Message::Text(r#"{"seg":[{"id":0,"grp":1,"spc":0,"of":0}]}"#.to_string());
                                timeout(tokio::time::Duration::from_secs(2), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::GetState(response_tx)) => {
                                let _ = response_tx.send(self.state.clone());
                            }
                            Some(BoardCommand::Shutdown) => {
                                info!(board_id = %self.id, "Shutting down actor");
                                return Ok(());
                            }
                            None => return Ok(()),
                        }
                    }
                }
            }

            info!(board_id = %self.id, "WebSocket closed, reconnecting in 3 seconds...");
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        }
    }

    fn update_state_from_json(&mut self, json: &Value) {
        if let Some(on) = json["state"]["on"].as_bool() {
            self.state.on = on;
        }
        if let Some(bri) = json["state"]["bri"].as_u64() {
            self.state.brightness = bri as u8;
        }

        // Parse color and effect from segment data
        if let Some(seg) = json["state"]["seg"].as_array() {
            if let Some(first_seg) = seg.first() {
                // Parse color: state.seg[0].col[0] is the RGB array
                if let Some(col) = first_seg["col"].as_array() {
                    if let Some(color_array) = col.first().and_then(|c| c.as_array()) {
                        if color_array.len() >= 3 {
                            self.state.color = [
                                color_array[0].as_u64().unwrap_or(255) as u8,
                                color_array[1].as_u64().unwrap_or(255) as u8,
                                color_array[2].as_u64().unwrap_or(255) as u8,
                            ];
                        }
                    }
                }

                // Parse effect: state.seg[0].fx
                if let Some(fx) = first_seg["fx"].as_u64() {
                    self.state.effect = fx as u8;
                }

                // Parse speed: state.seg[0].sx
                if let Some(sx) = first_seg["sx"].as_u64() {
                    self.state.speed = sx as u8;
                }

                // Parse intensity: state.seg[0].ix
                if let Some(ix) = first_seg["ix"].as_u64() {
                    self.state.intensity = ix as u8;
                }

                // Parse LED count: state.seg[0].stop
                if let Some(stop) = first_seg["stop"].as_u64() {
                    self.state.led_count = Some(stop as u16);
                }
            }
        }

        // Parse max LEDs from info
        if let Some(leds) = json["info"]["leds"]["count"].as_u64() {
            self.state.max_leds = Some(leds as u16);
        }
    }
}
