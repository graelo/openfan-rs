# OpenFAN Controller Dockerfile
# Multi-stage build for optimized container size

# Build stage
FROM rust:1.75-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    libc6-compat \
    build-base

# Set working directory
WORKDIR /usr/src/openfan

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY openfan-core/ ./openfan-core/
COPY openfand/ ./openfand/
COPY openfanctl/ ./openfanctl/

# Build release binaries
RUN cargo build --release --target x86_64-unknown-linux-musl

# Runtime stage
FROM alpine:3.18

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    tzdata

# Create non-root user
RUN addgroup -g 1000 openfan && \
    adduser -D -s /bin/sh -u 1000 -G openfan openfan

# Create directories
RUN mkdir -p /opt/openfan/bin \
    /etc/openfan \
    /var/lib/openfan \
    /var/log/openfan && \
    chown -R openfan:openfan /opt/openfan \
    /etc/openfan \
    /var/lib/openfan \
    /var/log/openfan

# Copy binaries from builder
COPY --from=builder /usr/src/openfan/target/x86_64-unknown-linux-musl/release/openfand /opt/openfan/bin/
COPY --from=builder /usr/src/openfan/target/x86_64-unknown-linux-musl/release/openfanctl /usr/local/bin/

# Copy configuration
COPY config.yaml /etc/openfan/

# Set permissions
RUN chmod 755 /opt/openfan/bin/openfand \
    /usr/local/bin/openfanctl && \
    chmod 640 /etc/openfan/config.yaml && \
    chown openfan:openfan /etc/openfan/config.yaml

# Switch to non-root user
USER openfan

# Set working directory
WORKDIR /var/lib/openfan

# Expose port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD /usr/local/bin/openfanctl --server http://localhost:8080 health || exit 1

# Set environment variables
ENV RUST_LOG=info
ENV OPENFAN_CONFIG=/etc/openfan/config.yaml

# Default command
CMD ["/opt/openfan/bin/openfand", "--config", "/etc/openfan/config.yaml", "--mock"]

# Labels
LABEL maintainer="OpenFAN Contributors <maintainer@example.com>" \
    description="OpenFAN Controller - Fan Management System" \
    version="1.0.0" \
    org.opencontainers.image.title="OpenFAN Controller" \
    org.opencontainers.image.description="High-performance fan management system" \
    org.opencontainers.image.vendor="OpenFAN Project" \
    org.opencontainers.image.source="https://github.com/graelo/OpenFanController" \
    org.opencontainers.image.documentation="https://github.com/graelo/OpenFanController/blob/main/Software/openfan/README.md"
