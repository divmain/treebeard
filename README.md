# Treebeard: Isolating AI wizards that should know better

Sometimes AI coding tools turn evil on you, just like Saruman.

That's why you need Treebeard.

Don't be hasty. Contain your wizards properly.

## Overview

Treebeard runs subprocesses inside ephemeral Git worktree environments where you can experiment freely without affecting your main repository. It uses a FUSE overlay filesystem to provide copy-on-write behavior for ignored files (like build artifacts, local config, etc.), while keeping your tracked Git files properly isolated in the worktree. Your `.env` and `node_modules` are there immediately, without risking the corruption of your original repository, and any changes you make can by optionally sync'd back to the main repository.

Additionally, Treebeard will prevent the subprocess from reading & writing things that it shouldn't. You can lock down the network, too, if that suits you.

## Features

- **Ephemeral worktrees**: Create isolated branches that disappear when you're done
- **Copy-on-write for ignored files**: Efficiently handle large build artifacts and other ignored files
- **Automatic cleanup**: Removes the worktree and FUSE mount when the shell exits
- **Auto-commit**: Automatically commits changes to files during your session
- **Squash commits**: Optionally squash all commits into a single commit on exit

## Installation

### Prerequisites

treebeard requires macOS 15 (Sequoia) or later and macFUSE:

```bash
# Install macFUSE
brew install --cask macfuse
```

After installing macFUSE, you'll need to approve the kernel extension in System Settings → Privacy & Security.

### Homebrew (recommended)

```bash
brew tap divmain/treebeard
brew install treebeard
```

### Build from source

```bash
cargo install --path .
```

## Usage

Create a new branch environment:

```bash
treebeard branch feature-xyz
```

This will:
1. Create a new Git branch `feature-xyz`
2. Create a worktree for that branch
3. Mount a FUSE overlay filesystem with:
   - **Lower layer**: Your main repository's ignored files (read-only)
   - **Upper layer**: Worktree-specific modifications to ignored files (writeable)
4. Start a shell in the mounted environment

### Running commands instead of a shell

You can run a specific command instead of starting a shell:

```bash
treebeard branch feature-xyz -- make test
```

### Listing active sessions

```bash
treebeard list
```

The `list` command supports two machine-readable output formats:

```bash
# JSON output
treebeard list --json

# Porcelain output (tab-separated values)
treebeard list --porcelain
```

### Manual cleanup

```bash
treebeard cleanup feature-xyz
```

## Architecture

### Module Structure

The codebase is organized into focused modules:

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

### FUSE Backend

treebeard uses **macFUSE's VFS backend** (kernel extension), not the user-space FSKit backend.

**Why VFS?**
- Better I/O performance
- Works on all macOS versions where macFUSE is supported
- More mature and stable
- FSKit would require a complete filesystem rewrite

**Trade-offs:**
- Requires one-time kernel extension approval in System Settings
- Requires macOS 15 or later
- Theoretical future risk if Apple removes kernel extension support

**Alternatives considered:**
- **FSKit**: User-space backend available on macOS 15.4+, requires `fskit-rs` + FSKitBridge app (high migration effort, immature ecosystem)
- **FUSE-T**: NFS-based alternative that could be considered if kernel extensions are eventually removed

### Filesystem Structure

The overlay filesystem provides copy-on-write semantics:

```
/mount/point/                          # What you see in your shell
├── .git/                              # Symlink to worktree's .git
├── node_modules/                      # Copy-on-write from main repo
├── build/                             # Copy-on-write from main repo
└── ...                                # Other files

Worktree/.git/                         # Actual Git worktree
├── ...
├── treebeard-upper/                   # Upper layer: Modifications to ignored files
└── ...

Main repo/                             # Your original repository
├── .git/
├── node_modules/                      # Lower layer (source)
├── build/                             # Lower layer (source)
└── ...
```

When you modify an ignored file (like `node_modules/package/thing.js`), it gets copied to the upper layer and modified there. The lower layer remains untouched.

### Git Integration

- Tracked files: Managed by Git worktrees (standard behavior)
- Ignored files: Managed by FUSE overlay (copy-on-write)
- On exit: You're prompted to sync modified ignored files back to the main repo

## Configuration

### User Configuration

treebeard uses a configuration file at `~/.config/treebeard/config.toml`:

