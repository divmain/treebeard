# Test Structure

This document describes the organization of tests in the `tests/` directory.

## Directory Structure

```
tests/
├── e2e.rs                      # Entry point for end-to-end tests
├── integration.rs              # Entry point for integration tests
├── e2e/
│   ├── edge_cases/             # Tests for boundary conditions and error scenarios
│   ├── happy_path/             # Tests for normal, successful workflows
│   └── infrastructure/         # Tests for system-level concerns
├── integration/
│   ├── components/             # Tests for individual treebeard components
│   ├── config/                 # Tests for configuration handling
│   ├── fuse/                   # Tests for the FUSE overlay filesystem
│   └── git/                    # Tests for Git operations
└── shared/
    ├── mod.rs                  # Exports shared utilities
    ├── common.rs               # Common test infrastructure
    ├── e2e_helpers.rs          # PTY/expectrl-based session helpers
    └── fuse_helpers.rs         # FUSE mount session helpers
```

## Shared Utilities (`shared/`)

### `common.rs`

Core test infrastructure:

- **`TestWorkspace`**: Manages isolated test environments with temp directories, environment variable isolation, and automatic FUSE mount cleanup on drop
- **`TestConfigContext`**: Lighter-weight context for config-only tests
- **Helper functions**: `create_test_repo()`, `create_test_file()`, `git_commit_count()`, `get_branch_commits()`

### `e2e_helpers.rs`

PTY-based testing helpers using `expectrl`:

- **Session management**: `start_session()`, `exit_session()`, `spawn_treebeard_test_mode()`
- **Configuration types**: `SessionStartConfig`, `SessionExitConfig`, `SyncFlowResponse`
- **Shell command helpers**: `shell_exec()`, `shell_exec_output()`, `shell_create_file()`, `shell_read_file()`
- **Subcommand testing**: `spawn_treebeard_with_subcommand()`, `expect_error_contains()`

### `fuse_helpers.rs`

FUSE mount management for integration tests (macOS only):

- **`FuseTestSession`**: Manages FUSE mount lifecycle with multiple factory methods and automatic cleanup
- **Mount utilities**: `is_mount_active()`, `count_treebeard_mounts()`, `cleanup_all_test_mounts()`

## E2E Tests (`e2e/`)

E2E tests verify full user-facing behavior through the CLI using PTY-based interaction.

### Happy Path (`e2e/happy_path/`)

| Module | Purpose |
|--------|---------|
| `branch_creation` | `treebeard branch` creates Git branches correctly |
| `cleanup_lifecycle` | Graceful cleanup on Ctrl+C, squash behavior, commit preservation |
| `config_commands` | `config`, `config path`, `config show` subcommands |
| `doctor_diagnostics` | `doctor` command output (macOS, macFUSE, Git, disk space) |
| `file_operations` | File create/edit/delete with auto-commit through worktree |
| `fuse_overlay` | FUSE overlay behavior (append, truncate, read consistency) |
| `list_output` | `list` command formats (default, `--porcelain`, `--json`) |
| `path_commands` | `path` subcommand returning mount/worktree paths |
| `session_management` | Sequential sessions, `list` with active sessions, cleanup commands |
| `subcommand_execution` | `treebeard branch <name> -- <command>` execution |
| `sync_flow` | Sync flow for modified ignored files |
| `workflows` | Complete development workflows (feature dev, bugfix, docs) |

### Edge Cases (`e2e/edge_cases/`)

| Module | Purpose |
|--------|---------|
| `branch_collisions` | Behavior when branch/worktree already exists |
| `cleanup_conditions` | Cleanup flow edge cases and worktree preservation |
| `doctor_non_tty` | `doctor` command works without TTY |
| `error_handling` | Error messages for non-git dirs, invalid branch names |
| `git_interactions` | Git interaction edge cases |
| `special_filenames` | Unicode, emoji, long names, deep nesting (uses `proptest`) |
| `subcommand_failures` | Exit code propagation and failure messages |
| `tty_requirements` | TTY requirements for various commands |

### Infrastructure (`e2e/infrastructure/`)

| Module | Purpose |
|--------|---------|
| `mount_maintenance` | FUSE mount cleanup on termination |
| `output_formatting` | Enhanced `list` output format |

## Integration Tests (`integration/`)

Integration tests verify direct module integration without full CLI interaction.

### FUSE (`integration/fuse/`)

| Module | Purpose |
|--------|---------|
| `cow` | Copy-on-write behavior |
| `inodes` | Hard link inode tracking and TOCTOU handling |
| `lookup` | File lookup from both layers, overlay semantics |
| `mount` | Mount/unmount, multiple operations, backend detection |
| `passthrough` | Passthrough patterns (bypass upper layer) |
| `real_ops` | Real filesystem operations (readdir, whiteouts, flush, xattr) |
| `whiteouts` | Whiteout file creation for deletions |

### Git (`integration/git/`)

| Module | Purpose |
|--------|---------|
| `branch` | `GitRepo::create_branch()` and duplicate handling |
| `repo` | `GitRepo::from_path()` and `repo_name()` |
| `squash` | `squash_commits()` in worktree context |
| `stash` | `stash_push()` with various change types |
| `worktree` | Worktree creation, removal, external locations |

### Config (`integration/config/`)

| Module | Purpose |
|--------|---------|
| `defaults` | Default config values and placeholder replacement |
| `paths` | Path expansion and config file location |
| `patterns` | Glob pattern matching for sync patterns |
| `persistence` | Config save/load round-trip |
| `project_config` | Project-specific configuration |

### Components (`integration/components/`)

| Module | Purpose |
|--------|---------|
| `hooks` | Hooks config parsing and template variables |
| `sandbox` | macOS sandbox functionality |
| `watcher` | File watcher functionality |

## Testing Patterns

- **Process isolation**: Uses `cargo-nextest` for process-per-test isolation (required due to FUSE resource contention)
- **PTY testing**: Uses `expectrl` for interactive CLI testing with pseudo-TTY
- **RAII cleanup**: `TestWorkspace`, `FuseTestSession`, `MountCleanup` implement `Drop` for automatic cleanup
- **Property-based testing**: Uses `proptest` for randomized filename/content testing
- **Environment isolation**: Tests set `TREEBEARD_DATA_DIR` and `TREEBEARD_CONFIG_DIR`
- **macOS-only tests**: FUSE tests use `#[cfg(target_os = "macos")]`
