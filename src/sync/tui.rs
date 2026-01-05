use crate::config::SyncConfig;
use crate::error::{Result, TreebeardError};
use crate::sync::config::save_skip_pattern;
use crate::sync::display::PreviewResult;
use crate::sync::files::CompiledPatterns;
use crate::sync::types::{ChangeItem, DirectoryChange, FileChange, SyncResult};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct SyncUI {
    raw_mode_enabled: bool,
    cancelled: Arc<AtomicBool>,
}

impl Default for SyncUI {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncUI {
    pub fn new() -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        Self {
            raw_mode_enabled: false,
            cancelled,
        }
    }

    pub fn enter_raw_mode(&mut self) -> Result<()> {
        if !self.raw_mode_enabled {
            enable_raw_mode()
                .map_err(|e| TreebeardError::Config(format!("Failed to enable raw mode: {}", e)))?;
            self.raw_mode_enabled = true;
        }
        Ok(())
    }

    pub fn exit_raw_mode(&mut self) {
        if self.raw_mode_enabled {
            let _ = disable_raw_mode();
            self.raw_mode_enabled = false;
        }
    }

    pub fn poll_input(&self, timeout: Duration) -> Result<Option<Event>> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }

        if event::poll(timeout)
            .map_err(|e| TreebeardError::Config(format!("Failed to poll for events: {}", e)))?
        {
            let ev = event::read()
                .map_err(|e| TreebeardError::Config(format!("Failed to read event: {}", e)))?;
            if let Event::Key(key) = ev {
                if key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Char('c')
                    && key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL)
                {
                    self.cancelled.store(true, Ordering::Relaxed);
                    return Ok(Some(ev));
                }
            }
            return Ok(Some(ev));
        }
        Ok(None)
    }
}

impl Drop for SyncUI {
    fn drop(&mut self) {
        self.exit_raw_mode();
    }
}

pub fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        original_hook(panic_info);
    }));
}

enum SelectionAction {
    CursorUp,
    CursorDown,
    ToggleSelection,
    ToggleSelectAll,
    Done,
    ViewDetails,
    ViewDirectoryFiles,
    Quit,
    Cancelled,
    None,
}

fn handle_key_input(key: crossterm::event::KeyEvent) -> SelectionAction {
    if key.kind != KeyEventKind::Press {
        return SelectionAction::None;
    }
    match key.code {
        KeyCode::Up => SelectionAction::CursorUp,
        KeyCode::Down => SelectionAction::CursorDown,
        KeyCode::Char(' ') => SelectionAction::ToggleSelection,
        KeyCode::Char('a') => SelectionAction::ToggleSelectAll,
        KeyCode::Char('d') => SelectionAction::Done,
        KeyCode::Enter => SelectionAction::ViewDetails,
        KeyCode::Char('n') => SelectionAction::ViewDirectoryFiles,
        KeyCode::Char('q') => SelectionAction::Quit,
        _ => SelectionAction::None,
    }
}

enum ActionResult {
    Continue,
    Exit(SyncResult),
}

