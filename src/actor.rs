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
                    eprintln!(
                        "[{}] Failed to connect: {}, retrying in 5 seconds...",
                        self.id, e
                    );
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
                                Some(BoardCommand::Shutdown) => {
                                    println!("[{}] Shutting down actor", self.id);
                                    return Ok(());
                                }
                                None => return Ok(()),
                                _ => {} // Ignore other commands while disconnected
                            }
                        }
                    }
                    continue;
                }
            };

            println!("[{}] Connected to WLED", self.id);

            // Mark as connected and broadcast status
            self.state.connected = true;
            self.broadcast_connection_status();

            let (mut write, mut read) = ws_stream.split();

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
                              eprintln!("[{}] Read timeout, connection dead", self.id);
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
                              println!("[{}] Connection closed by remote", self.id);
                              self.state.connected = false;
                              self.broadcast_connection_status();
                              break;
                          }
                          Ok(Some(Err(e))) => {
                              let elapsed = last_message_time.elapsed().as_secs();
                              eprintln!("[{}] Connection lost after {}s: {:?}", self.id, elapsed, e);
                              self.state.connected = false;
                              self.broadcast_connection_status();
                              break;
                          }
                          Ok(None) => {
                              println!("[{}] Connection closed", self.id);
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
                            Some(BoardCommand::TogglePower) => {
                                self.state.on = !self.state.on;
                                let msg = Message::Text(format!(r#"{{"on":{}}}"#, self.state.on));
                                timeout(tokio::time::Duration::from_secs(5), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetBrightness(bri)) => {
                                self.state.brightness = bri;
                                let msg = Message::Text(format!(r#"{{"bri":{}}}"#, bri));
                                timeout(tokio::time::Duration::from_secs(5), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetColor { r, g, b }) => {
                                self.state.color = [r, g, b];
                                let msg = Message::Text(format!(
                                    r#"{{"seg":[{{"col":[[{},{},{}]]}}]}}"#,
                                    r, g, b
                                ));
                                timeout(tokio::time::Duration::from_secs(5), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetEffect(effect)) => {
                                self.state.effect = effect;
                                let msg = Message::Text(format!(r#"{{"seg":[{{"fx":{}}}]}}"#, effect));
                                timeout(tokio::time::Duration::from_secs(5), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetPreset(preset)) => {
                                let msg = Message::Text(format!(r#"{{"ps":{}}}"#, preset));
                                timeout(tokio::time::Duration::from_secs(5), write.send(msg))
                                    .await
                                    .map_err(|_| "Timeout")??;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::GetState(response_tx)) => {
                                let _ = response_tx.send(self.state.clone());
                            }
                            Some(BoardCommand::Shutdown) => {
                                println!("[{}] Shutting down actor", self.id);
                                return Ok(());
                            }
                            None => return Ok(()),
                        }
                    }
                }
            }

            println!(
                "WebSocket closed for {}, reconnecting in 5 seconds...",
                self.id
            );
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
            }
        }
    }
}