```toml
[paths]
worktree_dir = "~/.local/share/treebeard/worktrees"
mount_dir = "~/.local/share/treebeard/mounts"

[cleanup]
# What to do on exit: "prompt", "squash", or "keep"
on_exit = "prompt"

[commit]
# Auto-commit message for modified ignored files
auto_commit_message = "treebeard: auto-save"
# Message for squashing commits (use {branch} as placeholder)
squash_commit_message = "treebeard: {branch}"

[auto_commit_timing]
# Debounce time for auto-commit (milliseconds)
auto_commit_debounce_ms = 5000

[sync]
# Patterns to always skip when syncing ignored files back to main repo
sync_always_skip = []
# Patterns to always include when syncing
sync_always_include = []

[hooks]
# Commands to run after worktree and mount are created
post_create = []
# Commands to run before cleanup starts
pre_cleanup = []
# Commands to run after successful cleanup
post_cleanup = []
# Command to generate commit messages (stdout is used as the message)
# commit_message = "echo 'Auto-commit'"

[sandbox]
# Master switch for sandboxing (default: true on macOS)
enabled = true
# Paths to deny reading (default includes ~/.ssh, ~/.aws, ~/.gnupg, etc.)
deny_read = ["~/.ssh", "~/.aws", "~/.gnupg", "~/.config/gh"]
# Additional paths to allow writing (beyond mount path and /tmp)
allow_write = []

[sandbox.network]
# Network mode: "allow", "localhost", or "deny"
mode = "allow"
# Hosts to allow when mode = "localhost" or "deny"
allow_hosts = []
```

### Project Configuration

You can also create a project-specific configuration file at `.treebeard.toml` in your repository root. This allows you to override user settings on a per-project basis.

**Merge order**: Settings are merged in the following order (later sources override earlier ones):
1. Built-in defaults
2. User config (`~/.config/treebeard/config.toml`)
3. Project config (`.treebeard.toml` in repo root)

**Project config example**:

```toml
# .treebeard.toml (in repository root)

[hooks]
# Run project-specific setup after creating the worktree
post_create = ["pnpm install"]

[cleanup]
# Always squash commits on exit for this project
on_exit = "squash"

[sync]
# Always sync this specific cache directory
sync_always_include = ["node_modules/.cache"]

# But never sync these build artifacts
sync_always_skip = ["dist/**", "build/**"]
```

**Common use cases**:

- **Project-specific hooks**: Run `pnpm install` for Node.js projects or `pip install` for Python projects
- **Team alignment**: Commit `.treebeard.toml` to version control so all team members use the same settings
- **Cleanup behavior**: Use `squash` for feature branches, `keep` for long-running experiments
- **Sync patterns**: Customize which ignored files get synced based on project structure

### Hooks

treebeard supports lifecycle hooks that run shell commands at specific points during worktree operations. Hooks are executed via `sh -c` and can use template variables.

#### Hook Types

| Hook | When it runs | Working directory |
|------|--------------|-------------------|
| `post_create` | After worktree and FUSE mount are created | Mount path |
| `pre_cleanup` | Before cleanup starts | Mount path (if mounted) or worktree path |
| `post_cleanup` | After cleanup completes | Main repository path |
| `commit_message` | When generating auto-commit messages | Worktree path |

#### Template Variables

All hooks support these template variables:

| Variable | Description |
|----------|-------------|
| `{{branch}}` | Branch name |
| `{{mount_path}}` | FUSE mount path |
| `{{worktree_path}}` | Git worktree path |
| `{{repo_path}}` | Main repository path |
| `{{diff}}` | Diff of changes (only for `commit_message` hook) |

#### Environment Variables

Hooks also receive these environment variables:

- `TREEBEARD_BRANCH` - Branch name
- `TREEBEARD_MOUNT_PATH` - FUSE mount path
- `TREEBEARD_WORKTREE_PATH` - Git worktree path
- `TREEBEARD_REPO_PATH` - Main repository path

#### Examples

**Install dependencies after creating a worktree:**

```toml
[hooks]
post_create = [
    "npm install",
    "cp .env.example .env",
]
```

**Run build before cleanup:**

```toml
[hooks]
pre_cleanup = [
    "npm run build",
]
```

**Notify when cleanup completes:**

```toml
[hooks]
post_cleanup = [
    "echo 'Cleaned up {{branch}}'",
]
```

**Generate commit messages with an LLM:**

```toml
[hooks]
# Use an LLM to generate commit messages from the diff
commit_message = "echo '{{diff}}' | llm -s 'Write a concise commit message for these changes. Output only the raw message.' --no-stream"
```

**Fallback pattern for commit messages:**

```toml
[hooks]
# Try LLM, fall back to default if it fails
commit_message = "echo '{{diff}}' | llm -s 'Write commit message' 2>/dev/null || echo 'treebeard: auto-save'"
```

#### Hook Behavior

- Hooks run sequentially in the order defined
- If a hook fails (non-zero exit code), subsequent hooks in that phase are skipped
- Hook failures are logged as warnings but don't prevent treebeard from continuing
- The `commit_message` hook's stdout is trimmed and used as the commit message
- If `commit_message` produces empty output or fails, the default `auto_commit_message` is used

### Sandbox (macOS)

treebeard includes built-in sandbox support on macOS using `sandbox-exec`. When enabled (the default on macOS), subprocesses spawned by treebeard run with restricted filesystem and network access. This is especially useful for AI coding tools that should not have access to sensitive data like SSH keys, AWS credentials, or your personal documents.

**⚠️ Important**: By default, the sandbox blocks read access to common user directories:

