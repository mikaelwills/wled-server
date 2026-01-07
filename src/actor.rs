use crate::board::{BoardCommand, BoardState};
use crate::sse::SseEvent;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::HashSet;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

pub struct BoardActor {
    pub id: String,
    pub ip: String,
    pub state: BoardState,
    broadcast_tx: Arc<broadcast::Sender<SseEvent>>,
    connected_ips: Arc<RwLock<HashSet<String>>>,
    performance_mode: Arc<AtomicBool>,
}

impl BoardActor {
    pub fn new(
        id: String,
        ip: String,
        broadcast_tx: Arc<broadcast::Sender<SseEvent>>,
        connected_ips: Arc<RwLock<HashSet<String>>>,
        performance_mode: Arc<AtomicBool>,
    ) -> Self {
        Self {
            id: id.clone(),
            ip: ip.clone(),
            state: BoardState::new(id, ip),
            broadcast_tx,
            connected_ips,
            performance_mode,
        }
    }

    pub fn new_with_config(
        id: String,
        ip: String,
        transition: Option<u8>,
        led_count: Option<u16>,
        universe: Option<u16>,
        broadcast_tx: Arc<broadcast::Sender<SseEvent>>,
        connected_ips: Arc<RwLock<HashSet<String>>>,
        performance_mode: Arc<AtomicBool>,
    ) -> Self {
        let mut state = BoardState::new(id.clone(), ip.clone());
        if let Some(trans) = transition {
            state.transition = trans;
        }
        if let Some(leds) = led_count {
            state.led_count = Some(leds);
        }
        if let Some(uni) = universe {
            state.universe = Some(uni);
        }
        Self {
            id,
            ip,
            state,
            broadcast_tx,
            connected_ips,
            performance_mode,
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

    async fn send_ws_message<S>(
        &mut self,
        write: &mut futures_util::stream::SplitSink<S, Message>,
        msg: Message,
    ) -> bool
    where
        S: futures_util::Sink<Message> + Unpin,
        <S as futures_util::Sink<Message>>::Error: std::fmt::Display,
    {
        if let Err(e) = timeout(tokio::time::Duration::from_secs(2), write.send(msg)).await {
            error!(board_id = %self.id, error = %e, "WebSocket send timed out");
            self.mark_disconnected().await;
            return false;
        }
        if let Err(e) = timeout(tokio::time::Duration::from_millis(500), write.flush()).await {
            error!(board_id = %self.id, error = %e, "WebSocket flush failed");
            self.mark_disconnected().await;
            return false;
        }
        self.broadcast_state();
        true
    }

    async fn mark_connected(&mut self) {
        self.state.connected = true;
        let mut ips = self.connected_ips.write().await;
        ips.insert(self.ip.clone());
        self.broadcast_connection_status();
    }

    async fn mark_disconnected(&mut self) {
        self.state.connected = false;
        let mut ips = self.connected_ips.write().await;
        ips.remove(&self.ip);
        self.broadcast_connection_status();
    }

    async fn wait_for_retry(&mut self, cmd_rx: &mut mpsc::Receiver<BoardCommand>) -> bool {
        let sleep_duration = if self.performance_mode.load(Ordering::Relaxed) {
            tokio::time::Duration::from_secs(30)
        } else {
            tokio::time::Duration::from_secs(3)
        };

        tokio::select! {
            _ = tokio::time::sleep(sleep_duration) => true,
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(BoardCommand::GetState(response_tx)) => {
                        let _ = response_tx.send(self.state.clone());
                        true
                    }
                    Some(BoardCommand::Shutdown) => {
                        info!(board_id = %self.id, "Shutting down actor");
                        false
                    }
                    None => false,
                    _ => true,
                }
            }
        }
    }

