# AGENTS.md

Guidance for AI agents working with the restic-123pan codebase.

## Project Overview

Rust REST API server implementing Restic backup tool's REST backend protocol (v2 only), using 123pan cloud storage as the storage provider.

```
restic CLI  <--REST API-->  This Server  <--HTTPS-->  123pan Open Platform
```

## Build, Test Commands

```bash
# Build
just build                           # Development build

# Run all tests (single-threaded to avoid rate limiting)
just test

# Run all tests with output
just test-verbose

# Run unit tests only (no integration/e2e)
just test-unit

# Run integration tests
just test-integration

# Run cache consistency tests
just test-cache

# Run basic e2e backup/restore test
just test-e2e

# Run 100MB large scale test (takes several minutes)
just test-e2e-100mb

# Run a quick sanity check (build + unit tests)
just check
```

## Project Structure

```
src/
├── main.rs           # Entry point, CLI parsing, Axum server setup
├── lib.rs            # Library exports
├── config.rs         # Configuration via clap (CLI args + env vars)
├── error.rs          # Error types with HTTP response mapping
├── pan123/           # 123pan API client module
│   ├── auth.rs       # Token management with auto-refresh
│   ├── client.rs     # HTTP client for all 123pan operations
│   ├── entity.rs     # SeaORM entity for SQLite cache
│   └── types.rs      # Request/response types for 123pan API
└── restic/           # Restic REST API handlers
    ├── handler.rs    # Axum route handlers
    └── types.rs      # Restic API types (v2 only)

tests/
├── integration_test.rs  # Tests 123pan API directly
└── e2e_test.rs          # Full backup/restore with restic CLI
```

## Code Style Guidelines

### Imports

Order: std → external crates → crate-local modules. Use nested imports:

```rust
use axum::{body::Body, extract::State, http::StatusCode};
use std::sync::Arc;
use crate::error::{AppError, Result};
```

### Error Handling

- Use `AppError` enum in `error.rs` for all errors
- Define `pub type Result<T> = std::result::Result<T, AppError>;`
- Use `?` operator for propagation; use `thiserror` for error derives
- Map errors to HTTP status codes in `IntoResponse` impl

### Logging

Use `tracing` macros: `tracing::info!`, `tracing::debug!`, `tracing::warn!`, `tracing::error!`

### Types & Serialization

- Use `#[serde(rename_all = "camelCase")]` for 123pan API types
- Use explicit `#[serde(rename = "...")]` for non-standard field names
- Use `i64` for file IDs and sizes (123pan API uses large integers)

### Testing

- Use `skip_if_no_credentials!()` macro for tests requiring 123pan API
- Use `--test-threads=1` to avoid rate limiting
- Use `tempfile` for temporary directories; clean up resources in teardown

### Git

- Conventional commit message: `type: description` (lowercase, no period). Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`
- Never commit (`git commit`) without the user's explicit approval of changes

## Key Design Patterns

### Retry with Backoff

Use `retry_api!` macro for 429 (rate limit) and 401 (token expired) handling.

### Idempotent Operations

- DELETE returns 200 even if file doesn't exist
- Upload with `duplicate=2` for atomic overwrites

### Caching

- SQLite-backed persistent cache for file listings
- Cache updated synchronously on upload/delete operations
- Use `warm_cache()` at startup to pre-populate

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `PAN123_CLIENT_ID` | Yes | - | 123pan client ID |
| `PAN123_CLIENT_SECRET` | Yes | - | 123pan client secret |
| `PAN123_REPO_PATH` | No | `/restic-backup` | Root path on 123pan |
| `LISTEN_ADDR` | No | `127.0.0.1:8000` | Server bind address |
| `RUST_LOG` | No | `info` | Log level |
| `DB_PATH` | No | `cache-123pan.db` | SQLite cache file |
| `FORCE_CACHE_REBUILD` | No | `false` | Rebuild cache on startup |

## Common Tasks

### Adding a new 123pan API call

1. Add request/response types to `pan123/types.rs`
2. Implement method in `pan123/client.rs` using `self.get()` or `self.post()`
3. Add tests to `tests/integration_test.rs`

### Adding a new Restic endpoint

1. Add handler in `restic/handler.rs`
2. Register route in `create_router()`
3. Add tests as needed
