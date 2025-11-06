use crate::board::{BoardCommand, BoardState};
use crate::sse::SseEvent;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

pub struct BoardActor {
    pub id: String,
    pub ip: String,
    pub state: BoardState,
    broadcast_tx: Arc<broadcast::Sender<SseEvent>>,
}

impl BoardActor {
    pub fn new(id: String, ip: String, broadcast_tx: Arc<broadcast::Sender<SseEvent>>) -> Self {
        Self {
            id: id.clone(),
            ip: ip.clone(),
            state: BoardState::new(id, ip),
            broadcast_tx,
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
                Err(e) => {
                    warn!(board_id = %self.id, "Failed to connect: {}, retrying in 5 seconds...", e);
                    self.state.connected = false;
                    self.broadcast_connection_status();

                    // Handle commands while disconnected
                    tokio::select! {
                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {},
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

            info!(board_id = %self.id, "Connected to WLED");

            // Mark as connected and broadcast status
            self.state.connected = true;
            self.broadcast_connection_status();

            let (mut write, mut read) = ws_stream.split();

            // Sync cached state to the physical board after reconnection
            // Build a composite state update message with all cached properties
            let sync_msg = format!(
                r#"{{"on":{},"bri":{},"seg":[{{"col":[[{},{},{}]],"fx":{}}}]}}"#,
                self.state.on,
                self.state.brightness,
                self.state.color[0],
                self.state.color[1],
                self.state.color[2],
                self.state.effect
            );

            if let Err(e) = timeout(
                tokio::time::Duration::from_secs(5),
                write.send(Message::Text(sync_msg))
            ).await {
                error!(board_id = %self.id, "Failed to sync state after reconnection: {:?}", e);
            } else {
                info!(board_id = %self.id, "State synced to board after reconnection");
            }

            // Create ping interval for keepalive (5 seconds)
            let mut ping_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
            let mut last_message_time = Instant::now();

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
                              last_message_time = Instant::now();
                          }
                          Ok(Some(Ok(Message::Pong(_)))) => {
                              // Connection alive - reset timer
                              last_message_time = Instant::now();
                          }
                          Ok(Some(Ok(Message::Close(_)))) => {
                              info!(board_id = %self.id, "Connection closed by remote");
                              self.state.connected = false;
                              self.broadcast_connection_status();
                              break;
                          }
                          Ok(Some(Err(e))) => {
                              let elapsed = last_message_time.elapsed().as_secs();
                              error!(board_id = %self.id, elapsed = %elapsed, "Connection lost: {:?}", e);
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
                              // Other message types (Binary, Ping, etc.) - ignore but reset timer
                              last_message_time = Instant::now();
                          }
                      }
                  }
                  cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(BoardCommand::SetPower(target_state, transition)) => {
                                self.state.on = target_state;
                                let msg = Message::Text(format!(r#"{{"on":{},"tt":{}}}"#, target_state, transition));
                                timeout(tokio::time::Duration::from_secs(5), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetBrightness(bri, transition)) => {
                                self.state.brightness = bri;
                                let msg = Message::Text(format!(r#"{{"bri":{},"tt":{}}}"#, bri, transition));
                                timeout(tokio::time::Duration::from_secs(5), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetColor { r, g, b, transition }) => {
                                self.state.color = [r, g, b];
                                let msg = Message::Text(format!(
                                    r#"{{"seg":[{{"col":[[{},{},{}]]}}],"tt":{}}}"#,
                                    r, g, b, transition
                                ));
                                timeout(tokio::time::Duration::from_secs(5), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetEffect(effect, transition)) => {
                                self.state.effect = effect;
                                let msg = Message::Text(format!(r#"{{"seg":[{{"fx":{}}}],"tt":{}}}"#, effect, transition));
                                timeout(tokio::time::Duration::from_secs(5), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetPreset(preset, transition)) => {
                                let msg = Message::Text(format!(r#"{{"ps":{},"tt":{}}}"#, preset, transition));
                                timeout(tokio::time::Duration::from_secs(5), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetLedCount(led_count)) => {
                                let msg = Message::Text(format!(r#"{{"seg":[{{"len":{}}}]}}"#, led_count));
                                timeout(tokio::time::Duration::from_secs(5), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.state.led_count = Some(led_count);
                                self.broadcast_state();
                            }
                            Some(BoardCommand::ResetSegment) => {
                                // Reset segment to defaults: grp=1, spc=0, of=0
                                let msg = Message::Text(r#"{"seg":[{"id":0,"grp":1,"spc":0,"of":0}]}"#.to_string());
                                timeout(tokio::time::Duration::from_secs(5), write.send(msg))
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

            info!(board_id = %self.id, "WebSocket closed, reconnecting in 5 seconds...");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
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
            if let Some(first_seg) = seg.get(0) {
                // Parse color: state.seg[0].col[0] is the RGB array
                if let Some(col) = first_seg["col"].as_array() {
                    if let Some(color_array) = col.get(0).and_then(|c| c.as_array()) {
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
