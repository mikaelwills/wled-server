# Multi-stage Dockerfile for WLED Rust Server
# Builds both Rust backend and SvelteKit frontend

# Stage 1: Build Rust backend
FROM rust:1.75 as rust-builder

WORKDIR /app

# Copy Cargo files for dependency caching
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build release binary
RUN cargo build --release

# Stage 2: Build SvelteKit frontend
FROM oven/bun:1 as frontend-builder

WORKDIR /app/frontend

# Copy package files
COPY frontend/package.json frontend/bun.lockb ./

# Install dependencies
RUN bun install --frozen-lockfile

# Copy frontend source
COPY frontend/ ./

# Build frontend for production
RUN bun run build

# Stage 3: Runtime image
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

# Install Bun for running frontend
RUN curl -fsSL https://bun.sh/install | bash
ENV PATH="/root/.bun/bin:${PATH}"

# Copy Rust backend binary
COPY --from=rust-builder /app/target/release/rust-wled-server /app/backend

# Copy built frontend
COPY --from=frontend-builder /app/frontend/build /app/frontend/build
COPY --from=frontend-builder /app/frontend/package.json /app/frontend/
COPY --from=frontend-builder /app/frontend/node_modules /app/frontend/node_modules

# Copy startup script
COPY docker-start.sh /app/

RUN chmod +x /app/docker-start.sh

# Expose ports
EXPOSE 3000 3001

# Start both services
CMD ["/app/docker-start.sh"]
