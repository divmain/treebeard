#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
HOMEBREW_TAP_REPO="divmain/homebrew-treebeard"
TARGET="aarch64-apple-darwin"
BINARY_NAME="treebeard"
ARCHIVE_NAME="${BINARY_NAME}-${TARGET}.tar.gz"

info() {
    echo -e "${BLUE}==>${NC} $1"
}

success() {
    echo -e "${GREEN}==>${NC} $1"
}

warn() {
    echo -e "${YELLOW}==>${NC} $1"
}

error() {
    echo -e "${RED}ERROR:${NC} $1"
    exit 1
}

prompt() {
    echo -e "${YELLOW}$1${NC}"
}

# Check prerequisites
check_prerequisites() {
    info "Checking prerequisites..."
    
    local missing=()
    
    if ! command -v cargo &> /dev/null; then
        missing+=("cargo (Rust toolchain)")
    fi
    
    if ! command -v gh &> /dev/null; then
        missing+=("gh (GitHub CLI - brew install gh)")
    fi
    
    if ! command -v git &> /dev/null; then
        missing+=("git")
    fi
    
    if ! rustup target list --installed | grep -q "$TARGET"; then
        missing+=("Rust target $TARGET (run: rustup target add $TARGET)")
    fi
    
    if [ ${#missing[@]} -ne 0 ]; then
        error "Missing prerequisites:\n  - ${missing[*]}"
    fi
    
    # Check gh authentication
    if ! gh auth status &> /dev/null; then
        error "GitHub CLI not authenticated. Run: gh auth login"
    fi
    
    success "All prerequisites met"
}

# Get current version from Cargo.toml
get_current_version() {
    grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'
}

# Validate version format
validate_version() {
    if [[ ! $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        error "Invalid version format: $1 (expected: X.Y.Z)"
    fi
}

# Update version in Cargo.toml
update_cargo_version() {
    local version=$1
    info "Updating Cargo.toml version to $version..."
    
    # Use sed to update version (macOS compatible)
    sed -i '' "s/^version = \".*\"/version = \"$version\"/" Cargo.toml
    
    # Verify the change
    local new_version=$(get_current_version)
    if [ "$new_version" != "$version" ]; then
        error "Failed to update Cargo.toml version"
    fi
    
    success "Updated Cargo.toml to version $version"
}

# Run pre-release checks
run_checks() {
    info "Running pre-release checks..."
    
    info "Checking formatting..."
    if ! cargo fmt --check; then
        error "Code formatting check failed. Run: cargo fmt"
    fi
    
    info "Running clippy..."
    if ! cargo clippy -- -D warnings; then
        error "Clippy found issues"
    fi
    
    success "All checks passed"
}

# Build the release binary
build_release() {
    info "Building release binary for $TARGET..."
    
    cargo build --release --target "$TARGET"
    
    local binary_path="target/$TARGET/release/$BINARY_NAME"
    if [ ! -f "$binary_path" ]; then
        error "Binary not found at $binary_path"
    fi
    
    info "Stripping debug symbols..."
    strip "$binary_path" || warn "Strip failed (non-fatal)"
    
    success "Build complete"
}

# Create release archive
create_archive() {
    info "Creating release archive..."
    
    local release_dir="target/$TARGET/release"
    
    # Create archive
    tar -czf "$release_dir/$ARCHIVE_NAME" -C "$release_dir" "$BINARY_NAME"
    
    if [ ! -f "$release_dir/$ARCHIVE_NAME" ]; then
        error "Failed to create archive"
    fi
    
    success "Archive created: $release_dir/$ARCHIVE_NAME"
}

# Calculate SHA256
calculate_sha256() {
    local archive_path="target/$TARGET/release/$ARCHIVE_NAME"
    shasum -a 256 "$archive_path" | cut -d' ' -f1
}

# Commit version bump
commit_version_bump() {
    local version=$1
    
    info "Committing version bump..."
    
    # Update Cargo.lock by running cargo check
    cargo check --quiet 2>/dev/null || true
    
    git add Cargo.toml Cargo.lock
    git commit -m "chore: bump version to v$version"
    git push origin main
    
    success "Version bump committed and pushed"
}

# Create tag and release on main repo
create_main_release() {
    local version=$1
    
    info "Creating tag v$version..."
    git tag "v$version"
    git push origin "v$version"
    
    info "Creating GitHub release on treebeard repo..."
    gh release create "v$version" \
        --title "v$version" \
        --generate-notes
    
    success "Release v$version created on treebeard repo"
}

# Create release on homebrew tap and upload binary
create_homebrew_release() {
    local version=$1
    local sha256=$2
    local archive_path="target/$TARGET/release/$ARCHIVE_NAME"
    
    info "Creating release on homebrew-treebeard and uploading binary..."
    
    gh release create "v$version" \
        --repo "$HOMEBREW_TAP_REPO" \
        --title "v$version" \
        --notes "Treebeard v$version" \
        "$archive_path"
    
    success "Binary uploaded to homebrew-treebeard releases"
}

# Update the Cask formula
update_cask_formula() {
    local version=$1
    local sha256=$2
    
    info "Updating Cask formula in homebrew-treebeard..."
    
    # Clone the tap repo to a temp directory
    local tmp_dir=$(mktemp -d)
    local tap_dir="$tmp_dir/homebrew-treebeard"
    
    git clone --depth 1 "git@github.com:$HOMEBREW_TAP_REPO.git" "$tap_dir"
    
    local cask_file="$tap_dir/Casks/treebeard.rb"
    
    if [ ! -f "$cask_file" ]; then
        rm -rf "$tmp_dir"
        error "Cask file not found at $cask_file"
    fi
    
    # Update version
    sed -i '' "s/version \".*\"/version \"$version\"/" "$cask_file"
    
    # Update sha256
    sed -i '' "s/sha256 \".*\"/sha256 \"$sha256\"/" "$cask_file"
    
    # Update URL
    sed -i '' "s|releases/download/v[^/]*/|releases/download/v$version/|" "$cask_file"
    
    # Commit and push
    cd "$tap_dir"
    git add Casks/treebeard.rb
    git commit -m "Update treebeard to v$version"
    git push origin main
    cd - > /dev/null
    
    # Cleanup
    rm -rf "$tmp_dir"
    
    success "Cask formula updated"
}

# Verify the release
verify_release() {
    local version=$1
    
    info "Verifying release..."
    
    prompt "To verify the release, run:"
    echo "  brew update"
    echo "  brew upgrade treebeard  # or: brew install divmain/treebeard/treebeard"
    echo "  treebeard --version"
    echo ""
    prompt "Expected version: $version"
}

# Main script
main() {
    echo ""
    echo "==============================="
    echo "       Treebeard Release"
    echo "==============================="
    echo ""
    
    # Ensure we're in the repo root
    if [ ! -f "Cargo.toml" ]; then
        error "Must be run from the treebeard repository root"
    fi
    
    # Ensure we're on main branch
    local current_branch=$(git branch --show-current)
    if [ "$current_branch" != "main" ]; then
        error "Must be on main branch (currently on: $current_branch)"
    fi
    
    # Ensure working directory is clean
    if [ -n "$(git status --porcelain)" ]; then
        error "Working directory is not clean. Commit or stash changes first."
    fi
    
    # Pull latest
    info "Pulling latest changes..."
    git pull origin main
    
    # Check prerequisites
    check_prerequisites
    
    # Get version info
    local current_version=$(get_current_version)
    echo ""
    info "Current version: $current_version"
    echo ""
    
    # Prompt for new version
    prompt "Enter the new version (X.Y.Z):"
    read -r new_version
    
    validate_version "$new_version"
    
    # Confirm
    echo ""
    prompt "This will:"
    echo "  1. Update Cargo.toml to version $new_version"
    echo "  2. Run formatting and lint checks"
    echo "  3. Build release binary for $TARGET"
    echo "  4. Commit and push version bump to main"
    echo "  5. Create tag v$new_version and GitHub release"
    echo "  6. Upload binary to homebrew-treebeard releases"
    echo "  7. Update the Cask formula"
    echo ""
    prompt "Continue? (y/N):"
    read -r confirm
    
    if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 0
    fi
    
    echo ""
    
    # Execute release steps
    update_cargo_version "$new_version"
    run_checks
    build_release
    create_archive
    
    local sha256=$(calculate_sha256)
    info "SHA256: $sha256"
    
    commit_version_bump "$new_version"
    create_main_release "$new_version"
    create_homebrew_release "$new_version" "$sha256"
    update_cask_formula "$new_version" "$sha256"
    
    echo ""
    echo "=========================================="
    success "Release v$new_version complete!"
    echo "=========================================="
    echo ""
    
    verify_release "$new_version"
}

main "$@"
