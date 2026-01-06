# Releasing Treebeard

This document describes how to release a new version of Treebeard to Homebrew.

## Quick Start

Run the release script:

```bash
./release.sh
```

The script will prompt for the new version number and handle everything automatically.

## Prerequisites

- macOS with macFUSE installed (required for building)
- Rust toolchain installed (`rustup`)
- `aarch64-apple-darwin` target installed: `rustup target add aarch64-apple-darwin`
- GitHub CLI installed and authenticated: `brew install gh && gh auth login`
- Push access to both `divmain/treebeard` and `divmain/homebrew-treebeard`

## Why Manual Releases?

GitHub Actions runners don't support macFUSE because it requires kernel extensions with special security entitlements. Builds must be done locally on a machine with macFUSE installed.

## What the Release Script Does

1. Validates prerequisites (Rust, gh CLI, target architecture)
2. Prompts for the new version number
3. Updates `Cargo.toml` with the new version
4. Runs formatting and lint checks (`cargo fmt --check`, `cargo clippy`)
5. Builds the release binary for `aarch64-apple-darwin`
6. Strips debug symbols and creates the release archive
7. Commits and pushes the version bump to main
8. Creates a git tag and GitHub release on `divmain/treebeard`
9. Uploads the binary to `divmain/homebrew-treebeard` releases
10. Updates the Cask formula with the new version and SHA256

## Manual Release Process

If you need to perform the release manually (or if the script fails partway through), follow these steps:

### 1. Prepare the Release

Ensure all changes are committed and the code is ready for release:

```bash
# Run tests
cargo nextest run

# Run lints
cargo clippy -- -D warnings

# Check formatting
cargo fmt --check
```

### 2. Update Version

Update the version in `Cargo.toml`:

```bash
# Edit Cargo.toml and update: version = "X.Y.Z"
```

Commit the version bump:

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to vX.Y.Z"
git push origin main
```

### 3. Build the Release Binary

```bash
# Build release binary for Apple Silicon
cargo build --release --target aarch64-apple-darwin

# Strip debug symbols to reduce binary size
strip target/aarch64-apple-darwin/release/treebeard

# Create the release archive
cd target/aarch64-apple-darwin/release
tar -czf treebeard-aarch64-apple-darwin.tar.gz treebeard
cd -
```

### 4. Calculate SHA256

```bash
shasum -a 256 target/aarch64-apple-darwin/release/treebeard-aarch64-apple-darwin.tar.gz
```

Save this hash for the next step.

### 5. Create GitHub Release on treebeard repo

```bash
# Create and push the tag
git tag vX.Y.Z
git push origin vX.Y.Z

# Create the release (this just documents the release, no assets needed here)
gh release create vX.Y.Z --title "vX.Y.Z" --generate-notes
```

### 6. Upload Binary to homebrew-treebeard

The Homebrew tap hosts the actual binary downloads:

```bash
# Create a release with the binary on the homebrew tap
gh release create vX.Y.Z \
  --repo divmain/homebrew-treebeard \
  --title "vX.Y.Z" \
  --notes "Treebeard vX.Y.Z" \
  target/aarch64-apple-darwin/release/treebeard-aarch64-apple-darwin.tar.gz
```

### 7. Update the Cask Formula

Clone and update the homebrew tap:

```bash
git clone git@github.com:divmain/homebrew-treebeard.git /tmp/homebrew-treebeard
cd /tmp/homebrew-treebeard
```

Edit `Casks/treebeard.rb`:

```ruby
cask "treebeard" do
  version "X.Y.Z"  # Update this
  sha256 "YOUR_SHA256_HASH"  # Update this with the hash from step 4

  # ... rest of the file
  url "https://github.com/divmain/homebrew-treebeard/releases/download/vX.Y.Z/treebeard-aarch64-apple-darwin.tar.gz"
  # ...
end
```

Commit and push:

```bash
git add Casks/treebeard.rb
git commit -m "Update treebeard to vX.Y.Z"
git push origin main
```

### 8. Verify the Release

```bash
# Update Homebrew
brew update

# Upgrade or install
brew upgrade treebeard
# or
brew install divmain/treebeard/treebeard

# Verify version
treebeard --version
```

## Troubleshooting

### Binary won't run after installation

Ensure the binary was built on a compatible macOS version. The cask requires macOS Sequoia (15.0) or later.

### SHA256 mismatch

Re-download the binary and recalculate the hash. Ensure you're using the exact file that was uploaded to the release.

### macFUSE issues

Users need to install macFUSE separately and approve the kernel extension in System Settings > Privacy & Security.

### Release script failed partway through

Check where it failed and continue from that step using the manual process above. The script is designed to fail fast, so you can pick up where it left off.
