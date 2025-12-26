#![cfg(target_os = "macos")]

mod common;
mod fuse_common;

use fuse_common::FuseTestSession;
use std::fs;

/// Lookup finds all lower layer files
#[test]
fn test_lookup_finds_all_lower_layer_files() {
    let Some(session) = FuseTestSession::with_lower_layer_setup("lookup-all-files", |lower| {
        let tracked_file1 = lower.join("example.txt");
        fs::write(&tracked_file1, "tracked file content").unwrap();

        let tracked_file2 = lower.join("src");
        fs::create_dir(&tracked_file2).unwrap();
        let tracked_file3 = lower.join("src/main.rs");
        fs::write(&tracked_file3, "fn main() {}").unwrap();

        let gitignore = lower.join(".gitignore");
        fs::write(&gitignore, "*.log\n.env\n").unwrap();

        let ignored_file = lower.join(".env");
        fs::write(&ignored_file, "SECRET=value").unwrap();
    }) else {
        return;
    };

    let mounted_example = session.mountpoint.join("example.txt");
    match fs::read_to_string(&mounted_example) {
        Ok(content) => {
            assert_eq!(content, "tracked file content");
            eprintln!("✓ Can access tracked file 'example.txt' from lower layer");
        }
        Err(e) => {
            panic!(
                "REGRESSION: Cannot access tracked file from lower layer: {}",
                e
            );
        }
    }

    let mounted_gitignore = session.mountpoint.join(".gitignore");
    match fs::read_to_string(&mounted_gitignore) {
        Ok(content) => {
            assert!(content.contains("*.log"));
            eprintln!("✓ Can access '.gitignore' from lower layer");
        }
        Err(e) => {
            panic!(
                "REGRESSION: Cannot access .gitignore from lower layer: {}",
                e
            );
        }
    }

    let mounted_src = session.mountpoint.join("src");
    assert!(
        mounted_src.exists(),
        "REGRESSION: 'src' directory should be accessible"
    );
    eprintln!("✓ Can access 'src' directory from lower layer");

    let mounted_main = session.mountpoint.join("src/main.rs");
    match fs::read_to_string(&mounted_main) {
        Ok(content) => {
            assert_eq!(content, "fn main() {}");
            eprintln!("✓ Can access 'src/main.rs' from lower layer");
        }
        Err(e) => {
            panic!(
                "REGRESSION: Cannot access nested file from lower layer: {}",
                e
            );
        }
    }

    let mounted_env = session.mountpoint.join(".env");
    match fs::read_to_string(&mounted_env) {
        Ok(content) => {
            assert_eq!(content, "SECRET=value");
            eprintln!("✓ Can access ignored file '.env' from lower layer");
        }
        Err(e) => {
            panic!(
                "REGRESSION: Cannot access ignored file from lower layer: {}",
                e
            );
        }
    }

    drop(session.handle);
    eprintln!("✓ All file lookups work correctly");
}

/// Full overlay behavior test
#[test]
fn test_combined_overlay_semantics() {
    let Some(session) = FuseTestSession::with_lower_layer_setup("combined-overlay", |lower| {
        fs::write(lower.join("README.md"), "# Project\n").unwrap();
        fs::write(lower.join(".gitignore"), "*.log\nnode_modules/\n.env\n").unwrap();
        fs::create_dir(lower.join("src")).unwrap();
        fs::write(
            lower.join("src/main.rs"),
            "fn main() { println!(\"Hello\"); }",
        )
        .unwrap();
        fs::write(lower.join("src/lib.rs"), "pub fn lib() {}").unwrap();
        fs::create_dir(lower.join("tests")).unwrap();
        fs::write(lower.join("tests/test.rs"), "#[test] fn test() {}").unwrap();

        fs::write(lower.join(".env"), "DATABASE_URL=postgres://...").unwrap();
    }) else {
        return;
    };

    let root_entries: Vec<String> = fs::read_dir(&session.mountpoint)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    eprintln!("Root directory entries: {:?}", root_entries);

    assert!(
        root_entries.contains(&"README.md".to_string()),
        "README.md should be visible"
    );
    assert!(
        root_entries.contains(&".gitignore".to_string()),
        ".gitignore should be visible"
    );
    assert!(
        root_entries.contains(&"src".to_string()),
        "src directory should be visible"
    );
    assert!(
        root_entries.contains(&"tests".to_string()),
        "tests directory should be visible"
    );
    assert!(
        root_entries.contains(&".env".to_string()),
        ".env should be visible"
    );
    eprintln!("✓ All root files visible in readdir");

    let src_entries: Vec<String> = fs::read_dir(session.mountpoint.join("src"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert!(src_entries.contains(&"main.rs".to_string()));
    assert!(src_entries.contains(&"lib.rs".to_string()));
    eprintln!("✓ Nested directory contents visible in readdir");

    let readme_content = fs::read_to_string(session.mountpoint.join("README.md")).unwrap();
    assert_eq!(readme_content, "# Project\n");
    eprintln!("✓ Can lookup and read README.md");

    let main_content = fs::read_to_string(session.mountpoint.join("src/main.rs")).unwrap();
    assert!(main_content.contains("fn main()"));
    eprintln!("✓ Can lookup and read src/main.rs");

    let env_content = fs::read_to_string(session.mountpoint.join(".env")).unwrap();
    assert!(env_content.contains("DATABASE_URL"));
    eprintln!("✓ Can lookup and read .env (ignored file)");

    let readme_path = session.mountpoint.join("README.md");
    fs::write(&readme_path, "# Modified Project\n").unwrap();
    eprintln!("✓ Modified README.md through overlay");

    let upper_readme = session.upper_layer.join("README.md");
    assert!(
        upper_readme.exists(),
        "Modified file should be copied to upper layer"
    );
    let upper_content = fs::read_to_string(&upper_readme).unwrap();
    assert_eq!(upper_content, "# Modified Project\n");
    eprintln!("✓ COW: Modified file copied to upper layer");

    let lower_readme = session.lower_layer.join("README.md");
    let lower_content = fs::read_to_string(&lower_readme).unwrap();
    assert_eq!(lower_content, "# Project\n");
    eprintln!("✓ COW: Lower layer file preserved");

    fs::remove_file(session.mountpoint.join("src/lib.rs")).unwrap();
    eprintln!("✓ Deleted src/lib.rs through overlay");

    let lib_rs = session.mountpoint.join("src/lib.rs");
    assert!(!lib_rs.exists(), "Deleted file should not be accessible");

    let src_entries_after: Vec<String> = fs::read_dir(session.mountpoint.join("src"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert!(!src_entries_after.contains(&"lib.rs".to_string()));
    eprintln!("✓ Whiteout: Deleted file not visible in readdir");

    assert!(session.lower_layer.join("src/lib.rs").exists());
    eprintln!("✓ Whiteout: Lower layer file preserved");

    drop(session.handle);
    eprintln!("✓ Combined overlay semantics test completed successfully");
}
