use std::error::Error;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use tracing::info;

use crate::timing_metrics::TimingMetrics;

const SEQUENCE_OFFSET: usize = 111;
const DMX_DATA_OFFSET: usize = 126;
const PACKET_SIZE: usize = 638;

pub struct E131RawTransport {
    socket: Arc<UdpSocket>,
    target_addr: SocketAddr,
    universe: u16,
    sequence: u8,
    packet: [u8; PACKET_SIZE],
    timing_metrics: Option<Arc<TimingMetrics>>,
}

impl E131RawTransport {
    pub fn create_socket() -> Result<UdpSocket, Box<dyn Error>> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        Ok(socket)
    }

    pub fn new(board_ips: Vec<String>, universe: u16) -> Result<Self, Box<dyn Error>> {
        let socket = Arc::new(Self::create_socket()?);
        Self::with_socket(socket, &board_ips, universe)
    }

    pub fn with_socket(socket: Arc<UdpSocket>, board_ips: &[String], universe: u16) -> Result<Self, Box<dyn Error>> {
        let target_addr = Self::resolve_target_addr(board_ips)?;
        let packet = Self::build_header_template(universe);

        info!(
            universe = universe,
            target = %target_addr,
            "E1.31 unicast transport: universe {} → {}",
            universe, target_addr
        );

        Ok(Self {
            socket,
            target_addr,
            universe,
            sequence: 0,
            packet,
            timing_metrics: None,
        })
    }

    pub fn set_timing_metrics(&mut self, metrics: Arc<TimingMetrics>) {
        self.timing_metrics = Some(metrics);
    }

    fn build_header_template(universe: u16) -> [u8; PACKET_SIZE] {
        let mut p = [0u8; PACKET_SIZE];

        p[0..2].copy_from_slice(&[0x00, 0x10]); // Preamble
        p[2..4].copy_from_slice(&[0x00, 0x00]); // Postamble
        p[4..16].copy_from_slice(b"ASC-E1.17\0\0\0"); // ACN ID
        p[16..18].copy_from_slice(&(0x7000u16 | 622).to_be_bytes()); // Root flags+length (622 = 600+38-16)
        p[18..22].copy_from_slice(&[0x00, 0x00, 0x00, 0x04]); // VECTOR_ROOT_E131_DATA
        p[22..38].copy_from_slice(&[
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
        ]); // CID
        p[38..40].copy_from_slice(&(0x7000u16 | 600).to_be_bytes()); // Framing flags+length
        p[40..44].copy_from_slice(&[0x00, 0x00, 0x00, 0x02]); // VECTOR_E131_DATA_PACKET
        p[44..60].copy_from_slice(b"WLED Rust Server"); // Source name (16 bytes, rest stays 0)
        p[108] = 100; // Priority
        // [109..111] Sync address = 0
        // [111] Sequence = 0 (updated per packet)
        // [112] Options = 0
        p[113..115].copy_from_slice(&universe.to_be_bytes()); // Universe
        p[115..117].copy_from_slice(&(0x7000u16 | 523).to_be_bytes()); // DMP flags+length (523 = 11+512)
        p[117] = 0x02; // VECTOR_DMP_SET_PROPERTY
        p[118] = 0xa1; // Address type
        // [119..121] First property address = 0
        p[121..123].copy_from_slice(&[0x00, 0x01]); // Address increment
        p[123..125].copy_from_slice(&513u16.to_be_bytes()); // Property count
        // [125] DMX start code = 0
        // [126..638] DMX data = 0

        p
    }

    fn resolve_target_addr(board_ips: &[String]) -> Result<SocketAddr, Box<dyn Error>> {
        if let Some(first_ip) = board_ips.first() {
            let ip_part = first_ip.split(':').next().unwrap_or(first_ip);
            let addr = format!("{}:5568", ip_part);
            return Ok(addr.parse()?);
        }
        Err("No board IP provided".into())
    }

    pub fn send_dmx_packet(&mut self, dmx_data: &[u8; 512]) -> Result<(), Box<dyn Error>> {
        self.packet[SEQUENCE_OFFSET] = self.sequence;
        self.packet[DMX_DATA_OFFSET..].copy_from_slice(dmx_data);

        match self.socket.send_to(&self.packet, self.target_addr) {
            Ok(_) => {
                if let Some(ref metrics) = self.timing_metrics {
                    metrics.record_packet_ok();
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if let Some(ref metrics) = self.timing_metrics {
                    metrics.record_packet_wouldblock();
                }
            }
            Err(_) => {
                if let Some(ref metrics) = self.timing_metrics {
                    metrics.record_packet_err();
                }
            }
        }

        self.sequence = self.sequence.wrapping_add(1);

        Ok(())
    }

    pub fn universe(&self) -> u16 {
        self.universe
    }

    pub fn target_addr(&self) -> SocketAddr {
        self.target_addr
    }

    pub fn send_raw_leds(&mut self, led_count: usize, r: u8, g: u8, b: u8) -> Result<(), Box<dyn Error>> {
        let mut dmx_data = [0u8; 512];
        let count = led_count.min(128);

        for i in 0..count {
            let offset = i * 4;
            dmx_data[offset] = r;
            dmx_data[offset + 1] = g;
            dmx_data[offset + 2] = b;
            dmx_data[offset + 3] = 0; // White channel
        }

        self.send_dmx_packet(&dmx_data)
    }

    pub fn send_solid_color(&mut self, r: u8, g: u8, b: u8, brightness: u8) -> Result<(), Box<dyn Error>> {
        let r_scaled = ((r as u16 * brightness as u16) / 255) as u8;
        let g_scaled = ((g as u16 * brightness as u16) / 255) as u8;
        let b_scaled = ((b as u16 * brightness as u16) / 255) as u8;
        self.send_raw_leds(128, r_scaled, g_scaled, b_scaled)
    }

    pub fn send_blackout(&mut self) -> Result<(), Box<dyn Error>> {
        self.send_raw_leds(128, 0, 0, 0)
    }

    pub fn send_led_buffer(&mut self, led_data: &[[u8; 3]]) -> Result<(), Box<dyn Error>> {
        let mut dmx_data = [0u8; 512];
        let count = led_data.len().min(128);

        for i in 0..count {
            let offset = i * 4;
            dmx_data[offset] = led_data[i][0];
            dmx_data[offset + 1] = led_data[i][1];
            dmx_data[offset + 2] = led_data[i][2];
            dmx_data[offset + 3] = 0;
        }

        self.send_dmx_packet(&dmx_data)
    }
}
