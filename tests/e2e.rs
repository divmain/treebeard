//! End-to-end tests for treebeard.
//!
//! These tests verify full user-facing behavior through the CLI.

mod shared;

mod e2e {
    pub mod edge_cases;
    pub mod happy_path;
    pub mod infrastructure;
}
