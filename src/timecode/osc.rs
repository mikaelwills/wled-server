use std::net::UdpSocket;

use super::{TimecodeOutput, TimecodePosition};

pub struct OscTimecodeOutput {
    name: String,
    ip: String,
    port: u16,
    address_prefix: String,
    socket: Option<UdpSocket>,
    enabled: bool,
    last_sent_second: i64,
}

impl OscTimecodeOutput {
    pub fn new(ip: String, port: u16) -> Self {
        Self {
            name: "osc".to_string(),
            ip,
            port,
            address_prefix: "/timecode".to_string(),
            socket: None,
            enabled: false,
            last_sent_second: -1,
        }
    }

    pub fn with_address_prefix(mut self, prefix: String) -> Self {
        self.address_prefix = prefix;
        self
    }

    pub fn set_target(&mut self, ip: String, port: u16) {
        self.ip = ip;
        self.port = port;
    }

    fn send_osc_message(&self, address: &str, args: Vec<rosc::OscType>) -> Result<(), String> {
        let socket = self.socket.as_ref().ok_or("Socket not initialized")?;

        let packet = rosc::encoder::encode(&rosc::OscPacket::Message(rosc::OscMessage {
            addr: address.to_string(),
            args,
        }))
        .map_err(|e| format!("Failed to encode OSC message: {}", e))?;

        let target = format!("{}:{}", self.ip, self.port);
        socket
            .send_to(&packet, &target)
            .map_err(|e| format!("Failed to send OSC to {}: {}", target, e))?;

        Ok(())
    }
}

impl TimecodeOutput for OscTimecodeOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn start(&mut self) -> Result<(), String> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| format!("Failed to bind OSC socket: {}", e))?;
        self.socket = Some(socket);
        self.last_sent_second = -1;
        eprintln!("[Timecode/OSC] Started, target: {}:{}", self.ip, self.port);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        self.socket = None;
        self.last_sent_second = -1;
        eprintln!("[Timecode/OSC] Stopped");
        Ok(())
    }

    fn send_position(&mut self, position: &TimecodePosition) -> Result<(), String> {
        if self.socket.is_none() {
            return Err("Socket not initialized".to_string());
        }

        let current_second = position.seconds.floor() as i64;
        if current_second == self.last_sent_second {
            return Ok(());
        }
        self.last_sent_second = current_second;

        let position_address = format!("{}/position", self.address_prefix);
        let mut args = vec![
            rosc::OscType::Float(position.seconds as f32),
        ];

        if let Some(bpm) = position.bpm {
            args.push(rosc::OscType::Float(bpm as f32));
        }

        if let Some(beat) = position.beat {
            args.push(rosc::OscType::Float(beat as f32));
        }

        self.send_osc_message(&position_address, args)
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}
