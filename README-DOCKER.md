# WLED Server - Docker Deployment Guide

This guide covers deploying the WLED Server in Docker on a Synology NAS or remote machine.

## Architecture

- **Single Container** running both services:
  - Rust backend on port 3010
  - SvelteKit frontend on port 3011
- **Volume Mount**: `boards.toml` persists across container restarts
- **Auto-restart**: Container automatically restarts on crashes or reboots

## Prerequisites

### On Synology NAS (192.168.1.161)
- Docker package installed (via Package Center)
- SSH access enabled
- User: `mikael`
- Deploy path: `/volume1/docker/wled-server`

### On Development Laptop
- SSH access to NAS configured
- rsync installed (comes with macOS/Linux)

## Deployment to Synology NAS

### Initial Setup (First Time)

**1. Deploy to NAS:**
```bash
./deploy-to-nas.sh
```

This will:
- Create `/volume1/docker/wled-server` on the NAS
- Sync all project files via rsync
- Build Docker images
- Start containers

**2. Create/Edit boards.toml on NAS:**
```bash
ssh mikael@192.168.1.161
cd /volume1/docker/wled-server
nano boards.toml
```

Add your WLED boards:
```toml
[[boards]]
id = "bedroom"
ip = "192.168.1.172"
```

**3. Restart if needed:**
```bash
docker-compose restart
```

## Development Workflow

### On Laptop (Development)

1. **Make changes** to code
2. **Test locally**:
   ```bash
   ./restart.sh
   ```
3. **Commit to git** (optional, for version control):
   ```bash
   git add .
   git commit -m "Your commit message"
   git push
   ```
4. **Deploy to NAS**:
   ```bash
   ./deploy-to-nas.sh
   ```

The deploy script automatically:
- Syncs all code changes to NAS via rsync
- Stops existing containers
- Rebuilds Docker images
- Starts new containers
- Shows status

**That's it!** One command deployment.

## Common Commands (on NAS)

SSH to NAS first:
```bash
ssh mikael@192.168.1.161
cd /volume1/docker/wled-server
```

### View Logs
```bash
# All logs
docker-compose logs -f

# Just backend
docker logs wled-server | grep backend

# Just frontend
docker logs wled-server | grep frontend
```

### Check Status
```bash
docker-compose ps
```

### Restart Container
```bash
docker-compose restart
```

### Stop Container
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

### Edit boards.toml
```bash
nano boards.toml
docker-compose restart
```

## Accessing the Application

Once deployed, access the application at:

- **Frontend**: `http://192.168.1.161:3011`
- **Backend API**: `http://192.168.1.161:3010`

The frontend automatically connects to the backend using the browser's hostname, so no configuration is needed!

## Configuration

### Editing boards.toml

Since `boards.toml` is mounted as a volume, you can edit it directly on the host:

```bash
# On remote machine
cd rust-wled-server
nano boards.toml

# Restart to apply changes
docker-compose restart
```

### Environment Variables

Edit `docker-compose.yml` to add environment variables:

```yaml
environment:
  - RUST_LOG=debug  # Change log level
  - YOUR_VAR=value
```

## Troubleshooting

### Container Won't Start

Check logs:
```bash
docker-compose logs
```

### Port Already in Use

Check what's using the port:
```bash
sudo lsof -i :3010
sudo lsof -i :3011
```

Kill the process or change ports in `docker-compose.yml`.

### Build Failures

Clear Docker cache and rebuild:
```bash
docker-compose down
docker system prune -a
docker-compose up -d --build
```

### boards.toml Not Found

Ensure `boards.toml` exists in the project root on the host machine. The container needs it mounted.

## Updating

To update to the latest code:

```bash
# On laptop
git push

# On remote machine
./deploy.sh
```

That's it! The deploy script handles everything else.

## Network Access

The application will be accessible from:
- **Local network**: `http://192.168.1.161:3011`
- **Same machine**: `http://localhost:3011`
- **Mobile devices**: Access via the IP address

All CORS is pre-configured to work from any origin.

## Security Notes

- This setup is designed for local network use
- For public internet access, add:
  - Reverse proxy (nginx/traefik)
  - HTTPS/SSL certificates
  - Authentication layer
- Current CORS policy allows all origins (suitable for local network)
