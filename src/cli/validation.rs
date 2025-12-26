use crate::cli::Commands;
use crate::error::{Result, TreebeardError};
use std::io::IsTerminal;

pub fn validate_branch_name(branch_name: &str) -> Result<()> {
    if branch_name.is_empty() {
        return Err(TreebeardError::Config(
            "Branch name cannot be empty".to_string(),
        ));
    }

    if branch_name.starts_with('-') {
        return Err(TreebeardError::Config(
            "Branch name cannot start with '-'".to_string(),
        ));
    }

    for byte in branch_name.bytes() {
        if byte < 32 || byte == 127 {
            return Err(TreebeardError::Config(
                "Branch name contains control characters".to_string(),
            ));
        }
    }

    let disallowed_patterns = ["..", "~", "^", ":", "?", "*", "[", "\\", " "];
    for pattern in &disallowed_patterns {
        if branch_name.contains(pattern) {
            return Err(TreebeardError::Config(format!(
                "Branch name cannot contain '{}'",
                pattern
            )));
        }
    }

    if branch_name.starts_with('/') || branch_name.ends_with('/') {
        return Err(TreebeardError::Config(
            "Branch name cannot start or end with '/'".to_string(),
        ));
    }

    if branch_name.contains("//") {
        return Err(TreebeardError::Config(
            "Branch name cannot contain consecutive slashes".to_string(),
        ));
    }

    if branch_name.starts_with('.') || branch_name.ends_with('.') {
        return Err(TreebeardError::Config(
            "Branch name cannot start or end with '.'".to_string(),
        ));
    }

    Ok(())
}

