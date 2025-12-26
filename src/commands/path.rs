use crate::cli::validate_branch_name;
use crate::config::{get_mount_dir, get_worktree_dir};
use crate::error::Result;
use crate::git::GitRepo;

pub fn print_path(branch_name: &str, worktree: bool) -> Result<()> {
    validate_branch_name(branch_name)?;
    let repo = GitRepo::discover()?;

    let path = if worktree {
        let worktree_base_dir = get_worktree_dir()?;
        let repo_name = repo.repo_name();
        worktree_base_dir.join(repo_name).join(branch_name)
    } else {
        let mount_base_dir = get_mount_dir()?;
        let repo_name = repo.repo_name();
        mount_base_dir.join(repo_name).join(branch_name)
    };

    println!("{}", path.display());
    Ok(())
}
