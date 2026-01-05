pub mod paths;
pub mod persistence;
pub mod schema;

pub use paths::*;
pub use persistence::*;
pub use schema::*;

fn default_worktree_dir() -> String {
    "~/.local/share/treebeard/worktrees".to_string()
}

fn default_mount_dir() -> String {
    "~/.local/share/treebeard/mounts".to_string()
}

fn default_passthrough() -> Vec<String> {
    vec![]
}

fn default_auto_commit_message() -> String {
    "treebeard: auto-save".to_string()
}

fn default_squash_commit_message() -> String {
    "treebeard: {branch}".to_string()
}

fn default_sync_always_skip() -> Vec<String> {
    vec![]
}

fn default_sync_always_include() -> Vec<String> {
    vec![]
}

fn default_fuse_ttl_secs() -> u64 {
    1
}