pub fn check_tty_requirement_for_command(command: &Commands) -> Result<()> {
    let is_test_mode = std::env::var("TREEBEARD_TEST_MODE").is_ok();

    match command {
        Commands::Branch { no_shell, .. } => {
            if *no_shell || is_test_mode {
                return Ok(());
            }
            if !std::io::stdin().is_terminal() {
                return Err(TreebeardError::Config(
                    "This command requires an interactive terminal (TTY). \
                     Cannot run with piped input."
                        .to_string(),
                ));
            }
            Ok(())
        }
        Commands::Config { .. }
        | Commands::Doctor
        | Commands::List { .. }
        | Commands::Path { .. }
        | Commands::Cleanup { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_branch_name_empty() {
        assert!(validate_branch_name("").is_err());
    }

    #[test]
    fn test_validate_branch_name_whitespace() {
        assert!(validate_branch_name("   ").is_err());
        assert!(validate_branch_name("test name").is_err());
        assert!(validate_branch_name("\t").is_err());
        assert!(validate_branch_name("\n").is_err());
    }

    #[test]
    fn test_validate_branch_name_leading_hyphen() {
        assert!(validate_branch_name("-test").is_err());
        assert!(validate_branch_name("--test").is_err());
    }

    #[test]
    fn test_validate_branch_name_leading_hyphen_with_valid_name() {
        assert!(validate_branch_name("test-branch").is_ok());
        assert!(validate_branch_name("my-test-branch-name").is_ok());
    }

    #[test]
    fn test_validate_branch_name_path_traversal() {
        assert!(validate_branch_name("..").is_err());
        assert!(validate_branch_name("test..name").is_err());
        assert!(validate_branch_name("test/name/..").is_err());
    }

    #[test]
    fn test_validate_branch_name_leading_dot() {
        assert!(validate_branch_name(".test").is_err());
        assert!(validate_branch_name("..test").is_err());
    }

    #[test]
    fn test_validate_branch_name_trailing_dot() {
        assert!(validate_branch_name("test.").is_err());
        assert!(validate_branch_name("test..").is_err());
    }

    #[test]
    fn test_validate_branch_name_dots_in_middle() {
        assert!(validate_branch_name("test.name").is_ok());
        assert!(validate_branch_name("feature.new-branch").is_ok());
    }

    #[test]
    fn test_validate_branch_name_colon() {
        assert!(validate_branch_name("test:name").is_err());
        assert!(validate_branch_name(":test").is_err());
    }

    #[test]
    fn test_validate_branch_name_special_chars() {
        assert!(validate_branch_name("test^name").is_err());
        assert!(validate_branch_name("test?name").is_err());
        assert!(validate_branch_name("test*name").is_err());
        assert!(validate_branch_name("test[name]").is_err());
        assert!(validate_branch_name("test~name").is_err());
    }

    #[test]
    fn test_validate_branch_name_backslash() {
        assert!(validate_branch_name("test\\name").is_err());
    }

    #[test]
    fn test_validate_branch_name_leading_slash() {
        assert!(validate_branch_name("/test").is_err());
        assert!(validate_branch_name("//test").is_err());
    }

    #[test]
    fn test_validate_branch_name_trailing_slash() {
        assert!(validate_branch_name("test/").is_err());
        assert!(validate_branch_name("test//").is_err());
    }

    #[test]
    fn test_validate_branch_name_consecutive_slashes() {
        assert!(validate_branch_name("test//name").is_err());
        assert!(validate_branch_name("test///name").is_err());
    }

    #[test]
    fn test_validate_branch_name_slashes_in_middle() {
        assert!(validate_branch_name("feature/test").is_ok());
    }

    #[test]
    fn test_validate_branch_name_control_characters() {
        let control_chars = [
            "\x01", "\x02", "\x03", "\x04", "\x05", "\x06", "\x07", "\x08", "\x09", "\x0A", "\x0B",
            "\x0C", "\x0D", "\x0E", "\x0F", "\x10", "\x11", "\x12", "\x13", "\x14", "\x15", "\x16",
            "\x17", "\x18", "\x19", "\x1A", "\x1B", "\x1C", "\x1D", "\x1E", "\x1F", "\x7F",
        ];

        for c in control_chars {
            let name = format!("test{}branch", c);
            assert!(
                validate_branch_name(&name).is_err(),
                "Branch name with control character should be rejected: {:?}",
                name
            );
        }

        let name_with_null = "test\x00branch";
        assert!(
            validate_branch_name(name_with_null).is_err(),
            "Branch name with null character should be rejected"
        );
    }

    #[test]
    fn test_validate_branch_name_simple_valid_names() {
        assert!(validate_branch_name("main").is_ok());
        assert!(validate_branch_name("develop").is_ok());
        assert!(validate_branch_name("test").is_ok());
    }

    #[test]
    fn test_validate_branch_name_hyphens() {
        assert!(validate_branch_name("test-branch").is_ok());
        assert!(validate_branch_name("my-long-branch-name").is_ok());
        assert!(validate_branch_name("feature-123").is_ok());
    }

    #[test]
    fn test_validate_branch_name_slashes() {
        assert!(validate_branch_name("feature/new-feature").is_ok());
        assert!(validate_branch_name("bugfix/critical-issue").is_ok());
        assert!(validate_branch_name("release/v1.0.0").is_ok());
    }

    #[test]
    fn test_validate_branch_name_dots() {
        assert!(validate_branch_name("fix.v2").is_ok());
        assert!(validate_branch_name("release.1.0").is_ok());
    }

    #[test]
    fn test_validate_branch_name_numbers() {
        assert!(validate_branch_name("123").is_ok());
        assert!(validate_branch_name("branch-123").is_ok());
        assert!(validate_branch_name("v1.0.0").is_ok());
    }

    #[test]
    fn test_validate_branch_name_underscores() {
        assert!(validate_branch_name("test_branch").is_ok());
        assert!(validate_branch_name("feature_new").is_ok());
        assert!(validate_branch_name("my_test_branch").is_ok());
    }

    #[test]
    fn test_validate_branch_name_complex_valid() {
        assert!(validate_branch_name("feature/new-auth-flow").is_ok());
        assert!(validate_branch_name("bugfix/issue-123.v2").is_ok());
        assert!(validate_branch_name("release/v1.2.3-rc1").is_ok());
    }
}
