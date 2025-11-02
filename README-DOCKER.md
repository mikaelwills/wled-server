# WLED Server - Docker Deployment Guide

This guide covers deploying the WLED Server in Docker on a remote machine.

## Architecture

- **Single Container** running both services:
  - Rust backend on port 3000
  - SvelteKit frontend on port 3001
- **Volume Mount**: `boards.toml` persists across container restarts
- **Auto-restart**: Container automatically restarts on crashes or reboots

## Prerequisites

### On Remote Machine (192.168.1.161)
- Docker installed
- Docker Compose installed
- Git installed
- SSH access configured

### On Development Laptop
- Git repository set up with remote

## Initial Setup

### 1. Set Up Git Remote

**Option A: Using GitHub/GitLab**
```bash
# Create a new repository on GitHub/GitLab, then:
git remote add origin https://github.com/yourusername/rust-wled-server.git
git branch -M main
git push -u origin main
```

**Option B: Using Remote Machine as Git Server**
```bash
# On remote machine (.161)
mkdir -p ~/git/wled-server.git
cd ~/git/wled-server.git
git init --bare

# On laptop
git remote add origin user@192.168.1.161:~/git/wled-server.git
git push -u origin main
```

### 2. Clone on Remote Machine

SSH to remote machine:
```bash
ssh user@192.168.1.161
```

Clone the repository:
```bash
git clone <your-git-url> rust-wled-server
cd rust-wled-server
```

### 3. Create boards.toml

Create a `boards.toml` file if it doesn't exist:
```bash
cat > boards.toml << 'EOF'
[[boards]]
id = "bedroom"
ip = "192.168.1.172"
EOF
```

### 4. Build and Run

```bash
docker-compose up -d --build
```

## Development Workflow

### On Laptop (Development)

1. **Make changes** to code
2. **Test locally**:
   ```bash
   ./restart.sh
   ```
3. **Commit changes**:
   ```bash
   git add .
   git commit -m "Your commit message"
   ```
4. **Push to remote**:
   ```bash
   git push
   ```

### On Remote Machine (.161)

1. **SSH to machine**:
   ```bash
   ssh user@192.168.1.161
   cd rust-wled-server
   ```

2. **Deploy updates**:
   ```bash
   ./deploy.sh
   ```

   This script automatically:
   - Pulls latest code from git
   - Stops existing containers
   - Rebuilds images
   - Starts new containers

## Common Commands

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

## Accessing the Application

Once deployed, access the application at:

- **Frontend**: `http://192.168.1.161:3001`
- **Backend API**: `http://192.168.1.161:3000`

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
sudo lsof -i :3000
sudo lsof -i :3001
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
- **Local network**: `http://192.168.1.161:3001`
- **Same machine**: `http://localhost:3001`
- **Mobile devices**: Access via the IP address

All CORS is pre-configured to work from any origin.

## Security Notes

- This setup is designed for local network use
- For public internet access, add:
  - Reverse proxy (nginx/traefik)
  - HTTPS/SSL certificates
  - Authentication layer
- Current CORS policy allows all origins (suitable for local network)
