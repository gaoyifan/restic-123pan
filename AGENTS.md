# AGENTS.md

This file provides guidance for AI agents working with the restic-123pan codebase.

## Project Overview

This is a Rust-based REST API server that implements the Restic backup tool's REST backend protocol (v2 only), using 123pan cloud storage as the storage provider. It acts as a bridge between Restic clients and the 123pan Open Platform API.

## Architecture

```
restic CLI  <--REST API-->  This Server  <--HTTPS-->  123pan Open Platform
```

The server receives standard Restic REST API v2 requests and translates them into 123pan API calls.

## Project Structure

```
src/
├── main.rs           # Entry point, CLI parsing, Axum server setup
├── config.rs         # Configuration via clap (CLI args + env vars)
├── error.rs          # Error types with HTTP response mapping
├── pan123/           # 123pan API client module
│   ├── mod.rs        # Module exports
│   ├── auth.rs       # Token management with auto-refresh
│   ├── client.rs     # HTTP client for all 123pan operations
│   └── types.rs      # Request/response types for 123pan API
└── restic/           # Restic REST API handlers
    ├── mod.rs        # Module exports
    ├── handler.rs    # Axum route handlers for all endpoints
    └── types.rs      # Restic API types (v2 only)

tests/
├── integration_test.rs  # Tests 123pan API directly
└── e2e_test.rs          # Full backup/restore with restic CLI

docs/
└── technical.md      # Detailed implementation documentation (Chinese)
```

## Key Components

### `pan123/auth.rs` - Token Manager
- Manages OAuth access tokens from 123pan
- Auto-refreshes tokens before expiry (5-minute buffer)
- Thread-safe with `parking_lot::RwLock`

### `pan123/client.rs` - 123pan Client
- Core API operations: list, upload, download, delete
- Directory ID caching for performance
- Dynamic upload domain fetching at startup
- Uses `duplicate=2` for atomic file overwrites
- Native Range download support

### `restic/handler.rs` - REST Handlers
- Implements Restic REST API v2 endpoints only
- Maps Restic file types to 123pan directory paths
- Idempotent delete operations (returns 200 even if file doesn't exist)

## Building

```bash
# Development build
cargo build

# Release build
cargo build --release

# Run with required env vars
PAN123_CLIENT_ID=xxx PAN123_CLIENT_SECRET=xxx cargo run
```

## Testing

```bash
# Run all tests with single thread (avoids rate limiting)
PAN123_CLIENT_ID=xxx PAN123_CLIENT_SECRET=xxx cargo test -- --test-threads=1

# E2E tests only
cargo test --test e2e_test

# Integration tests only
cargo test --test integration_test
```

## Dependencies

Key crates used:
- **axum** - Web framework for REST API
- **reqwest** - HTTP client for 123pan API calls
- **tokio** - Async runtime
- **serde/serde_json** - JSON serialization
- **clap** - CLI argument parsing with env var support
- **tracing** - Structured logging
- **parking_lot** - Fast synchronization primitives
- **md5** - Hash calculation for uploads

## 123pan API Endpoints Used

| 123pan API | Purpose |
|------------|---------|
| `POST /api/v1/access_token` | Get OAuth token |
| `GET /upload/v2/file/domain` | Get upload domain (cached) |
| `POST /upload/v1/file/mkdir` | Create directory |
| `GET /api/v2/file/list` | List directory contents |
| `GET /api/v1/file/download_info` | Get download URL |
| `POST {upload_domain}/upload/v2/file/single/create` | Upload files (with duplicate=2) |
| `POST /api/v1/file/trash` | Move to trash |
| `POST /api/v1/file/delete` | Permanently delete |

## Restic REST API v2 Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/?create=true` | Initialize repository |
| DELETE | `/` | Delete repository (not implemented) |
| HEAD/GET/POST | `/config` | Config file operations |
| GET | `/:type/` | List files by type (v2 format) |
| HEAD/GET/POST/DELETE | `/:type/:name` | Individual file operations |

File types: `data`, `keys`, `locks`, `snapshots`, `index`

## Coding Conventions

1. **Error Handling**: Use `AppError` enum, convert to appropriate HTTP status codes
2. **Async**: All I/O operations are async using tokio
3. **Logging**: Use `tracing` macros (`tracing::info!`, `tracing::debug!`, etc.)
4. **Path Parameters**: Axum uses `:param` syntax (e.g., `/:type/:name`)
5. **JSON**: Use serde with `#[serde(rename_all = "camelCase")]` for 123pan API types

## Common Tasks

### Adding a new 123pan API call
1. Add request/response types to `pan123/types.rs`
2. Implement the method in `pan123/client.rs`
3. Use `self.get()` or `self.post()` helper methods for authenticated requests

### Adding a new Restic endpoint
1. Add the handler function in `restic/handler.rs`
2. Register the route in `create_router()`
3. Update tests as needed

### Debugging API issues
- Set `RUST_LOG=debug` to see detailed request/response logs
- Check 123pan API response codes in error messages
- Verify token is valid (auto-refresh should handle this)

## Known Limitations

1. Single-step upload limited to 1GB files (123pan API limitation)
2. Delete repository not implemented
3. No multipart upload support for large files

## Design Decisions

### Directory Listing vs Search API
123pan's `searchMode=1` has index delays for newly created files, making it unreliable for Restic's workflow. We use full directory listing (`list_files`) instead.

### Idempotent Delete
Delete operations return HTTP 200 even if the file doesn't exist, as Restic may attempt to delete non-existent lock files.

### Atomic Overwrites
Using `duplicate=2` in uploads allows atomic file replacement without needing to delete first.

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `PAN123_CLIENT_ID` | 123pan client ID | (required) |
| `PAN123_CLIENT_SECRET` | 123pan client secret | (required) |
| `PAN123_REPO_PATH` | Root path on 123pan | `/restic-backup` |
| `LISTEN_ADDR` | Server bind address | `127.0.0.1:8000` |
| `RUST_LOG` | Log level | `info` |

## Useful Commands

```bash
# Check for lint issues
cargo clippy

# Format code
cargo fmt

# Run with debug logging
RUST_LOG=debug cargo run

# Test with actual restic
restic -r rest:http://127.0.0.1:8000/ init
restic -r rest:http://127.0.0.1:8000/ backup /path/to/files
restic -r rest:http://127.0.0.1:8000/ snapshots
```