- `~/.ssh` - SSH private keys
- `~/.aws` - AWS credentials
- `~/.gnupg` - GPG keys
- `~/.config/gh` - GitHub CLI tokens
- `~/Documents` - Your documents folder
- `~/Pictures` - Your pictures folder
- `~/Desktop` - Your desktop folder

This is intentional security isolation to prevent AI tools or untrusted code from accessing sensitive data. If you need access to these directories, you can remove them from the `deny_read` list or disable the sandbox entirely.

#### How It Works

- **Filesystem Reads**: Allowed by default, except paths in the `deny_read` list
- **Filesystem Writes**: Denied by default, except:
  - The FUSE mount path (worktree overlay)
  - Temp directories (`/tmp`, `/private/tmp`, `/var/folders`)
  - Paths in the `allow_write` list
- **Process Execution**: Unrestricted
- **Network**: Allowed by default; can be restricted via `sandbox.network.mode`

#### Configuration

```toml
# ~/.config/treebeard/config.toml
# Can also be specified in project-level .treebeard.toml (replaces global config)

[sandbox]
# Master switch for sandboxing (default: true on macOS)
enabled = true

# Paths to deny reading (sensitive data)
# These paths are blocked from read access by sandboxed subprocesses
deny_read = [
    "~/.ssh",
    "~/.aws",
    "~/.gnupg",
    "~/.config/gh",      # GitHub CLI tokens
    "~/Documents",
    "~/Pictures",
    "~/Desktop",
]

# Additional paths to allow writing (beyond mount path and /tmp)
# Most use cases won't need this
allow_write = []

[sandbox.network]
# Network access mode:
#   "allow"     - No network restrictions (default)
#   "localhost" - Only localhost + allow_hosts
#   "deny"      - Only allow_hosts
mode = "allow"

# Hosts to allow when mode = "localhost" or "deny"
allow_hosts = []
```

#### What This Prevents

- AI tools reading SSH keys, AWS credentials, or GPG keys
- Exfiltration of personal documents
- Unauthorized network access (when network mode is restricted)
- Modification of files outside the worktree and temp directories

#### Disabling the Sandbox

To disable sandboxing entirely:

```toml
[sandbox]
enabled = false
```

Or for a specific project, create a `.treebeard.toml` in the repository root:

```toml
# .treebeard.toml
[sandbox]
enabled = false
```

### Lower Layer Passthrough

By default, treebeard's overlay filesystem provides copy-on-write semantics: reads come from the lower layer (your main repository), but writes go to the upper layer (worktree-specific). The `passthrough` option lets you bypass this behavior for specific paths, making reads and writes go directly to the lower layer.

**Use case**: AI coding tools like Claude Code and Cursor store their configuration and session data in directories like `.claude/` and `.cursor/`. You may want these tools to share state across all worktrees and persist changes back to your main repository immediately.

#### Configuration

```toml
[paths]
passthrough = [".claude/**", ".cursor/**"]
```

#### Glob Pattern Syntax

- `**` matches any number of directories (e.g., `.claude/**` matches everything under `.claude/`)
- `*` matches any characters within a single path component
- Exact paths also work (e.g., `.claude/settings.json`)

#### Behavior

For paths matching passthrough patterns:

| Operation | Normal Overlay Behavior | Passthrough Behavior |
|-----------|------------------------|---------------------|
| Read | Check upper, then lower | Lower layer only |
| Write | Copy-up to upper, write there | Write directly to lower |
| Delete | Create whiteout in upper | Delete from lower |
| Create | Create in upper | Create in lower |
| Readdir | Merge upper and lower | Lower layer only |

**Important**: Passthrough files are modified in your main repository immediately. Changes are not isolated to the worktree.

#### Example Configurations

**Share Claude Code state across worktrees:**

```toml
[paths]
passthrough = [".claude/**"]
```

**Share multiple AI tool directories:**

```toml
[paths]
passthrough = [".claude/**", ".cursor/**", ".aider/**"]
```

**Share specific config files:**

```toml
[paths]
passthrough = [".claude/settings.json", ".cursor/config.json"]
```

## Troubleshooting

### "failed to open device" error

This means you've hit macFUSE's limit of 64 simultaneous mounts. Run treebeard and wait for it to clean up stale mounts automatically, or clean them manually:

```bash
# Manually clean up treebeard mounts
mount | grep treebeard
diskutil unmount force /path/to/stale/mount
```

### Skip automatic cleanup

If you have multiple intentional treebeard sessions, set:

```bash
export TREEBEARD_NO_CLEANUP=1
```

### macOS version error

treebeard requires macOS 15 (Sequoia) or later. Earlier versions are not supported.

## Testing

Run the test suite:

```bash
# All tests (uses cargo-nextest for process isolation)
cargo nextest run

# E2E tests
cargo nextest run --test e2e_tests

# Real FUSE tests (requires macFUSE installed)
FUSE_TESTS=REAL cargo nextest run --test fuse_real_tests -- --ignored
```

## License

[MIT](./LICENSE)
