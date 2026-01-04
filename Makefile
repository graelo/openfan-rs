# OpenFAN Controller Makefile
# Provides convenient commands for building, testing, and Docker operations

# Extract version from Cargo.toml
VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

# Docker image name
DOCKER_IMAGE := openfan

.PHONY: help build test clean docker docker-build docker-push

help:
	@echo "OpenFAN Controller v$(VERSION)"
	@echo ""
	@echo "Usage: make <target>"
	@echo ""
	@echo "Targets:"
	@echo "  build        Build release binaries"
	@echo "  test         Run all tests"
	@echo "  clean        Clean build artifacts"
	@echo "  docker       Build Docker image with version from Cargo.toml"
	@echo "  docker-push  Push Docker image to registry"
	@echo ""
	@echo "Current version: $(VERSION)"

build:
	cargo build --release

test:
	cargo test --all

clean:
	cargo clean

# Build Docker image with version extracted from Cargo.toml
docker:
	docker build --build-arg VERSION=$(VERSION) -t $(DOCKER_IMAGE):$(VERSION) -t $(DOCKER_IMAGE):latest .

# Build for multiple platforms
docker-multiplatform:
	docker buildx build --platform linux/amd64,linux/arm64 \
		--build-arg VERSION=$(VERSION) \
		-t $(DOCKER_IMAGE):$(VERSION) \
		-t $(DOCKER_IMAGE):latest .

# Push to registry (assumes docker login already done)
docker-push:
	docker push $(DOCKER_IMAGE):$(VERSION)
	docker push $(DOCKER_IMAGE):latest