fn execute_selection_action(
    action: SelectionAction,
    items: &[ChangeItem],
    cursor: &mut usize,
    selected: &mut HashSet<usize>,
    repo_path: &Path,
    worktree_path: &Path,
    ui: &mut SyncUI,
) -> Result<ActionResult> {
    use crate::sync::ops::sync_selected;

    match action {
        SelectionAction::CursorUp => {
            *cursor = cursor.saturating_sub(1);
        }
        SelectionAction::CursorDown => {
            if *cursor < items.len().saturating_sub(1) {
                *cursor += 1;
            }
        }
        SelectionAction::ToggleSelection => {
            if selected.contains(cursor) {
                selected.remove(cursor);
            } else {
                selected.insert(*cursor);
            }
        }
        SelectionAction::ToggleSelectAll => {
            if selected.len() == items.len() {
                selected.clear();
            } else {
                for i in 0..items.len() {
                    selected.insert(i);
                }
            }
        }
        SelectionAction::Done => {
            ui.exit_raw_mode();
            return Ok(ActionResult::Exit(sync_selected(
                items,
                selected,
                repo_path,
                worktree_path,
            )?));
        }
        SelectionAction::ViewDetails => {
            ui.exit_raw_mode();
            let should_sync = match &items[*cursor] {
                ChangeItem::File(file) => display_file_diff(file, repo_path, worktree_path)?,
                ChangeItem::Directory(dir) => {
                    display_directory_summary(dir, repo_path, worktree_path, ui)?
                }
            };
            if should_sync {
                selected.insert(*cursor);
            }
            ui.enter_raw_mode()?;
        }
        SelectionAction::ViewDirectoryFiles => {
            if let ChangeItem::Directory(dir) = &items[*cursor] {
                if dir.files.len() <= super::MAX_VIEWABLE_FILES {
                    ui.exit_raw_mode();
                    match run_directory_file_selection(dir, repo_path, worktree_path, ui) {
                        Ok(SyncResult::Synced(count)) => {
                            ui.enter_raw_mode()?;
                            println!("\n✓ Marked {} files for sync", count);
                            std::thread::sleep(Duration::from_millis(500));
                        }
                        Ok(SyncResult::Cancelled) => {
                            ui.exit_raw_mode();
                            println!("\nInterrupted. No files synced.");
                            return Ok(ActionResult::Exit(SyncResult::Cancelled));
                        }
                        Ok(SyncResult::Partial(progress)) => {
                            ui.enter_raw_mode()?;
                            println!(
                                "\n✓ Partially synced: {} succeeded, {} failed",
                                progress.synced_files.len(),
                                progress.failed_files.len()
                            );
                            std::thread::sleep(Duration::from_millis(500));
                        }
                        _ => {
                            ui.enter_raw_mode()?;
                        }
                    }
                }
            }
        }
        SelectionAction::Quit => {
            ui.exit_raw_mode();
            println!("\nNo files synced.");
            return Ok(ActionResult::Exit(SyncResult::Skipped));
        }
        SelectionAction::Cancelled => {
            ui.exit_raw_mode();
            println!("\nInterrupted. No files synced.");
            return Ok(ActionResult::Exit(SyncResult::Cancelled));
        }
        SelectionAction::None => {}
    }
    Ok(ActionResult::Continue)
}

pub fn run_interactive_selection(
    items: &[ChangeItem],
    repo_path: &Path,
    worktree_path: &Path,
    sync_config: &SyncConfig,
) -> Result<SyncResult> {
    let mut ui = SyncUI::new();
    ui.enter_raw_mode()?;

    let include_patterns = CompiledPatterns::new(&sync_config.get_sync_always_include());
    let mut selected: HashSet<usize> = HashSet::new();
    for (idx, item) in items.iter().enumerate() {
        let should_preselect = match item {
            ChangeItem::File(file) => include_patterns.matches(&file.path),
            ChangeItem::Directory(dir) => include_patterns.matches(&dir.path),
        };
        if should_preselect {
            selected.insert(idx);
        }
    }

    let mut cursor = 0;

    loop {
        display_selection_menu(items, &selected, cursor);

        loop {
            if let Some(ev) = ui.poll_input(Duration::from_millis(100))? {
                if ui.cancelled.load(Ordering::Relaxed) {
                    let result = execute_selection_action(
                        SelectionAction::Cancelled,
                        items,
                        &mut cursor,
                        &mut selected,
                        repo_path,
                        worktree_path,
                        &mut ui,
                    )?;
                    if let ActionResult::Exit(sync_result) = result {
                        return Ok(sync_result);
                    }
                }

                if let Event::Key(key) = ev {
                    let action = handle_key_input(key);
                    let result = execute_selection_action(
                        action,
                        items,
                        &mut cursor,
                        &mut selected,
                        repo_path,
                        worktree_path,
                        &mut ui,
                    )?;
                    if let ActionResult::Exit(sync_result) = result {
                        return Ok(sync_result);
                    }
                }
                break;
            }
        }
    }
}

