//! Edge case tests for git interactions.
//!
//! Note: The git check-ignore failure path is tested via unit tests in
//! src/sync/aggregation.rs since simulating git failures in e2e tests
//! would require modifying the treebeard process's PATH, which is complex.
//!
//! These tests verify that the normal git interaction flow works correctly.
