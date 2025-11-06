# GL.iNet Beryl Router Deployment Guide (Native)

This guide explains how to deploy the WLED Rust Server **natively** on your GL.iNet Beryl router **without Docker**.

> **Note:** For Synology NAS Docker deployment, see [README-DOCKER.md](README-DOCKER.md)

## Why Native Deployment for Beryl?

The Beryl router has limited resources (RAM, storage). Native deployment:
- **No Docker overhead** - saves ~100MB RAM
- **Lightweight web server** - lighttpd uses ~1MB vs Python's ~50MB
- **Faster startup** - no container initialization
- **Better for power cycling** - survives power loss (no graceful shutdown needed)

## Prerequisites

1. **GL.iNet Beryl Router** with SSH access enabled
2. **Admin access** to router settings
3. **Development machine** for cross-compilation (Mac/Linux)
4. **Internet connection** on router for package installation

## Architecture

**Backend:** Rust server (port 3010) - ARM64 binary with MUSL static linking
**Frontend:** lighttpd web server (port 3011) - serves static Svelte build
**Service Management:** OpenWrt procd - auto-start and restart management
**Logging:** Structured logs via tracing, captured by syslog/logread

## Automated Deployment

Use the deployment script from your development machine:

```bash
./deploy-to-beryl.sh
```

This script will:
1. Cross-compile Rust backend for ARM64 (aarch64-unknown-linux-musl)
2. Build frontend with Bun (static files)
3. Deploy via rsync to router
4. Install and configure lighttpd
5. Set up procd service for auto-start
6. Start the service

## Manual Deployment Steps

If you need to deploy manually:

### Step 1: Cross-Compile Backend

On your development machine:

```bash
# Install ARM64 MUSL target
rustup target add aarch64-unknown-linux-musl

# Build for ARM64
cargo build --release --target aarch64-unknown-linux-musl
```

### Step 2: Build Frontend

```bash
cd frontend
bun install
bun run build
```

### Step 3: SSH to Router

```bash
ssh root@192.168.8.1
```

### Step 4: Install lighttpd

```bash
opkg update
opkg install lighttpd lighttpd-mod-rewrite
```

### Step 5: Create Directories

```bash
mkdir -p /etc/wled-server/frontend/build
mkdir -p /etc/wled-server/programs
mkdir -p /var/log
```

### Step 6: Copy Files

From your development machine:

```bash
# Copy backend binary
scp target/aarch64-unknown-linux-musl/release/rust-wled-server \
    root@192.168.8.1:/etc/wled-server/

# Copy frontend build
scp -r frontend/build/* \
    root@192.168.8.1:/etc/wled-server/frontend/build/

# Copy init script
scp wled-server.init \
    root@192.168.8.1:/etc/init.d/wled-server

# Copy lighttpd config
scp lighttpd.conf \
    root@192.168.8.1:/etc/lighttpd/lighttpd.conf

# Copy wrapper script
scp wled-server-wrapper.sh \
    root@192.168.8.1:/etc/wled-server/
```

### Step 7: Set Permissions

On router:

```bash
chmod +x /etc/wled-server/rust-wled-server
chmod +x /etc/wled-server/wled-server-wrapper.sh
chmod +x /etc/init.d/wled-server
```

### Step 8: Enable and Start Service

```bash
/etc/init.d/wled-server enable
/etc/init.d/wled-server start
```

## Configuration

### Boards Configuration

Edit `/etc/wled-server/boards.toml`:

```toml
[[boards]]
id = "desk-lights"
ip = "192.168.1.100"

[[boards]]
id = "shelf-lights"
ip = "192.168.1.101"

[[groups]]
id = "all-lights"
members = ["desk-lights", "shelf-lights"]
```

After editing, restart the service:
```bash
/etc/init.d/wled-server restart
```

### Log Configuration

Logs are captured by OpenWrt syslog. View with:

```bash
# View all logs
logread | grep wled

# View real-time logs
logread -f | grep wled

# View last 50 lines
logread | grep wled | tail -50
```

Set log level via environment variable in wrapper script:
```bash
export RUST_LOG=info  # info, warn, error, debug, trace
```

## Service Management

```bash
# Start service
/etc/init.d/wled-server start

# Stop service
/etc/init.d/wled-server stop

# Restart service
/etc/init.d/wled-server restart

# Check status
/etc/init.d/wled-server status

# Enable auto-start on boot
/etc/init.d/wled-server enable

# Disable auto-start
/etc/init.d/wled-server disable
```

## Accessing the Server

Once deployed:
- **Frontend**: http://192.168.8.1:3011
- **API**: http://192.168.8.1:3010/api
- **Health Check**: http://192.168.8.1:3010/api/health

## Troubleshooting

### Service Won't Start

```bash
# Check if process is running
ps | grep wled-server

# Check init script status
/etc/init.d/wled-server status

# View logs
logread | grep wled | tail -20

# Try manual start to see errors
/etc/wled-server/rust-wled-server
```

### lighttpd Errors

```bash
# Check lighttpd status
/etc/init.d/lighttpd status

# Restart lighttpd
/etc/init.d/lighttpd restart

# Check lighttpd config
lighttpd -t -f /etc/lighttpd/lighttpd.conf

# View lighttpd logs
logread | grep lighttpd
```

### Can't Connect to WLED Devices

```bash
# Check network connectivity
ping <wled-device-ip>

# Check if boards.toml exists
cat /etc/wled-server/boards.toml

# Check server logs for connection errors
logread | grep -i "connection\|wled" | tail -20
```

### Out of Space

```bash
# Check disk space
df -h

# Check overlay space
df -h | grep overlay

# Clean up old files
rm -rf /tmp/*
opkg clean
```

### Auto-Restart Issues

The service is configured to auto-restart on failure (max 10 restarts in 6 hours).

Check restart count:
```bash
logread | grep wled-server | grep respawn
```

If service stopped after too many restarts:
```bash
# Reset by restarting manually
/etc/init.d/wled-server restart
```

## Performance Monitoring

```bash
# Check memory usage
free -h

# Check CPU usage
top -n 1 | grep wled

# Monitor network connections
netstat -tn | grep -E ':(3010|3011)'
```

## Updating the Server

To update to a new version:

```bash
# From your development machine
./deploy-to-beryl.sh
```

This will:
1. Build new version
2. Stop old service
3. Deploy new files
4. Restart service

**Note:** Power cycling the router is safe - the service will auto-start on boot.

## Security Considerations

1. **Change default router password**
2. **Use SSH keys** instead of passwords
3. **Update router firmware** regularly
4. **Restrict access** via firewall if needed
5. **Monitor logs** for unusual activity

## Resource Usage

Typical resource footprint on Beryl:

- **Backend RAM**: ~10-20MB
- **lighttpd RAM**: ~1MB
- **Total disk space**: ~15MB (binary + frontend)
- **CPU**: <5% idle, 10-20% during operations

## Known Limitations

1. **Programs stored in RAM** - Lost on power cycle (use NAS or USB storage for persistence)
2. **No graceful shutdown** - Safe because state is cached and synced on reconnect
3. **Limited concurrent connections** - Router CPU/RAM constrained

## Future Enhancements

When USB storage is added:
- Move programs to persistent storage
- Add resource limits (CPU/memory)
- Enable log rotation to USB
- Store more data persistently

## Support

If you encounter issues:

1. Check logs: `logread | grep wled`
2. Verify service status: `/etc/init.d/wled-server status`
3. Test manually: `/etc/wled-server/rust-wled-server`
4. Check network: `ping <wled-device-ip>`
5. Verify disk space: `df -h`

For more help, create an issue in the repository.
