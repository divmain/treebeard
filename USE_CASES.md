# USE_CASES.md - Composing treebeard with Other Tools

treebeard follows the Unix philosophy: it does one thing well (isolated worktree environments with copy-on-write for ignored files) and composes with other tools via standard interfaces.

This document demonstrates how treebeard integrates into various development workflows.

## Core Composition Patterns

### Pattern 1: Pipe-Friendly Output

treebeard provides machine-readable output for scripting:

```bash
# List worktrees in porcelain format (proposed feature)
git treebeard list --porcelain
# feature-auth<TAB>/path/to/mount<TAB>mounted<TAB>3
# bugfix-123<TAB>/path/to/mount<TAB>mounted<TAB>0

# Get just the mount path for a branch (proposed feature)
git treebeard path feature-auth
# /Users/dev/.local/share/treebeard/mounts/myrepo/feature-auth
```

### Pattern 2: Command Passthrough

Run any command in the isolated environment:

```bash
# Run tests in isolation
git treebeard branch feature-auth -- cargo nextest run

# Run a build
git treebeard branch feature-auth -- make build

# Start an AI coding tool
git treebeard branch feature-auth -- claude
```

### Pattern 3: Environment Variables

treebeard sets environment variables for subprocess awareness:

```bash
TREEBEARD_ACTIVE=1        # Indicates running inside treebeard
TREEBEARD_BRANCH=feature  # Current branch name
```

---

## AI Coding Tool Workflows

Run AI-assisted coding tools in an isolated worktree with copy-on-write access to dependencies.

**Benefits:**
- The AI coding tool cannot accidentally modify your main repo's node_modules, .env, or build artifacts
- Changes to ignored files are tracked and can be synced back on exit
- Multiple coding tool sessions can run in parallel without conflicts

### OpenCode

```bash
git treebeard branch feature-ui -- opencode
```

### Claude Code

```bash
# Start Claude in isolated environment
git treebeard branch feature-auth -- claude

# With initial prompt
git treebeard branch feature-auth -- claude --message "Implement OAuth flow"

# Non-interactive mode for automation
git treebeard branch feature-auth -- claude -p "Fix the failing tests" --allowedTools Edit,Bash
```

### Codex CLI

```bash
git treebeard branch feature-cli -- codex
```

### Parallel AI Agents

Run multiple AI agents simultaneously, each in their own isolated environment:

```bash
#!/bin/bash
# parallel-agents.sh

FEATURES=("auth" "api" "ui" "tests")

for feature in "${FEATURES[@]}"; do
    git treebeard branch "feature-$feature" -- claude -p "Implement $feature" &
done

wait
echo "All agents completed"
```

---

## Terminal Multiplexer Integration

### tmux

Create tmux windows for each worktree:

```bash
# Shell function for tmux integration
tb-tmux() {
    local branch="$1"
    shift
    local cmd="${*:-$SHELL}"
    
    # Create new tmux window with treebeard session
    tmux new-window -n "$branch" "git treebeard branch $branch -- $cmd"
}

# Usage
tb-tmux feature-auth
tb-tmux feature-auth claude
```

**Advanced tmux layout:**

```bash
# Create development layout with treebeard
tb-dev() {
    local branch="$1"
    local mount_path=$(git treebeard path "$branch" 2>/dev/null)
    
    if [ -z "$mount_path" ]; then
        # Branch doesn't exist, create it with --no-shell first
        git treebeard branch "$branch" --no-shell
        mount_path=$(git treebeard path "$branch")
    fi
    
    # Create tmux layout
    tmux new-window -n "$branch" -c "$mount_path"
    tmux split-window -h -c "$mount_path"
    tmux split-window -v -c "$mount_path"
    tmux select-pane -t 0
    
    # Start services in panes
    tmux send-keys -t 0 "nvim" Enter
    tmux send-keys -t 1 "npm run dev" Enter
    tmux send-keys -t 2 "npm run test:watch" Enter
}
```

### Zellij

Zellij is a terminal multiplexer that can be powerfully combined with treebeard to create instant, pre-configured development environments.

The following shell function demonstrates a powerful pattern: capturing the branch name in a variable (`local branch="$1"`) and using it to:
1.  **Prepare the environment**: Ensure the treebeard worktree exists and get its path.
2.  **Contextualize the session**: Name the Zellij session after the branch so you can easily attach/detach (`zellij attach feature-auth`).
3.  **Set the working directory**: Use `options --default-cwd` so every new pane automatically opens inside the isolated worktree.

