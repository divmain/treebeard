//! Edge case tests for special filenames.

use crate::shared::common::{git_commit_count, TestWorkspace};
use crate::shared::e2e_helpers::{send_signal, spawn_treebeard_test_mode, terminate_treebeard};
use nix::sys::signal;
use proptest::prelude::*;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn test_ec_unicode_characters() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ec-unicode-test";
    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    let unicode_files = [
        ("файл_русский.txt", "русский текст"),
        ("文件中文.txt", "中文内容"),
        ("ファイル日本語.txt", "日本語の内容"),
        ("ملف_عربي.txt", "محتوى عربي"),
        ("tête_à_tête.txt", "Français"),
        ("café_éspresso.txt", "Español"),
        ("über_cool.txt", "Deutsch"),
    ];

    for (filename, content) in unicode_files.iter() {
        let test_file = mount_dir.join(filename);
        fs::write(&test_file, content).expect("Failed to write unicode file");
        thread::sleep(Duration::from_millis(100));
    }

    thread::sleep(Duration::from_millis(700));
    send_signal(&treebeard, signal::Signal::SIGINT);
    let _ = treebeard.wait();
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let commit_count = git_commit_count(&workspace.repo_path, branch_name);
    assert!(
        commit_count >= 1,
        "Expected at least 1 commit for unicode files, got {}",
        commit_count
    );

    for (filename, content) in unicode_files.iter() {
        let output = Command::new("git")
            .args(["show", &format!("{}:{}", branch_name, filename)])
            .current_dir(&workspace.repo_path)
            .output()
            .expect("Failed to read file from git");

        assert!(
            output.status.success(),
            "Unicode file '{}' should exist in git history. stderr: {}",
            filename,
            String::from_utf8_lossy(&output.stderr)
        );

        let actual_content = String::from_utf8_lossy(&output.stdout);
        assert!(
            actual_content.contains(content),
            "Content mismatch for '{}'. Expected '{}' in '{}'",
            filename,
            content,
            actual_content
        );
    }
}

#[test]
fn test_ec_emoticons_and_special_symbols() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ec-emoticons-test";
    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    let special_files = [
        ("file_🚀.txt", "rocket emoji"),
        ("test_💯.txt", "100 emoji"),
        ("data_⚡.txt", "lightning"),
        ("report_📊.txt", "chart"),
    ];

    for (filename, content) in special_files.iter() {
        let test_file = mount_dir.join(filename);
        fs::write(&test_file, content).expect("Failed to write emoticon file");
        thread::sleep(Duration::from_millis(100));
    }

    thread::sleep(Duration::from_millis(700));
    send_signal(&treebeard, signal::Signal::SIGINT);
    let _ = treebeard.wait();
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    for (filename, content) in special_files.iter() {
        let output = Command::new("git")
            .args(["show", &format!("{}:{}", branch_name, filename)])
            .current_dir(&workspace.repo_path)
            .output()
            .expect("Failed to read file from git");

        assert!(
            output.status.success(),
            "Emoticon file '{}' should exist in git history. stderr: {}",
            filename,
            String::from_utf8_lossy(&output.stderr)
        );

        let actual_content = String::from_utf8_lossy(&output.stdout);
        assert!(
            actual_content.contains(content),
            "Content mismatch for '{}'. Expected '{}' in '{}'",
            filename,
            content,
            actual_content
        );
    }
}

#[test]
fn test_ec_deep_directory_nesting() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ec-deep-nesting";
    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    let test_paths = [
        "level1/file.txt",
        "level1/level2/file.txt",
        "level1/level2/level3/file.txt",
        "level1/level2/level3/level4/file.txt",
        "level1/level2/level3/level4/level5/file.txt",
        "a/b/c/d/e/f/g/file.txt",
    ];

    for test_path in test_paths.iter() {
        let file_path = mount_dir.join(test_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create nested directories");
        }
        fs::write(&file_path, format!("content at {}", test_path)).expect("Failed to write file");
        thread::sleep(Duration::from_millis(50));
    }

    thread::sleep(Duration::from_millis(700));
    send_signal(&treebeard, signal::Signal::SIGINT);
    let _ = treebeard.wait();
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let commit_count = git_commit_count(&workspace.repo_path, branch_name);
    assert!(
        commit_count >= 1,
        "Expected at least 1 commit for deeply nested files, got {}",
        commit_count
    );

    let output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list files in branch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for test_path in test_paths.iter() {
        assert!(
            stdout.contains(test_path),
            "Nested file '{}' should exist in git history. Files: {}",
            test_path,
            stdout
        );
    }
}

