# Builder stage
FROM rust:latest as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create a new empty shell project
WORKDIR /app/api-server
COPY . .

# Build with release profile
RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy the built binary from builder
COPY --from=builder /app/api-server/target/release/api-server /app/api-server/
COPY --from=builder /app/api-server/migrations /app/api-server/migrations
# Create a non-root user
RUN useradd -m -u 1000 -U app
USER app

# Set environment variables
ENV RUST_LOG=info

# Expose the port your application listens on
EXPOSE 8000

# Run the binary
CMD ["./api-server"]