```bash
# Add this to your ~/.zshrc or ~/.bashrc
tb-zellij() {
    # 1. Capture the branch name argument
    local branch="$1"
    
    if [ -z "$branch" ]; then
        echo "Usage: tb-zellij <branch-name>"
        return 1
    fi

    # 2. Ensure the treebeard environment exists (without spawning a shell yet)
    #    This creates the worktree and FUSE mount if they don't exist.
    git treebeard branch "$branch" --no-shell
    
    # 3. Get the absolute path to the FUSE mount point
    local mount_path=$(git treebeard path "$branch")
    
    # 4. Launch Zellij with a specific layout and configuration
    #    -s "$branch": Name the session after the branch (e.g., "feature-auth")
    #    --layout dev: Use a layout file named "dev.kdl" (see below)
    #    options --default-cwd ...: Ensure all new panes open in the mount path
    zellij --layout dev -s "$branch" options --default-cwd "$mount_path"
}
```

**Zellij layout file** (`~/.config/zellij/layouts/dev.kdl`):

This layout defines your ideal workspace. Because we passed `--default-cwd`, every pane defined here will open inside the correct treebeard mount.

```kdl
layout {
    // Standard tab bar at the top
    pane size=1 borderless=true {
        plugin location="compact-bar"
    }
    
    // Main development area
    pane split_direction="vertical" {
        // Left: Editor (takes up 60% width)
        pane size="60%" {
            // You could even set a specific command, e.g., 'command "nvim"'
            // but leaving it empty opens your default shell.
            name "Editor"
        }
        
        // Right: Stacked utility panes
        pane split_direction="horizontal" {
            // Top right: Server runner
            pane {
                name "Server"
                command "npm"
                args "run" "dev" 
            }
            
            // Bottom right: Test runner or git status
            pane {
                name "Terminal"
            }
        }
    }
}
```

**Workflow:**

1.  Run `tb-zellij feature-auth`.
2.  Zellij launches instantly.
3.  You have `nvim` on the left, your server running on the top right, and a free terminal on the bottom right.
4.  **Crucially**, all of them are inside `/Users/dev/.local/share/treebeard/mounts/myrepo/feature-auth`, completely isolated from your main repository.

### Tmuxinator

```yaml
# ~/.tmuxinator/treebeard-dev.yml
name: <%= @args[0] %>
root: <%= `git treebeard path #{@args[0]} 2>/dev/null`.strip %>

pre_window: |
  if [ -z "$TREEBEARD_ACTIVE" ]; then
    echo "Warning: Not in a treebeard session"
  fi

windows:
  - editor:
      layout: main-vertical
      panes:
        - nvim
        - cargo watch -x test
  - server:
      panes:
        - cargo run
        - tail -f logs/app.log
  - ai:
      panes:
        - claude
