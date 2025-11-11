# WLED Server - Docker Deployment Guide

This guide covers deploying the WLED Server using Docker.

## Architecture

- **Single Container** running both services:
  - Rust backend on port 3010
  - lighttpd serving static frontend on port 3011
- **Volume Mounts**: Configuration and data persist across container restarts
- **Auto-restart**: Container automatically restarts on crashes or reboots
- **Pre-built Binaries**: Container uses cross-compiled backend and pre-built frontend (no build tools in image)

## Prerequisites

### On Target Host
- Docker and Docker Compose installed
- SSH access (for remote deployment)
- Sufficient storage for application and data

### On Development Machine
- Docker (for local testing)
- SSH access to target host configured (for remote deployment)
- rsync installed (for deployment scripts)

## Quick Start

### Local Development

1. **Start the services:**
```bash
docker-compose up -d --build
```

2. **Create/Edit boards.toml:**
```bash
cp data/boards.toml.example data/boards.toml
nano data/boards.toml
```

Add your WLED boards:
```toml
[[boards]]
id = "board-1"
ip = "192.168.1.100"

[[groups]]
id = "all-boards"
members = ["board-1"]
```

3. **Access the application:**
- Frontend: `http://localhost:3011`
- Backend API: `http://localhost:3010`

### Remote Deployment

For remote deployment, you'll need to create a deployment script that:
1. Syncs project files to the target host
2. Builds Docker images on the target
3. Starts the containers

Example workflow:
```bash
# Sync files to remote host
rsync -avz --exclude 'target' --exclude 'node_modules' \
  ./ user@remote-host:/path/to/wled-server/

# SSH to host and deploy
ssh user@remote-host "cd /path/to/wled-server && docker-compose up -d --build"
```

## Docker Commands

### View Logs
```bash
# All logs
docker-compose logs -f

# Backend only
docker-compose logs -f backend

# Frontend only
docker-compose logs -f frontend
```

### Check Status
```bash
docker-compose ps
```

### Restart Services
```bash
docker-compose restart
```

### Stop Services
```bash
docker-compose down
```

### Rebuild and Restart
```bash
docker-compose down
docker-compose up -d --build
```

### Access Container Shell
```bash
docker exec -it wled-server bash
```

### Edit Configuration
```bash
nano data/boards.toml
docker-compose restart
```

## Configuration

### Volume Mounts

The following directories are mounted as volumes:
- `./data`: Configuration files (boards.toml)
- `./audio`: Uploaded audio files for programs
- `./programs`: Light program definitions
- `./presets`: Saved preset configurations

### Environment Variables

Edit `docker-compose.yml` to configure environment variables:

```yaml
environment:
  - RUST_LOG=info  # Log level: debug, info, warn, error
  - API_URL=http://localhost:3010  # Backend API URL
```

### Ports

Default ports can be changed in `docker-compose.yml`:

```yaml
ports:
  - "3010:3010"  # Backend API
  - "3011:3011"  # Frontend
```

## Network Access

The application will be accessible from:
- **Same machine**: `http://localhost:3011`
- **Local network**: `http://[host-ip]:3011`
- **Mobile devices**: Access via the host's IP address

The frontend automatically connects to the backend using the browser's hostname.

## Troubleshooting

### Container Won't Start

Check logs for errors:
```bash
docker-compose logs
```

### Port Already in Use

Check what's using the port:
```bash
# Linux/macOS
sudo lsof -i :3010
sudo lsof -i :3011

# Windows
netstat -ano | findstr :3010
netstat -ano | findstr :3011
```

Kill the process or change ports in `docker-compose.yml`.

### Build Failures

Clear Docker cache and rebuild:
```bash
docker-compose down
docker system prune -a
docker-compose up -d --build
```

### Configuration Not Loading

Ensure configuration files exist:
```bash
ls -la data/boards.toml
```

If missing, copy from example:
```bash
cp data/boards.toml.example data/boards.toml
```

Restart after editing:
```bash
docker-compose restart
```

### Connection to WLED Boards Failing

1. **Verify network connectivity** from container:
```bash
docker exec -it wled-server ping [wled-board-ip]
```

2. **Check board configuration**:
```bash
cat data/boards.toml
```

3. **Check container logs** for connection errors:
```bash
docker-compose logs -f backend
```

## Updating

To update to a new version:

1. **Pull latest code** (if using git):
```bash
git pull
```

2. **Rebuild and restart**:
```bash
docker-compose down
docker-compose up -d --build
```

For remote deployments, run your deployment script to sync and rebuild.

## Performance

Typical resource usage:
- **RAM**: 15-35MB total (Rust backend ~10-30MB, lighttpd ~5MB)
- **CPU**: <5% idle, 10-30% during active operations
- **Disk**: ~50MB base install + user data

## Security Considerations

This setup is designed for local network use. For public internet access, consider:

1. **Reverse Proxy**: Use nginx or traefik for SSL termination
2. **HTTPS**: Add SSL/TLS certificates
3. **Authentication**: Implement an auth layer
4. **Firewall**: Restrict access to trusted networks
5. **Updates**: Keep Docker and system packages updated

Current CORS policy allows all origins (suitable for local network only).

## Support

For issues:
1. Check container logs: `docker-compose logs -f`
2. Verify network connectivity to WLED devices
3. Review configuration files in `data/` directory
4. Check Docker daemon status
5. Create an issue in the repository
