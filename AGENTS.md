# AGENTS.md - Coding Agent Guidelines for treebeard

## Project Overview

**treebeard** is a Rust CLI tool that creates isolated Git worktree environments with copy-on-write semantics for ignored files. It uses FUSE (macFUSE) to provide an overlay filesystem on macOS 15+.

## Build Commands

```bash
cargo build                        # Debug build
cargo build --release              # Release build
cargo install --path .             # Install from source
cargo run -- branch <branch-name>  # Run directly
```

## Test Commands

For tests to run, macFUSE must be installed.

Tests use `cargo-nextest` for process-per-test isolation, which is required because FUSE-based tests have resource contention issues when run in the same process. All tests run serially (one at a time) via the `.config/nextest.toml` configuration.

```bash
cargo nextest run                                    # Run all tests (serially, process-per-test)
cargo nextest run -E 'test(test_git_repo_from_path)' # Run a single test by name
cargo nextest run --test git_tests                   # Run a specific test file
cargo nextest run --no-capture                       # Run tests with output visible
```

Note: `cargo test` is not recommended for this project due to FUSE resource contention issues. Use `cargo nextest run` instead. If `cargo-nextest` is not installed, run: `cargo install cargo-nextest`

## Formatting and Linting

```bash
cargo fmt            # Format code
cargo fmt --check    # Check formatting
cargo clippy         # Lint
cargo clippy -- -D warnings  # Lint with warnings as errors
```

## Code Style Guidelines

### Import Organization

Organize imports in this order (no blank lines between groups):
1. External crates
2. Standard library
3. Internal modules (`crate::`)

```rust
use fuser::{FileAttr, FileType, Filesystem, Request, FUSE_ROOT_ID};
use std::path::{Path, PathBuf};
use crate::error::{Result, TreebeardError};
```

### Error Handling

- Use `thiserror` for error definitions in `src/error.rs`
- Custom `TreebeardError` enum with `#[from]` for automatic conversion
- Type alias: `pub type Result<T> = std::result::Result<T, TreebeardError>;`
- Propagate errors with `?` operator
- Use `map_err` for adding context:

```rust
let content = std::fs::read_to_string(&path)
    .map_err(|e| TreebeardError::Config(format!("Failed to read: {}", e)))?;
```

### Naming Conventions

| Type | Convention | Example |
|------|-----------|---------|
| Structs | PascalCase | `GitRepo`, `TreebeardFs`, `Config` |
| Functions | snake_case | `create_branch`, `mount_fuse` |
| Constants | SCREAMING_SNAKE_CASE | `TTL`, `FUSE_ROOT_ID` |
| Modules | snake_case | `overlay`, `cleanup` |
| Type aliases | PascalCase | `Result<T>` |
| Enum variants | PascalCase | `LayerType::Upper` |

### Type Patterns

- Use `PathBuf` for owned paths, `&Path` for borrowed
- Use `Arc<RwLock<T>>` for shared mutable state across threads
- Prefer `OsString`/`OsStr` for filesystem names
- Derive `Clone`, `Debug`, `Serialize`, `Deserialize` when appropriate

### Async Patterns

- Uses `tokio` runtime with `#[tokio::main]`
- Spawn background tasks with `tokio::spawn`
- Signal handling with `tokio::signal::ctrl_c()`
- Use `#[tokio::test]` for async tests

### Documentation

- Use `///` doc comments for public functions and types
- Include `// SAFETY:` comments before unsafe blocks

## Testing Patterns

- Integration tests in `tests/` directory
- Common utilities in `tests/shared/common.rs`
- E2E helpers in `tests/shared/e2e_helpers.rs` for PTY-based testing with `expectrl`
- Use `#[ignore = "reason"]` for skipped tests with explanation

For detailed information about E2E and integration test structure, see [`tests/TESTING.md`](tests/TESTING.md).

```rust
#[test]
fn test_basic_workflow_spawn_exit() {
    // ...
}

// For async tests
#[tokio::test]
async fn test_async_operation() {
    // ...
}
```

### Test Helpers

- `TestWorkspace` struct manages test repo setup/teardown
- `create_test_repo()` creates isolated git repos for testing
- `create_test_file(repo_path, filename, content)` creates files in test repos
- `git_commit_count(repo_path, branch)` counts commits on a branch
- `wait_for_file_system(duration_ms)` waits for filesystem operations
- Uses `TREEBEARD_TEST_MODE` and `TREEBEARD_DATA_DIR` env vars

### Property-Based Testing

Uses `proptest` for randomized input testing:

```rust
proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]
    #[test]
    fn test_special_char_filenames(filename in "[a-zA-Z0-9 _-]{1,50}") {
        // ...
    }
}
```

### Interactive Process Testing

Uses `expectrl` for testing interactive shell sessions:

```rust
use expectrl::{spawn, Regex};

let mut session = spawn("treebeard branch test-branch")?;
session.expect(Regex("prompt"))?;
session.send_line("exit")?;
```

## Module Structure

```
src/
├── cli/*.rs          # CLI parsing and validation
├── commands/*.rs     # Command implementations
├── session/*.rs      # Session management
├── sync/*.rs         # File sync functionality
├── overlay/*.rs      # FUSE filesystem
├── cleanup.rs        # Cleanup logic
├── config.rs         # Configuration
├── error.rs          # Error types
├── git.rs            # Git operations
├── hooks.rs          # Git hooks
├── lib.rs            # Library exports
├── main.rs           # Entry point
├── sandbox.rs        # macOS sandbox
├── shell.rs          # Shell spawning
└── watcher.rs        # File watching
```

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `TREEBEARD_LOG` | Log level (default: "info") |
| `TREEBEARD_TEST_MODE` | Enables test mode behavior |
| `TREEBEARD_DATA_DIR` | Override data directory location |
| `TREEBEARD_NO_CLEANUP` | Skip automatic stale mount cleanup |
| `TREEBEARD_ACTIVE` | Set in spawned shells |
| `TREEBEARD_BRANCH` | Current branch name in spawned shells |

## Key Dependencies

- `clap` - CLI parsing with derive
- `gix` - Pure Rust Git implementation
- `fuser` - FUSE filesystem bindings
- `tokio` - Async runtime
- `notify` - File watching
- `thiserror` - Error handling
- `tracing` - Logging