```

Usage:
```bash
git treebeard branch feature-auth --no-shell
tmuxinator start treebeard-dev feature-auth
```

---

## Interactive Selection with fzf

### Basic worktree selection

```bash
# Select and switch to a treebeard worktree
tb-select() {
    local selection=$(git treebeard list --porcelain | \
        fzf --delimiter='\t' \
            --with-nth=1 \
            --preview 'git -C {2} log --oneline -10' \
            --preview-window=right:50%)
    
    if [ -n "$selection" ]; then
        local branch=$(echo "$selection" | cut -f1)
        git treebeard branch "$branch"
    fi
}
```

### Create new worktree from branch list

```bash
# Interactive branch selection for new worktree
tb-new() {
    local branch=$(git branch -a --format='%(refname:short)' | \
        fzf --prompt="Select branch for worktree: " \
            --preview 'git log --oneline -10 {}')
    
    if [ -n "$branch" ]; then
        # Strip remotes/origin/ prefix if present
        branch=${branch#remotes/origin/}
        git treebeard branch "$branch"
    fi
}
```

### Cleanup with preview

```bash
# Interactive cleanup with diff preview
tb-cleanup() {
    local branches=$(git treebeard list --porcelain | \
        fzf --multi \
            --delimiter='\t' \
            --with-nth=1 \
            --preview 'git -C {2} diff --stat' \
            --preview-window=right:60%)
    
    if [ -n "$branches" ]; then
        echo "$branches" | cut -f1 | while read branch; do
            git treebeard cleanup "$branch" --yes
        done
    fi
}
```

---

## Editor Integration

### Neovim/Vim

```lua
-- ~/.config/nvim/lua/treebeard.lua

local M = {}

-- Open file in treebeard worktree
function M.open_in_worktree(branch, file)
    local path = vim.fn.system('git treebeard path ' .. branch):gsub('\n', '')
    if vim.v.shell_error == 0 then
        vim.cmd('edit ' .. path .. '/' .. file)
    else
        vim.notify('Worktree not found: ' .. branch, vim.log.levels.ERROR)
    end
end

-- Telescope picker for treebeard worktrees
function M.telescope_worktrees()
    local pickers = require('telescope.pickers')
    local finders = require('telescope.finders')
    local actions = require('telescope.actions')
    local action_state = require('telescope.actions.state')
    
    local worktrees = vim.fn.systemlist('git treebeard list --porcelain')
    
    pickers.new({}, {
        prompt_title = 'Treebeard Worktrees',
        finder = finders.new_table({
            results = worktrees,
            entry_maker = function(entry)
                local parts = vim.split(entry, '\t')
                return {
                    value = entry,
                    display = parts[1],
                    ordinal = parts[1],
                    path = parts[2],
                }
            end,
        }),
        attach_mappings = function(prompt_bufnr, map)
            actions.select_default:replace(function()
                actions.close(prompt_bufnr)
                local selection = action_state.get_selected_entry()
                vim.cmd('cd ' .. selection.path)
            end)
            return true
        end,
    }):find()
end

return M
```

### VS Code

```json
// .vscode/tasks.json
{
    "version": "2.0.0",
    "tasks": [
        {
            "label": "treebeard: new branch",
            "type": "shell",
            "command": "git treebeard branch ${input:branchName}",
            "problemMatcher": []
        },
        {
            "label": "treebeard: run tests isolated",
            "type": "shell",
            "command": "git treebeard branch ${input:branchName} -- npm test",
            "problemMatcher": ["$tsc"]
        }
    ],
    "inputs": [
        {
            "id": "branchName",
            "type": "promptString",
            "description": "Branch name for worktree"
        }
    ]
}
```

### Cursor

Cursor works seamlessly with treebeard's isolated environments:

```bash
# Open Cursor in treebeard worktree
git treebeard branch feature-auth --no-shell
cursor "$(git treebeard path feature-auth)"
```

---

## CI/CD Integration

### GitHub Actions

```yaml
# .github/workflows/parallel-tests.yml
name: Parallel Tests with Worktrees

on: [push, pull_request]

jobs:
  test:
    runs-on: macos-latest
    strategy:
      matrix:
        shard: [1, 2, 3, 4]
    
    steps:
      - uses: actions/checkout@v4
      
      - name: Install macFUSE
        run: brew install --cask macfuse
      
      - name: Install treebeard
        run: cargo install --path .
      
      - name: Create isolated test environment
        run: |
          git treebeard branch test-shard-${{ matrix.shard }} --no-shell
          
      - name: Run tests in isolation
        run: |
          git treebeard branch test-shard-${{ matrix.shard }} -- \
            cargo nextest run --partition count:${{ matrix.shard }}/4
```

### Local parallel testing

```bash
#!/bin/bash
# parallel-test.sh - Run tests in parallel isolated environments

SHARDS=4
PIDS=()

for i in $(seq 1 $SHARDS); do
    git treebeard branch "test-shard-$i" -- \
        cargo nextest run --partition "count:$i/$SHARDS" &
    PIDS+=($!)
done

# Wait for all shards and collect exit codes
EXIT_CODE=0
for pid in "${PIDS[@]}"; do
    wait $pid || EXIT_CODE=1
done

# Cleanup
for i in $(seq 1 $SHARDS); do
    git treebeard cleanup "test-shard-$i" --yes
done

exit $EXIT_CODE
```

---

## Just Task Runner Integration

```just
# justfile

# Create new feature branch with treebeard
feature branch:
    git treebeard branch "feature/{{branch}}"

# Run AI agent in isolated environment
ai branch agent="claude":
    git treebeard branch "{{branch}}" -- {{agent}}

# Parallel AI development
parallel-ai:
    #!/usr/bin/env bash
    for feature in auth api ui; do
        git treebeard branch "feature-$feature" -- claude -p "Implement $feature" &
    done
    wait

# Run tests in isolation
test-isolated branch="test":
    git treebeard branch "{{branch}}" -- cargo nextest run

# Clean up all treebeard worktrees
cleanup-all:
    git treebeard cleanup --all --yes

# Development workflow with tmux
dev branch:
    #!/usr/bin/env bash
    git treebeard branch "{{branch}}" --no-shell
    tmux new-session -d -s "{{branch}}" -c "$(git treebeard path {{branch}})"
    tmux split-window -h -t "{{branch}}"
    tmux send-keys -t "{{branch}}:0.0" "nvim" Enter
    tmux send-keys -t "{{branch}}:0.1" "cargo watch -x test" Enter
    tmux attach -t "{{branch}}"
```

---

## Makefile Integration

```makefile
.PHONY: feature test-isolated ai cleanup

# Create feature branch with treebeard
feature:
	@read -p "Branch name: " branch; \
	git treebeard branch "feature/$$branch"

# Run tests in isolated environment
test-isolated:
	git treebeard branch test-env -- $(MAKE) test

# Start AI coding session
ai:
	git treebeard branch ai-session -- claude

# Parallel AI agents
parallel-ai:
	@for feature in auth api ui; do \
		git treebeard branch "feature-$$feature" -- claude -p "Implement $$feature" & \
	done; \
	wait

# Clean up all worktrees
cleanup:
	git treebeard cleanup --all --yes
```

---

## Shell Aliases and Functions

Add to your `~/.zshrc` or `~/.bashrc`:

```bash
# === Treebeard Aliases ===

# Quick branch creation
alias tb='git treebeard branch'
alias tbl='git treebeard list'
alias tbc='git treebeard cleanup'

# AI coding shortcuts
alias tb-claude='git treebeard branch ai-session -- claude'
alias tb-aider='git treebeard branch ai-session -- aider'

# === Treebeard Functions ===

# Create worktree and cd into it (requires directive file support)
tbd() {
    local branch="$1"
    git treebeard branch "$branch" --no-shell
    cd "$(git treebeard path "$branch")"
}

# Quick feature branch
tbf() {
    git treebeard branch "feature/$1" "${@:2}"
}

# Quick bugfix branch
tbb() {
    git treebeard branch "bugfix/$1" "${@:2}"
}

# Run command in existing worktree without interactive shell
tbr() {
    local branch="$1"
    shift
    git treebeard branch "$branch" -- "$@"
}

# Interactive worktree selection with fzf
tbs() {
    local branch=$(git treebeard list --porcelain 2>/dev/null | \
        cut -f1 | \
        fzf --prompt="Select worktree: ")
    
    if [ -n "$branch" ]; then
        git treebeard branch "$branch"
    fi
}

# Show status of all worktrees
tb-status() {
    git treebeard list --porcelain | while IFS=$'\t' read -r branch path status files; do
        echo "=== $branch ($status) ==="
        if [ -d "$path" ]; then
            git -C "$path" status -sb
        fi
        echo
    done
}

# Clean up merged branches
tb-prune() {
    local main_branch=$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's@^refs/remotes/origin/@@')
    main_branch=${main_branch:-main}
    
    git treebeard list --porcelain | while IFS=$'\t' read -r branch path _; do
        if git merge-base --is-ancestor "$branch" "$main_branch" 2>/dev/null; then
            echo "Cleaning up merged branch: $branch"
            git treebeard cleanup "$branch" --yes --delete-branch
        fi
    done
}
```

---

## Git Hooks Integration

### Pre-commit hook for worktree validation

```bash
#!/bin/bash
# .git/hooks/pre-commit

