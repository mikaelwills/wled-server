use sacn::source::SacnSource;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use tracing::{info, error};

/// E1.31 (sACN) transport for sending WLED commands via UDP
///
/// Uses WLED's E1.31 Preset Mode (mode 10):
/// - Channel 1: Preset ID (0-250)
///
/// Sends unicast packets to each board IP (multicast doesn't work on all networks)
pub struct E131Transport {
    source: SacnSource,
    universe: u16,
    board_ips: Vec<SocketAddr>,
}

impl E131Transport {
    /// Create a new E1.31 transport for unicast to specific board IPs
    ///
    /// # Arguments
    /// * `board_ips` - Vector of board IP addresses (e.g., ["192.168.8.148:5568"])
    /// * `universe` - E1.31 universe number (1-63999)
    ///
    /// # Returns
    /// Result<Self, Box<dyn Error>> - Transport instance or error
    pub fn new(board_ips: Vec<String>, universe: u16) -> Result<Self, Box<dyn Error>> {
        // Parse board IP addresses
        let parsed_ips: Result<Vec<SocketAddr>, _> = board_ips
            .iter()
            .map(|ip_str| {
                // Add port 5568 if not specified
                if ip_str.contains(':') {
                    ip_str.parse()
                } else {
                    format!("{}:5568", ip_str).parse()
                }
            })
            .collect();

        let parsed_ips = parsed_ips?;

        // Bind to any local address on an ephemeral port (not 5568 to avoid conflicts)
        let local_addr = SocketAddr::new(
            IpAddr::V4("0.0.0.0".parse().unwrap()),
            0  // OS assigns an available port
        );

        // Create sACN source with descriptive name and bind to local address
        let mut source = SacnSource::with_ip("WLED Rust Server", local_addr)?;

        // Register the universe before sending
        source.register_universe(universe)?;

        // Configure source
        let _ = source.set_preview_mode(false); // Live data, not preview

        info!(
            universe = universe,
            board_count = parsed_ips.len(),
            "E1.31 transport initialized (unicast to {} boards)", parsed_ips.len()
        );

        Ok(Self {
            source,
            universe,
            board_ips: parsed_ips,
        })
    }

    /// Send preset command via E1.31 Preset Mode (mode 10 - 1 channel)
    ///
    /// # Arguments
    /// * `preset` - WLED preset ID (0-250)
    /// * `_brightness` - Ignored (mode 10 uses preset's own brightness)
    pub fn send_preset(&mut self, preset: u8, _brightness: u8) -> Result<(), Box<dyn Error>> {
        self.send_preset_only(preset)
    }

    /// Send power on/off command via E1.31 (Preset mode - 1 channel)
    ///
    /// # Arguments
    /// * `on` - True for on, false for off
    /// * `preset` - Preset ID to activate when turning on (ignored when off, uses preset 0 for blackout)
    pub fn send_power(&mut self, on: bool, preset: u8) -> Result<(), Box<dyn Error>> {
        if on {
            // Turn on: Activate the specified preset
            self.send_preset_only(preset)
        } else {
            // Turn off: Use preset 0 (blackout preset)
            self.send_preset_only(0)
        }
    }

    /// Send brightness change via E1.31
    ///
    /// # Arguments
    /// * `brightness` - Master brightness (0-255)
    /// * `current_preset` - Current preset ID to maintain
    pub fn send_brightness(&mut self, brightness: u8, current_preset: u8) -> Result<(), Box<dyn Error>> {
        self.send_dmx_data(brightness, current_preset)
    }

    /// Internal: Send single channel preset ID (for mode 10 - Preset mode)
    ///
    /// # Arguments
    /// * `preset` - Preset ID (0-250)
    fn send_preset_only(&mut self, preset: u8) -> Result<(), Box<dyn Error>> {
        // Create 512-byte DMX buffer (sACN standard)
        let mut dmx_data = [0u8; 512];

        // Set WLED Preset Mode channel (1-indexed in DMX, 0-indexed in array)
        dmx_data[0] = preset;  // Channel 1: Preset ID

        info!(
            universe = self.universe,
            preset = preset,
            board_count = self.board_ips.len(),
            "Sending E1.31 preset-only data (mode 10) to {} boards", self.board_ips.len()
        );

        // Send unicast to each board IP
        // Note: Send to ALL boards even if one fails, log errors but don't abort
        let mut success_count = 0;
        for board_ip in &self.board_ips {
            match self.source.send(
                &[self.universe],      // Array of universes
                &dmx_data,             // Data buffer
                Some(100),             // Priority (1-200)
                Some(*board_ip),       // Unicast to specific board IP
                None                   // No synchronization address
            ) {
                Ok(_) => {
                    info!(board_ip = %board_ip, preset = preset, "Sent E1.31 unicast packet");
                    success_count += 1;
                }
                Err(e) => {
                    error!(board_ip = %board_ip, preset = preset, error = %e, "Failed to send E1.31 unicast packet (board may be offline)");
                }
            }
        }

        // Success if at least one board received the packet
        if success_count > 0 {
            info!(success_count = success_count, total = self.board_ips.len(), "E1.31 unicast completed ({}/{} boards)", success_count, self.board_ips.len());
            Ok(())
        } else {
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to send to any boards"
            )))
        }
    }

    /// Internal: Send DMX data packet to WLED board
    ///
    /// # Arguments
    /// * `channel1` - Brightness (0-255)
    /// * `channel2` - Preset ID (0-250)
    fn send_dmx_data(&mut self, channel1: u8, channel2: u8) -> Result<(), Box<dyn Error>> {
        // Create 512-byte DMX buffer (sACN standard)
        let mut dmx_data = [0u8; 512];

        // Set WLED Preset Mode channels (1-indexed in DMX, 0-indexed in array)
        dmx_data[0] = channel1;  // Channel 1: Master Brightness
        dmx_data[1] = channel2;  // Channel 2: Preset ID

        info!(
            universe = self.universe,
            ch1_brightness = channel1,
            ch2_preset = channel2,
            board_count = self.board_ips.len(),
            "Sending E1.31 DMX data to {} boards", self.board_ips.len()
        );

        // Send unicast to each board IP
        // Note: Send to ALL boards even if one fails, log errors but don't abort
        let mut success_count = 0;
        for board_ip in &self.board_ips {
            match self.source.send(
                &[self.universe],      // Array of universes
                &dmx_data,             // Data buffer
                Some(100),             // Priority (1-200)
                Some(*board_ip),       // Unicast to specific board IP
                None                   // No synchronization address
            ) {
                Ok(_) => {
                    info!(board_ip = %board_ip, ch1_brightness = channel1, ch2_preset = channel2, "Sent E1.31 unicast packet");
                    success_count += 1;
                }
                Err(e) => {
                    error!(board_ip = %board_ip, ch1_brightness = channel1, ch2_preset = channel2, error = %e, "Failed to send E1.31 unicast packet (board may be offline)");
                }
            }
        }

        // Success if at least one board received the packet
        if success_count > 0 {
            info!(success_count = success_count, total = self.board_ips.len(), "E1.31 unicast completed ({}/{} boards)", success_count, self.board_ips.len());
            Ok(())
        } else {
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to send to any boards"
            )))
        }
    }
}

impl Drop for E131Transport {
    fn drop(&mut self) {
        // Terminate E1.31 stream cleanly when transport is dropped
        if let Err(e) = self.source.terminate_stream(self.universe, 0) {
            error!(
                universe = self.universe,
                error = %e,
                "Failed to terminate E1.31 stream"
            );
        }
    }
}