#[test]
fn test_ec_mixed_extensions() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ec-mixed-extensions";
    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    let extensions = [
        "txt", "md", "rs", "py", "js", "json", "toml", "yaml", "csv", "log", "dat", "bin",
    ];
    let mut created_files = Vec::new();

    for (i, ext) in extensions.iter().enumerate() {
        let filename = format!("test_file_{}.{}", i, ext);
        let file_path = mount_dir.join(&filename);
        fs::write(&file_path, format!("content for {}", ext)).expect("Failed to write file");
        created_files.push(filename);
        thread::sleep(Duration::from_millis(50));
    }

    thread::sleep(Duration::from_millis(700));
    send_signal(&treebeard, signal::Signal::SIGINT);
    let _ = treebeard.wait();
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list files in branch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for filename in created_files {
        assert!(
            stdout.contains(&filename),
            "File '{}' with extension should exist in git history. Files: {}",
            filename,
            stdout
        );
    }
}

#[test]
fn test_ec_long_filename() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ec-long-filename";
    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    let long_name = format!(
        "very_long_filename_with_many_characters_{}_end.txt",
        "x".repeat(200)
    );
    let file_path = mount_dir.join(&long_name);

    fs::write(&file_path, "content for long filename").expect("Failed to write file");
    thread::sleep(Duration::from_millis(700));

    send_signal(&treebeard, signal::Signal::SIGINT);
    let _ = treebeard.wait();
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list files in branch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&long_name),
        "Long filename '{}' should exist in git history. Files: {}",
        long_name,
        stdout
    );
}

#[test]
fn test_ec_concurrent_rapid_modifications() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ec-concurrent-mods";
    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    let file_path = mount_dir.join("rapidly_changing_file.txt");

    for i in 0..10 {
        fs::write(&file_path, format!("iteration {}", i)).expect("Failed to write file");
        thread::sleep(Duration::from_millis(30));
    }

    thread::sleep(Duration::from_millis(700));
    send_signal(&treebeard, signal::Signal::SIGINT);
    let _ = treebeard.wait();
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let output = Command::new("git")
        .args([
            "show",
            &format!("{}:rapidly_changing_file.txt", branch_name),
        ])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to read file from git");

    assert!(output.status.success(), "File should exist in branch");
}

#[test]
fn test_dot_files() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ec-dot-files";
    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    let dot_files = [
        ".env.local",
        ".gitignore.custom",
        ".hidden.txt",
        ".config/settings.json",
        ".bashrc.custom",
    ];

    for dot_file in dot_files.iter() {
        let file_path = mount_dir.join(dot_file);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create directories");
        }
        fs::write(&file_path, format!("content of {}", dot_file)).expect("Failed to write file");
        thread::sleep(Duration::from_millis(100));
    }

    thread::sleep(Duration::from_millis(700));
    send_signal(&treebeard, signal::Signal::SIGINT);
    let _ = treebeard.wait();
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list files in branch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for dot_file in dot_files.iter() {
        assert!(
            stdout.contains(dot_file),
            "Dot file '{}' should exist in git history. Files: {}",
            dot_file,
            stdout
        );
    }
}

