//! Integration tests for treebeard modules.
//!
//! These tests verify direct integration between modules, not end-to-end behavior.

mod shared;

mod integration {
    pub mod components;
    pub mod config;
    pub mod fuse;
    pub mod git;
}