# Skip if not in treebeard environment
if [ -z "$TREEBEARD_ACTIVE" ]; then
    exit 0
fi

# Run linting in the isolated environment
cargo clippy -- -D warnings || exit 1
cargo fmt --check || exit 1
```

### Post-checkout hook

```bash
#!/bin/bash
# .git/hooks/post-checkout

# If entering a treebeard-managed worktree, run setup
if [ -n "$TREEBEARD_ACTIVE" ]; then
    # Reinstall dependencies if package.json changed
    if git diff --name-only HEAD@{1} HEAD | grep -q "package.json"; then
        npm install
    fi
fi
```

---

## Environment Management with direnv

```bash
# .envrc in project root
# This gets inherited by treebeard worktrees

# Load secrets from 1Password
export DATABASE_URL="op://Development/postgres/url"

# Project-specific settings
export RUST_LOG=debug
export CARGO_TARGET_DIR="${PWD}/target"

# Detect if running in treebeard
if [ -n "$TREEBEARD_ACTIVE" ]; then
    export LOG_PREFIX="[treebeard:$TREEBEARD_BRANCH]"
fi
```

---

## Docker Compose Integration

```yaml
# docker-compose.yml
services:
  app:
    build: .
    volumes:
      # Mount the treebeard worktree if available
      - ${TREEBEARD_MOUNT:-./}:/app
    environment:
      - TREEBEARD_ACTIVE=${TREEBEARD_ACTIVE:-}
      - TREEBEARD_BRANCH=${TREEBEARD_BRANCH:-main}
```

```bash
# Run docker compose in treebeard environment
git treebeard branch feature-docker -- docker compose up
```

---

## Sandboxing with sandbox-exec (Proposed)

When sandbox support is added to treebeard, AI agents can be restricted from accessing sensitive directories:

```bash
# Run Claude with filesystem restrictions (proposed feature)
git treebeard branch feature-auth --sandbox -- claude