fn display_selection_menu(items: &[ChangeItem], selected: &HashSet<usize>, cursor: usize) {
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0)
    );

    println!("Select items to sync:\n");

    for (idx, item) in items.iter().enumerate() {
        let is_cursor = idx == cursor;
        let is_selected = selected.contains(&idx);
        let marker = if is_cursor { ">" } else { " " };
        let checkbox = if is_selected { "[x]" } else { "[ ]" };

        match item {
            ChangeItem::File(file) => {
                println!("  {} {} {:?}", marker, checkbox, file.path.display());
            }
            ChangeItem::Directory(dir) => {
                println!(
                    "  {} {} {:?} ({} files)",
                    marker,
                    checkbox,
                    dir.path.display(),
                    dir.files.len()
                );
            }
        }
    }

    println!(
        "\n[↑↓] navigate  [space] toggle  [enter] view diff  [a] select all  [d] done  [q] quit"
    );
    if items.len() == 1
        || (!items.is_empty()
            && matches!(&items[cursor], ChangeItem::Directory(d) if d.files.len() <= 50))
    {
        println!("[n] view individual files");
    }
}

fn display_file_diff(file: &FileChange, repo_path: &Path, worktree_path: &Path) -> Result<bool> {
    use crate::sync::display::show_file_preview;
    let worktree_file = worktree_path.join(&file.path);
    let repo_file = repo_path.join(&file.path);

    if let PreviewResult::Skipped = show_file_preview(file, &repo_file, &worktree_file)? {
        return Ok(false);
    }

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| TreebeardError::Config(format!("Failed to read input: {}", e)))?;
    match input.trim().to_lowercase().as_str() {
        "y" => {
            println!("\n✓ Marked {} for sync", file.path.display());
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn display_directory_summary(
    dir: &DirectoryChange,
    repo_path: &Path,
    worktree_path: &Path,
    ui: &mut SyncUI,
) -> Result<bool> {
    use crate::sync::config::save_include_pattern;

    ui.exit_raw_mode();

    println!("{} — {} files\n", dir.path.display(), dir.files.len());

    println!("  Summary:");
    println!("    Modified:  {} files", dir.modified_count);
    println!("    Added:     {} files", dir.added_count);
    println!("    Deleted:   {} files", dir.deleted_count);

    let largest_changes: Vec<&FileChange> = dir.files.iter().take(3).collect();
    if !largest_changes.is_empty() {
        println!("\n  Largest changes:");
        for file in largest_changes {
            println!(
                "    {} {:?}",
                file.change_type.as_prefix(),
                file.path.display()
            );
        }
    }

    println!("\n[y] Sync all {} files", dir.files.len());
    println!("[n] Skip");
    println!("[r] Remember: always skip this pattern (global setting)");
    println!("[R] Remember: always sync this pattern (global setting)");
    if dir.files.len() <= super::MAX_VIEWABLE_FILES {
        println!("[v] View individual files");
    }
    println!("[q] Back\n");
    print!("Choice: ");
    std::io::Write::flush(&mut std::io::stdout())
        .map_err(|e| TreebeardError::Config(format!("Failed to flush stdout: {}", e)))?;

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| TreebeardError::Config(format!("Failed to read input: {}", e)))?;

    let trimmed_input = input.trim();
    match trimmed_input {
        "y" => {
            println!("\n✓ Marked {} for sync", dir.path.display());
            Ok(true)
        }
        "r" => {
            let pattern = format!("{}/**", dir.path.to_string_lossy());
            if let Err(e) = save_skip_pattern(pattern.clone()) {
                println!("\nFailed to save skip pattern: {}", e);
            } else {
                println!(
                    "\nAdded \"{}\" to sync_always_skip in global config.",
                    pattern
                );
                println!("This pattern will be automatically skipped in future sessions.");
            }
            println!("  Skipped: {}", dir.path.display());
            Ok(false)
        }
        "R" => {
            let pattern = format!("{}/**", dir.path.to_string_lossy());
            if let Err(e) = save_include_pattern(pattern.clone()) {
                println!("\nFailed to save include pattern: {}", e);
            } else {
                println!(
                    "\nAdded \"{}\" to sync_always_include in global config.",
                    pattern
                );
                println!("This pattern will be automatically selected in future sessions.");
            }
            println!("✓ Marked {} for sync", dir.path.display());
            Ok(true)
        }
        "v" if dir.files.len() <= super::MAX_VIEWABLE_FILES => {
            ui.enter_raw_mode()?;
            match run_directory_file_selection(dir, repo_path, worktree_path, ui) {
                Ok(SyncResult::Synced(count)) => {
                    ui.exit_raw_mode();
                    println!("\n✓ Marked {} files for sync", count);
                    Ok(count > 0)
                }
                Ok(SyncResult::Cancelled) => {
                    ui.exit_raw_mode();
                    println!("\nInterrupted.");
                    Ok(false)
                }
                Ok(SyncResult::Partial(progress)) => {
                    ui.exit_raw_mode();
                    println!(
                        "\n✓ Partially synced: {} succeeded, {} failed",
                        progress.synced_files.len(),
                        progress.failed_files.len()
                    );
                    Ok(!progress.synced_files.is_empty())
                }
                _ => {
                    ui.exit_raw_mode();
                    Ok(false)
                }
            }
        }
        _ => {
            println!("\n✓ Skipped {}", dir.path.display());
            Ok(false)
        }
    }
}

fn run_directory_file_selection(
    dir: &DirectoryChange,
    repo_path: &Path,
    worktree_path: &Path,
    ui: &mut SyncUI,
) -> Result<SyncResult> {
    ui.exit_raw_mode();

    let mut selected: HashSet<usize> = HashSet::new();
    let mut cursor = 0;
    let page_size = 15;
    let mut page = 0;

    loop {
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
            crossterm::cursor::MoveTo(0, 0)
        )
        .ok();

        let total_pages = dir.files.len().div_ceil(page_size);
        println!(
            "{} — viewing files (page {}/{})\n",
            dir.path.display(),
            page + 1,
            total_pages
        );

        let start_idx = page * page_size;
        let end_idx = (start_idx + page_size).min(dir.files.len());

        for i in start_idx..end_idx {
            let absolute_idx = i;
            let file = &dir.files[absolute_idx];
            let is_cursor = absolute_idx == cursor;
            let is_selected = selected.contains(&absolute_idx);
            let marker = if is_cursor { ">" } else { " " };
            let checkbox = if is_selected { "[x]" } else { "[ ]" };

            println!(
                "  {} {} {} {:?}",
                marker,
                checkbox,
                file.change_type.as_prefix(),
                file.path.display()
            );
        }

        println!("\n[↑↓] navigate  [space] toggle  [enter] view diff");
        if page > 0 {
            println!("[p] prev page");
        }
        if page < total_pages - 1 {
            println!("[n] next page");
        }
        println!("[a] select all  [d] done with directory  [q] back");

        loop {
            if let Some(ev) = ui.poll_input(Duration::from_millis(100))? {
                if ui.cancelled.load(Ordering::Relaxed) {
                    ui.exit_raw_mode();
                    return Ok(SyncResult::Cancelled);
                }

                if let Event::Key(key) = ev {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Up => {
                                if cursor > 0 {
                                    cursor -= 1;
                                    if cursor < page * page_size {
                                        page -= 1;
                                    }
                                }
                            }
                            KeyCode::Down => {
                                if cursor < dir.files.len().saturating_sub(1) {
                                    cursor += 1;
                                    if cursor >= (page + 1) * page_size {
                                        page += 1;
                                    }
                                }
                            }
                            KeyCode::Char(' ') => {
                                if selected.contains(&cursor) {
                                    selected.remove(&cursor);
                                } else {
                                    selected.insert(cursor);
                                }
                            }
                            KeyCode::Char('a') => {
                                if selected.len() == dir.files.len() {
                                    selected.clear();
                                } else {
                                    for i in 0..dir.files.len() {
                                        selected.insert(i);
                                    }
                                }
                            }
                            KeyCode::Char('d') => {
                                ui.exit_raw_mode();
                                return Ok(SyncResult::Synced(selected.len()));
                            }
                            KeyCode::Enter => {
                                ui.exit_raw_mode();
                                let file = &dir.files[cursor];
                                let should_sync =
                                    display_file_diff(file, repo_path, worktree_path)?;
                                if should_sync {
                                    selected.insert(cursor);
                                }
                                ui.enter_raw_mode()?;
                            }
                            KeyCode::Char('n') => {
                                if page < total_pages - 1 {
                                    page += 1;
                                    cursor = page * page_size;
                                }
                            }
                            KeyCode::Char('p') => {
                                if page > 0 {
                                    page -= 1;
                                    cursor = page * page_size;
                                }
                            }
                            KeyCode::Char('q') => {
                                ui.exit_raw_mode();
                                return Ok(SyncResult::Skipped);
                            }
                            _ => {}
                        }
                    }
                }
                break;
            }
        }
    }
}