#[test]
fn test_ec_similar_filenames() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ec-similar-names";
    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    let similar_files = ["file.txt", "file1.txt", "file_1.txt", "file.csv"];

    for filename in similar_files.iter() {
        let file_path = mount_dir.join(filename);
        fs::write(&file_path, format!("content of {}", filename)).expect("Failed to write file");
        thread::sleep(Duration::from_millis(100));
    }

    thread::sleep(Duration::from_millis(700));
    send_signal(&treebeard, signal::Signal::SIGINT);
    let _ = treebeard.wait();
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list files in branch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for filename in similar_files.iter() {
        assert!(
            stdout.contains(filename),
            "Similar file '{}' should exist in git history. Files: {}",
            filename,
            stdout
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]
    #[test]
    fn test_ec_special_char_filenames(filename in "[a-zA-Z0-9 _-]{1,50}") {
        let workspace = TestWorkspace::new();
        workspace.switch_to_repo();

        let branch_name = format!("ec-special-char-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
        let treebeard = spawn_treebeard_test_mode(&branch_name, &workspace.repo_path);

        let mount_dir = workspace.get_mount_path(&branch_name);
        let test_file = mount_dir.join(&filename);

        fs::write(&test_file, "test content").expect("Failed to write file");
        thread::sleep(Duration::from_millis(700));

        terminate_treebeard(treebeard);
        workspace.restore_dir();

        let commit_count = git_commit_count(&workspace.repo_path, &branch_name);
        assert!(
            commit_count >= 1,
            "Expected at least 1 commit for filename '{}', got {}",
            filename, commit_count
        );

        let output = Command::new("git")
            .args(["ls-tree", "-r", &branch_name, "--name-only"])
            .current_dir(&workspace.repo_path)
            .output()
            .expect("Failed to list files in branch");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(&filename),
            "File '{}' should exist in git history. Files listed: {}",
            filename, stdout
        );
    }

    #[test]
    fn test_ec_rapid_successive_changes(file_count in 1..15usize) {
        let workspace = TestWorkspace::new();
        workspace.switch_to_repo();

        let branch_name = format!("ec-rapid-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
        let mut treebeard = spawn_treebeard_test_mode(&branch_name, &workspace.repo_path);

        let mount_dir = workspace.get_mount_path(&branch_name);

        for i in 0..file_count {
            fs::write(mount_dir.join(format!("rapid_{}.txt", i)), format!("content {}", i))
                .expect("Failed to write file");
            thread::sleep(Duration::from_millis(50));
        }

        thread::sleep(Duration::from_millis(700));
        send_signal(&treebeard, signal::Signal::SIGINT);
        let _ = treebeard.wait();
        std::env::remove_var("TREEBEARD_TEST_MODE");
        workspace.restore_dir();

        let commit_count = git_commit_count(&workspace.repo_path, &branch_name);
        assert!(
            commit_count >= 1,
            "Expected at least 1 commit after {} rapid changes, got {}",
            file_count, commit_count
        );

        let output = Command::new("git")
            .args(["ls-tree", "-r", &branch_name, "--name-only"])
            .current_dir(&workspace.repo_path)
            .output()
            .expect("Failed to list files in branch");

        let stdout = String::from_utf8_lossy(&output.stdout);
        for i in 0..file_count {
            assert!(
                stdout.contains(&format!("rapid_{}.txt", i)),
                "File rapid_{}.txt should exist. Files: {}",
                i, stdout
            );
        }
    }

    #[test]
    fn test_ec_filename_with_spaces_and_underscores(filename in "[a-zA-Z]{1,10}( |_)[a-zA-Z0-9]{1,10}( |_)[a-zA-Z0-9]{1,10}") {
        let workspace = TestWorkspace::new();
        workspace.switch_to_repo();

        let branch_name = format!("ec-spaces-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
        let treebeard = spawn_treebeard_test_mode(&branch_name, &workspace.repo_path);

        let mount_dir = workspace.get_mount_path(&branch_name);
        let test_file = mount_dir.join(&filename);

        fs::write(&test_file, "content with spaces").expect("Failed to write file");
        thread::sleep(Duration::from_millis(700));

        terminate_treebeard(treebeard);
        workspace.restore_dir();

        let commit_count = git_commit_count(&workspace.repo_path, &branch_name);
        assert!(
            commit_count >= 1,
            "Expected at least 1 commit for filename with spaces/underscores, got {}",
            commit_count
        );
    }
}
