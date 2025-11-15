# OpenFAN Controller Dockerfile
# Multi-stage build for optimized container size

# Build stage
FROM rust:1.91-alpine AS builder

# Use Docker's automatic platform detection
ARG TARGETPLATFORM
ARG VERSION=0.1.0

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    libc6-compat \
    build-base \
    pkgconfig \
    openssl-dev

# Set working directory
WORKDIR /usr/src/openfan

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY openfan-core/ ./openfan-core/
COPY openfan-hardware/ ./openfan-hardware/
COPY openfand/ ./openfand/
COPY openfanctl/ ./openfanctl/

# Determine Rust target based on Docker platform
RUN case "$TARGETPLATFORM" in \
        "linux/amd64") echo "x86_64-unknown-linux-musl" > /tmp/rust-target ;; \
        "linux/arm64") echo "aarch64-unknown-linux-musl" > /tmp/rust-target ;; \
        *) echo "Unsupported platform: $TARGETPLATFORM" && exit 1 ;; \
    esac

# Add musl target for static linking
RUN rustup target add $(cat /tmp/rust-target)

# Build release binaries
RUN cargo build --release --target $(cat /tmp/rust-target)

# Runtime stage
FROM alpine:3.22

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

# Copy binaries from builder (use wildcard to match both x86_64 and aarch64)
COPY --from=builder /usr/src/openfan/target/*/release/openfand /opt/openfan/bin/
COPY --from=builder /usr/src/openfan/target/*/release/openfanctl /usr/local/bin/

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
ARG VERSION
LABEL maintainer="OpenFAN Contributors" \
    description="OpenFAN Controller - Fan Management System" \
    version="${VERSION}" \
    org.opencontainers.image.title="OpenFAN Controller" \
    org.opencontainers.image.description="High-performance fan management system" \
    org.opencontainers.image.vendor="OpenFAN Project" \
    org.opencontainers.image.version="${VERSION}" \
    org.opencontainers.image.source="https://github.com/graelo/openfan-rs" \
    org.opencontainers.image.documentation="https://github.com/graelo/openfan-rs/blob/main/README.md"
