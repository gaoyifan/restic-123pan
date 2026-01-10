# Justfile for restic-api-server-123pan
# Load environment variables from .env file
set dotenv-load

# Default recipe: show available commands
default:
    @just --list

# ============================================================================
# Build
# ============================================================================

# Build debug version
build:
    cargo build

# Build release version
build-release:
    cargo build --release

# Run clippy linter
lint:
    cargo clippy

# Format code
fmt:
    cargo fmt

# ============================================================================
# Run
# ============================================================================

# Run the server (debug mode)
run:
    cargo run

# Run the server with debug logging
run-debug:
    RUST_LOG=debug cargo run

# Run the server (release mode)
run-release:
    cargo run --release

# ============================================================================
# Tests
# ============================================================================

# Run all tests (single-threaded to avoid rate limiting)
test:
    cargo test -- --test-threads=1

# Run all tests with output
test-verbose:
    cargo test -- --test-threads=1 --nocapture

# Run unit tests only (no integration/e2e)
test-unit:
    cargo test --lib

# ============================================================================
# Integration Tests (require PAN123_CLIENT_ID and PAN123_CLIENT_SECRET)
# ============================================================================

# Run integration tests
test-integration:
    cargo test --test integration_test -- --test-threads=1 --nocapture

# Run cache consistency tests
test-cache:
    cargo test --test integration_test cache_scenario -- --test-threads=1 --nocapture

# ============================================================================
# E2E Tests (require credentials + restic CLI installed)
# ============================================================================

# Run basic e2e backup/restore test
test-e2e:
    cargo test --test e2e_test test_e2e_backup_and_restore -- --nocapture

# Run all e2e tests (basic + large scale)
test-e2e-all:
    cargo test --test e2e_test -- --test-threads=1 --nocapture

# Run server startup test only
test-e2e-startup:
    cargo test --test e2e_test test_server_startup -- --nocapture

# Run 100MB large scale test (takes several minutes, shows real-time logs)
test-e2e-100mb:
    cargo test --test e2e_test test_e2e_large_scale_100mb -- --nocapture

# Run 100MB test with debug server logs
test-e2e-100mb-debug:
    RUST_LOG=debug cargo test --test e2e_test test_e2e_large_scale_100mb -- --nocapture

# Run incremental backup test
test-e2e-incremental:
    cargo test --test e2e_test test_e2e_incremental_backup -- --nocapture

# Run e2e test with debug server logs (shows API server logs in real-time)
test-e2e-debug:
    RUST_LOG=debug cargo test --test e2e_test -- --test-threads=1 --nocapture

# ============================================================================
# Utilities
# ============================================================================

# Check if credentials are set
check-env:
    @echo "PAN123_CLIENT_ID: $(if [ -n \"$PAN123_CLIENT_ID\" ]; then echo 'SET'; else echo 'NOT SET'; fi)"
    @echo "PAN123_CLIENT_SECRET: $(if [ -n \"$PAN123_CLIENT_SECRET\" ]; then echo 'SET'; else echo 'NOT SET'; fi)"
    @which restic > /dev/null && echo "restic: $(restic version)" || echo "restic: NOT INSTALLED"

# Clean build artifacts
clean:
    cargo clean

# Watch for changes and rebuild
watch:
    cargo watch -x build

# Run a quick sanity check (build + unit tests + lint)
check: build test-unit lint
    @echo "All checks passed!"
