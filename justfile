# Justfile for restic-api-server-123pan
# Load environment variables from .env file
set dotenv-load

# Default recipe: show available commands
default:
    @just --list

# Build debug version
build:
    cargo build

# Run all tests (single-threaded to avoid rate limiting)
test:
    cargo test -- --test-threads=1

# Run all tests with output
test-verbose:
    cargo test -- --test-threads=1 --nocapture

# Run unit tests only (no integration/e2e)
test-unit:
    cargo test --lib

# Run integration tests
test-integration:
    cargo test --test integration_test -- --test-threads=1 --nocapture

# Run cache consistency tests
test-cache:
    cargo test --test integration_test cache_scenario -- --test-threads=1 --nocapture

# Run basic e2e backup/restore test
test-e2e:
    cargo test --test e2e_test -- --test-threads=1 --nocapture

# Run 100MB large scale test (takes several minutes, shows real-time logs)
test-e2e-100mb:
    cargo test --test e2e_test test_e2e_large_scale_100mb -- --nocapture

# Run a quick sanity check (build + unit tests + lint)
check: build test-unit lint
    @echo "All checks passed!"

# Run Docker container in detached mode
up:
    docker-compose up -d

# Stop Docker container
down:
    docker-compose down
