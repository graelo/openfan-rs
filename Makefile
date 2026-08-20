# Local task runner for build, test, and pre-push / pre-PR verification.
#
# Usage:
#   make check       # fmt + lint + test  (run before `git push`)
#   make check-all   # adds audits, commit lint, docs, and CI security checks
#   make fix         # auto-format and apply clippy fixes
#
# The check targets intentionally mirror the CI quality and test commands. They
# assume their external tools (cargo-nextest, cargo-deny, cargo-pants, convco,
# poutine, zizmor, rumdl, mandoc, and cargo-llvm-cov) are installed locally.

.DEFAULT_GOAL := help

VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
DOCKER_IMAGE := openfan

.PHONY: help build fmt lint test check audit commits ci-security md man check-all \
	fix release coverage clean docker docker-multiplatform docker-push

help:  ## List available targets
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_-]+:.*## / \
		{printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build:  ## Build all workspace binaries in release mode
	cargo build --locked --workspace --release

fmt:  ## rustfmt --check (no changes)
	cargo fmt --all -- --check

lint:  ## clippy with warnings denied
	cargo clippy --locked --workspace --all-targets --all-features -- -D warnings

test:  ## full test suite (build + nextest + doc tests)
	./ci/test_full.sh

check: fmt lint test  ## pre-push gate: fmt + lint + test

audit:  ## cargo-deny & cargo-pants: advisories, licenses, bans, sources
	cargo deny --locked check
	cargo pants

commits:  ## verify commit messages follow Conventional Commits
	convco check -c .convco

ci-security:  ## audit GitHub Actions workflows
	poutine --fail-on-violation analyze_local .
	zizmor .github

md:  ## lint Markdown against rumdl.toml
	rumdl check .

man:  ## lint the roff manpages
	mandoc -Tlint man/openfand.1
	mandoc -Tlint man/openfanctl.1

check-all: check audit commits ci-security md man  ## pre-PR gate: everything

fix:  ## auto-fix: rustfmt + clippy --fix
	cargo fmt --all
	cargo clippy --locked --workspace --all-targets --all-features --fix --allow-dirty --allow-staged -- -D warnings

release:  ## release build with native CPU optimizations
	RUSTFLAGS="-Ctarget-cpu=native" cargo build --locked --workspace --release

coverage:  ## HTML coverage report at target/llvm-cov/html/index.html
	cargo llvm-cov --locked --workspace --html

clean:  ## Remove build artifacts
	cargo clean

docker:  ## Build Docker image tagged with the Cargo version and latest
	docker build --build-arg VERSION=$(VERSION) -t $(DOCKER_IMAGE):$(VERSION) -t $(DOCKER_IMAGE):latest .

docker-multiplatform:  ## Build Docker image for amd64 and arm64
	docker buildx build --platform linux/amd64,linux/arm64 \
		--build-arg VERSION=$(VERSION) \
		-t $(DOCKER_IMAGE):$(VERSION) \
		-t $(DOCKER_IMAGE):latest .

docker-push:  ## Push versioned and latest Docker images
	docker push $(DOCKER_IMAGE):$(VERSION)
	docker push $(DOCKER_IMAGE):latest