    pub async fn run(
        mut self,
        mut cmd_rx: mpsc::Receiver<BoardCommand>,
    ) -> Result<(), Box<dyn Error>> {
        let mut retry_count: u32 = 0;
        loop {
            let url = format!("ws://{}/ws", self.ip);

            let ws_stream = match timeout(tokio::time::Duration::from_secs(2), connect_async(&url)).await {
                Ok(Ok((stream, _))) => {
                    if retry_count > 0 {
                        info!(board_id = %self.id, retry_count, "Connected after {} retries", retry_count);
                    }
                    retry_count = 0;
                    stream
                }
                Ok(Err(e)) => {
                    retry_count += 1;
                    if retry_count == 1 {
                        warn!(board_id = %self.id, error = %e, "Connection failed, will retry every 3s");
                    } else if retry_count % 20 == 0 {
                        warn!(board_id = %self.id, retry_count, "Still trying to connect ({} attempts)", retry_count);
                    }
                    self.mark_disconnected().await;
                    if !self.wait_for_retry(&mut cmd_rx).await {
                        return Ok(());
                    }
                    continue;
                }
                Err(_) => {
                    retry_count += 1;
                    if retry_count % 20 == 0 {
                        warn!(board_id = %self.id, retry_count, "Connect timeout ({} attempts)", retry_count);
                    }
                    self.mark_disconnected().await;
                    if !self.wait_for_retry(&mut cmd_rx).await {
                        return Ok(());
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

            self.mark_connected().await;
            self.broadcast_state();

            // Immediately set transition to 0ms for instant E1.31 commands
            // This ensures boards always start with 0ms transition regardless of WLED default
            info!(board_id = %self.id, "Setting transition to 0ms on connection");
            let init_msg = Message::Text(r#"{"tt":0,"transition":0}"#.to_string());
            if let Err(e) = write.send(init_msg).await {
                warn!(board_id = %self.id, "Failed to set initial transition: {}", e);
            } else {
                self.state.transition = 0;
                self.broadcast_state();
            }

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
                              warn!(board_id = %self.id, "Read timeout, connection dead");
                              self.mark_disconnected().await;
                              break;
                          }
                          Ok(Some(Ok(Message::Text(text)))) => {
                              if let Ok(json) = serde_json::from_str::<Value>(&text) {
                                  self.update_state_from_json(&json);
                                  self.broadcast_state();
                              }
                          }
                          Ok(Some(Ok(Message::Pong(_)))) => {}
                          Ok(Some(Ok(Message::Close(_)))) => {
                              info!(board_id = %self.id, "Connection closed by remote");
                              self.mark_disconnected().await;
                              break;
                          }
                          Ok(Some(Err(e))) => {
                              error!(board_id = %self.id, "Connection lost: {:?}", e);
                              self.mark_disconnected().await;
                              break;
                          }
                          Ok(None) => {
                              info!(board_id = %self.id, "Connection closed");
                              self.mark_disconnected().await;
                              break;
                          }
                          _ => {}
                      }
                  }
                  cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(BoardCommand::SetPower(target_state, _transition)) => {
                                self.state.on = target_state;
                                let msg = Message::Text(format!(r#"{{"on":{},"tt":0}}"#, target_state));
                                if !self.send_ws_message(&mut write, msg).await { break; }
                            }
                            Some(BoardCommand::SetBrightness(bri, _transition)) => {
                                self.state.brightness = bri;
                                let msg = Message::Text(format!(r#"{{"bri":{},"tt":0}}"#, bri));
                                if !self.send_ws_message(&mut write, msg).await { break; }
                            }
                            Some(BoardCommand::SetColor { r, g, b, transition }) => {
                                self.state.color = [r, g, b];
                                let msg = Message::Text(format!(
                                    r#"{{"seg":[{{"col":[[{},{},{}]]}}],"tt":{}}}"#,
                                    r, g, b, transition
                                ));
                                if !self.send_ws_message(&mut write, msg).await { break; }
                            }
                            Some(BoardCommand::SetEffect(effect, _transition)) => {
                                self.state.effect = effect;
                                let msg = Message::Text(format!(r#"{{"seg":[{{"fx":{}}}],"tt":0}}"#, effect));
                                if !self.send_ws_message(&mut write, msg).await { break; }
                            }
                            Some(BoardCommand::SetSpeed(speed, _transition)) => {
                                self.state.speed = speed;
                                let msg = Message::Text(format!(r#"{{"seg":[{{"sx":{}}}],"tt":0}}"#, speed));
                                if !self.send_ws_message(&mut write, msg).await { break; }
                            }
                            Some(BoardCommand::SetIntensity(intensity, _transition)) => {
                                self.state.intensity = intensity;
                                let msg = Message::Text(format!(r#"{{"seg":[{{"ix":{}}}],"tt":0}}"#, intensity));
                                if !self.send_ws_message(&mut write, msg).await { break; }
                            }
                            Some(BoardCommand::SetPreset(preset, _transition)) => {
                                self.state.preset = Some(preset);
                                let msg = Message::Text(format!(r#"{{"ps":{},"tt":0}}"#, preset));
                                if !self.send_ws_message(&mut write, msg).await { break; }
                            }
                            Some(BoardCommand::SetLedCount(led_count)) => {
                                self.state.led_count = Some(led_count);
                                let msg = Message::Text(format!(r#"{{"seg":[{{"len":{}}}]}}"#, led_count));
                                if !self.send_ws_message(&mut write, msg).await { break; }
                            }
                            Some(BoardCommand::ResetSegment) => {
                                let msg = Message::Text(r#"{"seg":[{"id":0,"grp":1,"spc":0,"of":0}]}"#.to_string());
                                if !self.send_ws_message(&mut write, msg).await { break; }
                            }
                            // Sync commands - update state without sending WebSocket (for E1.31 sync)
                            Some(BoardCommand::SyncPowerState(on)) => {
                                self.state.on = on;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SyncBrightnessState(brightness)) => {
                                self.state.brightness = brightness;
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SyncPresetState(preset)) => {
                                self.state.preset = Some(preset);
                                self.broadcast_state();
                            }
                            Some(BoardCommand::SetTransition(transition)) => {
                                self.state.transition = transition;
                                let msg = Message::Text(format!(r#"{{"tt":{},"transition":{}}}"#, transition, transition));
                                if !self.send_ws_message(&mut write, msg).await { break; }
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

        // Read transition value from WLED to show actual board state
        if let Some(transition) = json["state"]["transition"].as_u64() {
            self.state.transition = transition as u8;
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
