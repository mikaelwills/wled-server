use sacn::source::SacnSource;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use tracing::{info, error};

/// E1.31 (sACN) transport for sending WLED commands via UDP
///
/// Uses WLED's E1.31 Preset Mode (2 channels):
/// - Channel 1: Master Brightness (0-255)
/// - Channel 2: Preset ID (0-250)
///
/// Sends to multicast group (standard E1.31 behavior)
pub struct E131Transport {
    source: SacnSource,
    universe: u16,
}

impl E131Transport {
    /// Create a new E1.31 transport for multicast group
    ///
    /// # Arguments
    /// * `_ip` - Unused (kept for API compatibility, multicast doesn't need specific IP)
    /// * `universe` - E1.31 universe number (1-63999)
    ///
    /// # Returns
    /// Result<Self, Box<dyn Error>> - Transport instance or error
    pub fn new(_ip: &str, universe: u16) -> Result<Self, Box<dyn Error>> {
        // Bind to any local address on port 5568
        let local_addr = SocketAddr::new(
            IpAddr::V4("0.0.0.0".parse().unwrap()),
            5568
        );

        // Create sACN source with descriptive name and bind to local address
        let mut source = SacnSource::with_ip("WLED Rust Server", local_addr)?;

        // Register the universe before sending
        source.register_universe(universe)?;

        // Configure source
        let _ = source.set_preview_mode(false); // Live data, not preview

        info!(
            universe = universe,
            "E1.31 transport initialized (multicast)"
        );

        Ok(Self {
            source,
            universe,
        })
    }

    /// Send preset command via E1.31 Preset Mode
    ///
    /// # Arguments
    /// * `preset` - WLED preset ID (0-250)
    /// * `brightness` - Master brightness (0-255)
    pub fn send_preset(&mut self, preset: u8, brightness: u8) -> Result<(), Box<dyn Error>> {
        self.send_dmx_data(brightness, preset)
    }

    /// Send power on/off command via E1.31
    ///
    /// # Arguments
    /// * `on` - True for on, false for off
    /// * `brightness` - Brightness to use when turning on (ignored when off)
    pub fn send_power(&mut self, on: bool, brightness: u8) -> Result<(), Box<dyn Error>> {
        if on {
            // Turn on: Set brightness to desired level, preset 0 (current state)
            self.send_dmx_data(brightness, 0)
        } else {
            // Turn off: Set brightness to 0
            self.send_dmx_data(0, 0)
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

        // Send to universe with priority 100 (default DMX priority)
        self.source.send(
            &[self.universe],      // Array of universes
            &dmx_data,             // Data buffer
            Some(100),             // Priority (1-200)
            None,                  // Use multicast (standard E1.31 behavior)
            None                   // No synchronization address
        )?;

        Ok(())
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
