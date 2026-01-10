# Restic REST API Server for 123pan

A Restic REST API backend server that uses 123pan cloud storage as the storage provider.

## Features

- Full Restic REST API v2 support
- Automatic directory creation via 123pan's containDir feature
- Token auto-refresh for seamless authentication
- Supports backup, restore, snapshots, and all restic operations

## Requirements

- Rust 1.70+
- 123pan open platform credentials (client_id and client_secret)
- restic CLI (for testing)

## Installation

```bash
cargo build --release
```

## Usage

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `PAN123_CLIENT_ID` | 123pan Open Platform client ID | (required) |
| `PAN123_CLIENT_SECRET` | 123pan Open Platform client secret | (required) |
| `PAN123_REPO_PATH` | Root folder path on 123pan | `/restic-backup` |
| `LISTEN_ADDR` | Server listen address | `127.0.0.1:8000` |
| `RUST_LOG` | Log level (trace, debug, info, warn, error) | `info` |

### Running the Server

```bash
# Using environment variables
export PAN123_CLIENT_ID=your_client_id
export PAN123_CLIENT_SECRET=your_client_secret
cargo run --release

# Or using command line arguments
cargo run --release -- \
  --client-id your_client_id \
  --client-secret your_client_secret \
  --repo-path /my-restic-backup \
  --listen-addr 0.0.0.0:8000
```

### Using with Restic

```bash
# Initialize a new repository
export RESTIC_PASSWORD="your-secure-password"
restic -r rest:http://127.0.0.1:8000/ init

# Backup files
restic -r rest:http://127.0.0.1:8000/ backup /path/to/files

# List snapshots
restic -r rest:http://127.0.0.1:8000/ snapshots

# Restore files
restic -r rest:http://127.0.0.1:8000/ restore latest --target /path/to/restore
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/?create=true` | Initialize repository |
| DELETE | `/` | Delete repository (not implemented) |
| HEAD | `/config` | Check if config exists |
| GET | `/config` | Get config file |
| POST | `/config` | Save config file |
| GET | `/:type/` | List files of type (data, keys, locks, snapshots, index) |
| HEAD | `/:type/:name` | Check if file exists |
| GET | `/:type/:name` | Download file |
| POST | `/:type/:name` | Upload file |
| DELETE | `/:type/:name` | Delete file |

## Testing

```bash
# Set credentials
export PAN123_CLIENT_ID=your_client_id
export PAN123_CLIENT_SECRET=your_client_secret

# Run all tests
cargo test

# Run integration tests only
cargo test --test integration_test

# Run e2e tests (requires restic CLI)
cargo test --test e2e_test
```

## Project Structure

```
src/
├── main.rs           # Entry point, CLI parsing, server setup
├── config.rs         # Configuration handling
├── error.rs          # Error types
├── pan123/
│   ├── mod.rs        # Module exports
│   ├── client.rs     # 123pan HTTP client
│   ├── auth.rs       # Token management with auto-refresh
│   └── types.rs      # 123pan API request/response types
└── restic/
    ├── mod.rs        # Module exports
    ├── handler.rs    # Axum route handlers
    └── types.rs      # Restic REST API types

tests/
├── integration_test.rs  # Integration tests with 123pan API
└── e2e_test.rs          # End-to-end tests with restic CLI
```

## License

MIT
