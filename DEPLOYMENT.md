# Deployment Guide

This guide covers deployment options for the WLED Server.

## Deployment Options

### Option 1: Native Deployment (Recommended for Embedded Devices)

For resource-constrained devices like routers or embedded systems, native deployment provides the smallest footprint and fastest performance.

**Benefits:**
- No container overhead (saves ~100MB RAM)
- Lightweight web server options
- Faster startup times
- Better for systems with frequent power cycling

**Prerequisites:**
- Target device with SSH access
- Admin/root privileges
- Development machine for cross-compilation
- Internet connection for package installation

#### Cross-Compilation

Build for your target architecture:

```bash
# Install target (example for ARM64)
rustup target add aarch64-unknown-linux-musl

# Build for target
cargo build --release --target aarch64-unknown-linux-musl
```

Common targets:
- ARM64: `aarch64-unknown-linux-musl`
- ARM32: `armv7-unknown-linux-musleabihf`
- x86_64: `x86_64-unknown-linux-musl`

#### Frontend Build

```bash
cd frontend
bun install
bun run build
```

#### Deployment Steps

1. **Create directories on target device:**
```bash
mkdir -p /opt/wled-server/frontend/build
mkdir -p /opt/wled-server/data
mkdir -p /opt/wled-server/audio
mkdir -p /opt/wled-server/programs
```

2. **Copy files to target:**
```bash
# Copy backend binary
scp target/[TARGET]/release/rust-wled-server user@device:/opt/wled-server/

# Copy frontend
scp -r frontend/build/* user@device:/opt/wled-server/frontend/build/

# Copy configuration examples
scp data/boards.toml.example user@device:/opt/wled-server/data/
```

3. **Set permissions:**
```bash
chmod +x /opt/wled-server/rust-wled-server
```

4. **Configure boards:**
Edit `/opt/wled-server/data/boards.toml`:
```toml
[[boards]]
id = "board-1"
ip = "192.168.1.100"

[[groups]]
id = "all-boards"
members = ["board-1"]
```

#### Service Setup

Create a systemd service file `/etc/systemd/system/wled-server.service`:

```ini
[Unit]
Description=WLED Server
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/wled-server
ExecStart=/opt/wled-server/rust-wled-server
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
systemctl daemon-reload
systemctl enable wled-server
systemctl start wled-server
```

#### Web Server Setup

Serve the frontend using your preferred web server. Example with lighttpd:

```bash
# Install lighttpd
apt-get install lighttpd  # Debian/Ubuntu
# or
opkg install lighttpd     # OpenWrt

# Configure lighttpd to serve /opt/wled-server/frontend/build
# Edit /etc/lighttpd/lighttpd.conf as needed

# Start lighttpd
/etc/init.d/lighttpd start
```

### Option 2: Docker Deployment

For standard Linux servers or NAS devices with Docker support.

See [README-DOCKER.md](README-DOCKER.md) for Docker deployment instructions.

## Configuration

### Environment Variables

- `RUST_LOG`: Set logging level (info, warn, error, debug, trace)
- `API_URL`: Backend API URL (default: http://localhost:3010)

### Ports

- Backend API: 3010
- Frontend: 3011 (or configured web server port)

### Storage Directories

- `data/`: Configuration files (boards.toml)
- `audio/`: Uploaded audio files for programs
- `programs/`: Light program definitions
- `presets/`: Saved preset configurations

## Service Management

### Systemd (Linux)

```bash
# Start service
systemctl start wled-server

# Stop service
systemctl stop wled-server

# Restart service
systemctl restart wled-server

# Check status
systemctl status wled-server

# View logs
journalctl -u wled-server -f
```

### OpenWrt (Embedded)

```bash
# Start service
/etc/init.d/wled-server start

# Stop service
/etc/init.d/wled-server stop

# Restart service
/etc/init.d/wled-server restart

# Enable auto-start
/etc/init.d/wled-server enable
```

## Accessing the Server

Once deployed:
- Frontend: `http://[device-ip]:[frontend-port]`
- API: `http://[device-ip]:3010/api`
- Health Check: `http://[device-ip]:3010/api/health`

## Troubleshooting

### Service Won't Start

```bash
# Check if process is running
ps aux | grep wled-server

# Try manual start to see errors
/opt/wled-server/rust-wled-server

# Check logs (systemd)
journalctl -u wled-server -n 50
```

### Can't Connect to WLED Devices

```bash
# Verify network connectivity
ping [wled-device-ip]

# Check configuration
cat /opt/wled-server/data/boards.toml

# Check server logs for connection errors
```

### Permission Errors

```bash
# Ensure binary is executable
chmod +x /opt/wled-server/rust-wled-server

# Check directory permissions
ls -la /opt/wled-server/
```

## Updating

To update to a new version:

1. Build new version
2. Stop service
3. Replace binary and frontend files
4. Restart service

```bash
systemctl stop wled-server
# Copy new files
systemctl start wled-server
```

## Security Considerations

1. Use SSH keys instead of passwords
2. Keep system packages updated
3. Restrict network access via firewall if needed
4. Change default passwords
5. Monitor logs for unusual activity

## Performance

Typical resource usage:
- Backend RAM: 10-30MB
- Frontend: Static files, minimal server overhead
- CPU: <5% idle, 10-30% during active operations
- Disk: ~15-20MB base install

## Support

For issues or questions:
1. Check service status and logs
2. Verify network connectivity to WLED devices
3. Review configuration files
4. Create an issue in the repository
