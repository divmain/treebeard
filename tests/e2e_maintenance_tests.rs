mod common;
mod e2e_helpers;

use common::{
    cleanup_all_test_mounts, count_treebeard_mounts, get_treebeard_mount_paths, TestWorkspace,
};
use e2e_helpers::{spawn_treebeard_test_mode, terminate_treebeard};
use std::thread;
use std::time::Duration;

#[test]
fn test_terminate_cleanup_unmounts_fuse() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "cleanup-verify-terminate";
    let mount_path = workspace.get_mount_path(branch_name);

    let mounts_before = count_treebeard_mounts();

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    thread::sleep(Duration::from_millis(200));
    let mounts_during = count_treebeard_mounts();
    assert!(
        mounts_during > mounts_before,
        "Mount should be created during test. Before: {}, During: {}",
        mounts_before,
        mounts_during
    );

    terminate_treebeard(treebeard);

    thread::sleep(Duration::from_millis(300));

    workspace.restore_dir();

    assert!(
        workspace.verify_mount_cleaned_up(branch_name),
        "Mount should be cleaned up after terminate_treebeard. Mount path: {}",
        mount_path.display()
    );
}

#[test]
fn test_workspace_drop_cleans_stale_mounts() {
    let mounts_before = count_treebeard_mounts();

    {
        let workspace = TestWorkspace::new();
        workspace.switch_to_repo();

        let branch_name = "cleanup-verify-drop";

        let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

        thread::sleep(Duration::from_millis(300));

        let _ = treebeard.kill();
        let _ = treebeard.wait();
        std::env::remove_var("TREEBEARD_TEST_MODE");
    }

    thread::sleep(Duration::from_millis(300));

    let mounts_after = count_treebeard_mounts();
    assert!(
        mounts_after <= mounts_before,
        "Mounts should not increase after workspace drop. Before: {}, After: {}",
        mounts_before,
        mounts_after
    );
}

#[test]
fn test_zz_final_stale_mount_cleanup() {
    let mount_paths = get_treebeard_mount_paths();
    let test_mounts: Vec<_> = mount_paths
        .iter()
        .filter(|p| {
            p.contains("/var/folders/")
                || p.contains("/tmp/")
                || p.contains("/private/var/folders/")
        })
        .collect();

    let cleaned = cleanup_all_test_mounts();

    if !test_mounts.is_empty() {
        eprintln!(
            "\n*** WARNING: Found {} stale test mount(s), cleaned up {} ***",
            test_mounts.len(),
            cleaned
        );
        for path in &test_mounts {
            eprintln!("  - {}", path);
        }
        eprintln!();
    }
}
