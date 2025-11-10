use std::error::Error;
use std::net::{SocketAddr, UdpSocket};
use tracing::{info, error};

/// Raw E1.31 (sACN) transport using hand-crafted packets
/// This bypasses the sacn library which appears incompatible with WLED
pub struct E131RawTransport {
    socket: UdpSocket,
    universe: u16,
    board_ips: Vec<SocketAddr>,
    sequence: u8,
}

impl E131RawTransport {
    /// Create a new raw E1.31 transport
    pub fn new(board_ips: Vec<String>, universe: u16) -> Result<Self, Box<dyn Error>> {
        // Parse board IP addresses
        let parsed_ips: Result<Vec<SocketAddr>, _> = board_ips
            .iter()
            .map(|ip_str| {
                if ip_str.contains(':') {
                    ip_str.parse()
                } else {
                    format!("{}:5568", ip_str).parse()
                }
            })
            .collect();

        let parsed_ips = parsed_ips?;

        // Create UDP socket bound to any available port
        let socket = UdpSocket::bind("0.0.0.0:0")?;

        info!(
            universe = universe,
            board_count = parsed_ips.len(),
            "E1.31 raw transport initialized (unicast to {} boards)", parsed_ips.len()
        );

        Ok(Self {
            socket,
            universe,
            board_ips: parsed_ips,
            sequence: 0,
        })
    }

    /// Send preset command (mode 10 - uses 2 channels)
    /// Channel 0: Brightness (ignored if 0, preset uses own brightness)
    /// Channel 1: Preset ID (1-250)
    pub fn send_preset(&mut self, preset: u8, brightness: u8) -> Result<(), Box<dyn Error>> {
        // Create 512-byte DMX data
        let mut dmx_data = vec![0u8; 512];
        dmx_data[0] = brightness;  // Channel 0: Master brightness (0 = use preset's brightness)
        dmx_data[1] = preset;      // Channel 1: Preset ID

        self.send_dmx_packet(&dmx_data)
    }

    /// Send power command
    /// For power off, we set brightness to 0 (dims to black)
    /// For power on, we activate the specified preset with full brightness
    pub fn send_power(&mut self, on: bool, preset: u8) -> Result<(), Box<dyn Error>> {
        if on {
            // Turn on: Send preset with full brightness (255)
            // Channel 0 controls the master dimmer, so we need a non-zero value
            self.send_preset(preset, 255)
        } else {
            // Turn off: Set brightness to 0 (blackout)
            let mut dmx_data = vec![0u8; 512];
            dmx_data[0] = 0;  // Channel 0: Brightness = 0 (off)
            dmx_data[1] = preset;  // Channel 1: Keep current preset
            self.send_dmx_packet(&dmx_data)
        }
    }

    /// Send brightness command
    /// Sets master brightness (channel 0) while maintaining current preset
    pub fn send_brightness(&mut self, brightness: u8, current_preset: u8) -> Result<(), Box<dyn Error>> {
        let mut dmx_data = vec![0u8; 512];
        dmx_data[0] = brightness;      // Channel 0: Master brightness
        dmx_data[1] = current_preset;  // Channel 1: Maintain current preset
        self.send_dmx_packet(&dmx_data)
    }

    /// Internal: Send raw E1.31 packet with DMX data
    fn send_dmx_packet(&mut self, dmx_data: &[u8]) -> Result<(), Box<dyn Error>> {
        let packet = self.create_e131_packet(dmx_data)?;

        info!(
            universe = self.universe,
            dmx_ch0 = dmx_data[0],
            dmx_ch1 = dmx_data[1],
            board_count = self.board_ips.len(),
            "Sending raw E1.31 packet to {} boards (ch0=brightness, ch1=preset)", self.board_ips.len()
        );

        let mut success_count = 0;
        for board_ip in &self.board_ips {
            match self.socket.send_to(&packet, board_ip) {
                Ok(bytes_sent) => {
                    info!(
                        board_ip = %board_ip,
                        bytes = bytes_sent,
                        dmx_ch0 = dmx_data[0],
                        dmx_ch1 = dmx_data[1],
                        "Sent raw E1.31 packet"
                    );
                    success_count += 1;
                }
                Err(e) => {
                    error!(
                        board_ip = %board_ip,
                        error = %e,
                        "Failed to send raw E1.31 packet"
                    );
                }
            }
        }

        // Increment sequence number (wraps at 255)
        self.sequence = self.sequence.wrapping_add(1);

        if success_count > 0 {
            info!(
                success_count = success_count,
                total = self.board_ips.len(),
                "Raw E1.31 unicast completed ({}/{} boards)", success_count, self.board_ips.len()
            );
            Ok(())
        } else {
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to send to any boards"
            )))
        }
    }

    /// Create E1.31 (sACN) packet
    fn create_e131_packet(&self, dmx_data: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut packet = Vec::with_capacity(638);

        // Root Layer (38 bytes)
        packet.extend_from_slice(&[0x00, 0x10]); // Preamble size
        packet.extend_from_slice(&[0x00, 0x00]); // Postamble size
        packet.extend_from_slice(b"ASC-E1.17\0\0\0"); // ACN Packet Identifier (12 bytes)

        // Framing layer length
        let framing_length = 88 + dmx_data.len();
        let root_length = (framing_length + 38 - 16) as u16;
        packet.extend_from_slice(&((0x7000 | root_length).to_be_bytes())); // Flags + Length

        packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x04]); // Vector: VECTOR_ROOT_E131_DATA

        // CID (16-byte UUID) - static for this server
        packet.extend_from_slice(&[
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
        ]);

        // Framing Layer (77 bytes + universe)
        let framing_flags_length = (0x7000 | framing_length as u16).to_be_bytes();
        packet.extend_from_slice(&framing_flags_length);

        packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x02]); // Vector: VECTOR_E131_DATA_PACKET

        // Source name (64 bytes, null-padded)
        let source_name = b"WLED Rust Server";
        packet.extend_from_slice(source_name);
        packet.extend_from_slice(&vec![0u8; 64 - source_name.len()]);

        packet.push(100); // Priority
        packet.extend_from_slice(&[0x00, 0x00]); // Sync address (reserved)
        packet.push(self.sequence); // Sequence number
        packet.push(0x00); // Options (no preview, no stream_terminated)
        packet.extend_from_slice(&self.universe.to_be_bytes()); // Universe

        // DMP Layer
        let dmp_length = (11 + dmx_data.len()) as u16;
        packet.extend_from_slice(&((0x7000 | dmp_length).to_be_bytes())); // Flags + Length

        packet.push(0x02); // Vector: VECTOR_DMP_SET_PROPERTY
        packet.push(0xa1); // Address & Data Type
        packet.extend_from_slice(&[0x00, 0x00]); // First Property Address
        packet.extend_from_slice(&[0x00, 0x01]); // Address Increment
        packet.extend_from_slice(&((dmx_data.len() + 1) as u16).to_be_bytes()); // Property value count
        packet.push(0x00); // DMX512-A START Code

        // DMX Data
        packet.extend_from_slice(dmx_data);

        Ok(packet)
    }
}
