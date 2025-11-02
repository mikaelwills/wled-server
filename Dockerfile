# Runtime-only Dockerfile for WLED Rust Server
# Assumes binary is pre-built locally with cross-compilation for Linux

FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates python3 && \
    rm -rf /var/lib/apt/lists/*

# Copy pre-built Rust backend binary (cross-compiled for Linux x86_64)
COPY target/x86_64-unknown-linux-gnu/release/rust-wled-server /app/backend

# Copy pre-built frontend (built locally with Bun)
COPY frontend/build /app/frontend/build
COPY frontend/package.json /app/frontend/

# Copy startup script
COPY docker-start.sh /app/

RUN chmod +x /app/docker-start.sh /app/backend

# Expose ports
EXPOSE 3010 3001

# Start both services
CMD ["/app/docker-start.sh"]
