use glob::Pattern;
use std::path::Path;

pub struct CompiledPatterns {
    patterns: Vec<Pattern>,
}

impl CompiledPatterns {
    pub fn new(pattern_strings: &[String]) -> Self {
        let patterns: Vec<Pattern> = pattern_strings
            .iter()
            .filter_map(|s| Pattern::new(s).ok())
            .collect();

        CompiledPatterns { patterns }
    }

    pub fn matches(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        self.patterns
            .iter()
            .any(|pattern| pattern.matches(&path_str))
    }
}

#[allow(dead_code)]
pub fn path_matches_pattern(path: &Path, patterns: &[String]) -> bool {
    CompiledPatterns::new(patterns).matches(path)
}

pub fn detect_binary(content: &[u8]) -> bool {
    if content.is_empty() {
        return false;
    }
    let check_size = content.len().min(8192);
    content[..check_size].contains(&0)
}

pub fn should_skip_diff(repo_file: &Path, worktree_file: &Path) -> bool {
    use std::fs;
    let worktree_size = match fs::metadata(worktree_file) {
        Ok(m) => m.len(),
        Err(_) => return false,
    };

    let repo_size = if repo_file.exists() {
        match fs::metadata(repo_file) {
            Ok(m) => m.len(),
            Err(_) => return false,
        }
    } else {
        0
    };

    worktree_size > super::MAX_DIFF_FILE_SIZE || repo_size > super::MAX_DIFF_FILE_SIZE
}

pub fn both_files_exist(repo_file: &Path, worktree_file: &Path) -> bool {
    repo_file.exists() && worktree_file.exists()
}
