use std::error::Error;
use std::net::{SocketAddr, UdpSocket};
use tracing::info;

const SEQUENCE_OFFSET: usize = 111;
const DMX_DATA_OFFSET: usize = 126;
const PACKET_SIZE: usize = 638;

pub struct E131RawTransport {
    socket: UdpSocket,
    broadcast_addr: SocketAddr,
    universe: u16,
    sequence: u8,
    send_ok: u32,
    send_wouldblock: u32,
    send_err: u32,
    packet: [u8; PACKET_SIZE],
}

impl E131RawTransport {
    pub fn new(board_ips: Vec<String>, universe: u16) -> Result<Self, Box<dyn Error>> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_broadcast(true)?;

        let broadcast_addr = Self::derive_broadcast_addr(&board_ips)?;
        let packet = Self::build_header_template(universe);

        info!(
            universe = universe,
            broadcast = %broadcast_addr,
            board_count = board_ips.len(),
            "E1.31 broadcast transport: universe {} → {} ({} boards)",
            universe, broadcast_addr, board_ips.len()
        );

        Ok(Self {
            socket,
            broadcast_addr,
            universe,
            sequence: 0,
            send_ok: 0,
            send_wouldblock: 0,
            send_err: 0,
            packet,
        })
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

    fn derive_broadcast_addr(board_ips: &[String]) -> Result<SocketAddr, Box<dyn Error>> {
        if let Some(first_ip) = board_ips.first() {
            let ip_part = first_ip.split(':').next().unwrap_or(first_ip);
            let octets: Vec<&str> = ip_part.split('.').collect();
            if octets.len() == 4 {
                let broadcast = format!("{}.{}.{}.255:5568", octets[0], octets[1], octets[2]);
                return Ok(broadcast.parse()?);
            }
        }
        Ok("192.168.8.255:5568".parse()?)
    }

    pub fn send_dmx_packet(&mut self, dmx_data: &[u8; 512]) -> Result<(), Box<dyn Error>> {
        self.packet[SEQUENCE_OFFSET] = self.sequence;
        self.packet[DMX_DATA_OFFSET..].copy_from_slice(dmx_data);

        match self.socket.send_to(&self.packet, self.broadcast_addr) {
            Ok(_) => { self.send_ok += 1; }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => { self.send_wouldblock += 1; }
            Err(_) => { self.send_err += 1; }
        }

        self.sequence = self.sequence.wrapping_add(1);

        if self.sequence == 0 {
            info!(
                ok = self.send_ok,
                wouldblock = self.send_wouldblock,
                err = self.send_err,
                "E1.31 stats (last 256 packets)"
            );
            self.send_ok = 0;
            self.send_wouldblock = 0;
            self.send_err = 0;
        }

        Ok(())
    }

    pub fn universe(&self) -> u16 {
        self.universe
    }

    pub fn broadcast_addr(&self) -> SocketAddr {
        self.broadcast_addr
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