# Explicit sandbox configuration
git treebeard branch feature-auth \
    --sandbox \
    --sandbox-deny ~/.ssh \
    --sandbox-deny ~/.aws \
    --sandbox-deny ~/Documents \
    -- claude
```

This prevents AI coding tools from:
- Reading SSH keys or AWS credentials
- Accessing personal documents
- Making network requests (optional)
- Modifying files outside the worktree

---

## Workflow Recipes

### Recipe 1: Safe AI Experimentation

```bash
# Create isolated branch, run AI, review changes, then decide
git treebeard branch experiment-ai -- claude -p "Refactor auth module"

# After AI finishes, review in main repo
git diff main..experiment-ai

# If good, merge; otherwise cleanup
git merge experiment-ai  # or: git treebeard cleanup experiment-ai --delete-branch
```

### Recipe 2: Parallel Feature Development

```bash
#!/bin/bash
# Start multiple features in parallel

features=("user-auth" "api-v2" "new-ui")

for feature in "${features[@]}"; do
    tmux new-window -n "$feature" \
        "git treebeard branch feature/$feature -- $SHELL"
done
```

### Recipe 3: Test Reproduction

```bash
# Reproduce a bug in isolation without affecting your current work
git treebeard branch reproduce-bug-123 -- bash -c '
    git checkout abc123  # Checkout specific commit
    npm install
    npm test            # Reproduce the failure
'
```

### Recipe 4: Code Review

```bash
# Review a PR in isolation
git treebeard branch review-pr-456 -- bash -c '
    gh pr checkout 456
    cargo test
    cargo clippy
'
```

### Recipe 5: Dependency Updates

```bash
# Test dependency updates in isolation
git treebeard branch deps-update -- bash -c '
    npm update
    npm test
    npm audit
'
# If tests pass, sync changes back; otherwise cleanup
```

---

## AI-Generated Commit Messages with llm

Use the `llm` CLI tool to automatically generate commit messages for changes to ignored files in your treebeard session.

### Setup

Install `llm` and configure a model provider:
```bash
pip install llm
# Run 'llm keys' and follow prompts to set API key for OpenAI, Anthropic, etc.
```

### Using the commit_message Hook

Configure treebeard to use `llm` for auto-commit messages by adding a `commit_message` hook:

```toml
# ~/.config/treebeard/config.toml
[hooks]
commit_message = 'echo "{{diff}}" | llm -s "Write a concise commit message for these changes. Output only the raw message with no markdown." --no-stream'
```

When the file watcher detects changes to ignored files, treebeard will:
1. Run the hook, passing the diff via stdin
2. Use the stdout as the commit message
3. Create a git commit using the generated message

### Workflow Example

```bash
# 1. Start a treebeard session
git treebeard branch feature-deps

# 2. Edit files that modify ignored files (e.g., package.json)
vim package.json  # Add 'lodash' dependency

# 3. Run npm install
npm install

# 4. The watcher automatically runs the llm hook and commits:
#    - Hook receives: diff of package.json and package-lock.json
#    - llm generates: "Add lodash for data utility functions"
#    - Commit is created: "treebeard: Add lodash for data utility functions"

# 5. Review the generated commit messages
git log --oneline feature-deps

# 6. Exit treebeard (you'll be prompted to squash or keep)
exit
```

### Customizing the Prompt

You can be very specific about the style of commit message you want:

```toml
# Enforce conventional commits format
[hooks]
commit_message = "\"""\
echo '{{diff}}' | \
llm -s 'Write a commit message in conventional commits format (feat:, fix:, chore:, etc.). \
Analyze the diff and categorize the change correctly. \
Output only the message on a single line.' \
--no-stream\
\""""
```

Or use a pre-written prompt file:

```toml
# ~/.config/treebeard/config.toml
[hooks]
commit_message = 'echo "{{diff}}" | llm -t commit-message-prompt --no-stream'
```

### Without the Hook

If the `commit_message` hook is not defined, treebeard falls back to:
- The configured `auto_commit_message` template (default: "treebeard: auto-save")
- With `{{branch}}` and `{branch}` placeholders expanded

---

## Notes on Proposed Features

Some examples in this document use features that are proposed but not yet implemented:

- `--porcelain` output format for `git treebeard list`
- `git treebeard path <branch>` command
- `--no-shell` mode that mounts but doesn't spawn shell (partially implemented)
- `--sandbox` integration with macOS sandbox-exec

See [FEATURES.md](./FEATURES.md) for the full list of proposed enhancements.
