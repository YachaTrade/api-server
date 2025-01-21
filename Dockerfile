# Builder stage
FROM rust:latest as builder

WORKDIR /app/api-server

# Copy only files needed for dependency resolution first
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Now copy the real source code
COPY . .

# Install sqlx-cli for database preparation
RUN cargo install sqlx-cli --no-default-features --features native-tls,postgres

# Build with offline mode
ENV SQLX_OFFLINE=true
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app/api-server

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libpq5 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create directories
RUN mkdir -p /run/api-server

# Copy the binary and necessary files
COPY --from=builder /app/api-server/target/release/api-server /run/api-server/


# Set environment variables
ENV RUST_LOG=info

CMD ["/run/api-server/api-server"]