use fuser::FileAttr;
use std::collections::{HashMap, HashSet};
use std::ffi::{CString, OsStr, OsString};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use crate::overlay::convert::{metadata_to_fileattr, std_filetype_to_fuser};
use crate::overlay::inode_manager::InodeManager;
use crate::overlay::types::{InodeData, LayerType, MutationType};
use crate::overlay::TreebeardFs;

impl TreebeardFs {
    #[cfg(target_os = "macos")]
    pub(crate) fn clone_file_optimized(src: &Path, dest: &Path) -> io::Result<()> {
        use std::os::unix::ffi::OsStrExt;

        let src_cstr = CString::new(src.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains null byte"))?;
        let dest_cstr = CString::new(dest.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains null byte"))?;

        // SAFETY: Both paths are properly null-terminated CStrings that outlive the call.
        unsafe {
            if libc::clonefile(src_cstr.as_ptr(), dest_cstr.as_ptr(), 0) == 0 {
                return Ok(());
            }
        }
        fs::copy(src, dest)?;
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn clone_file_optimized(src: &Path, dest: &Path) -> io::Result<()> {
        fs::copy(src, dest)?;
        Ok(())
    }

    /// Internal copy-up implementation that assumes the caller is already holding
    /// the copy-up lock for this inode. Does not acquire or release the lock.
    ///
    /// This is used by `open()` which holds the lock through the entire operation
    /// to prevent race conditions between checking the layer and opening the file.
    pub(crate) fn copy_up_internal(&mut self, ino: u64) -> Result<(), i32> {
        // Check if already in upper layer
        if self.inode_manager.is_in_upper_layer(ino) {
            return Ok(());
        }

        let src_info = {
            let inodes = self.inode_manager.inodes.read();
            let inode = inodes.peek(ino).ok_or(libc::ENOENT)?;
            (inode.path.clone(), inode.name.clone(), inode.attrs)
        };

        let src_path = self.path_resolver.lower_path(&src_info.0);
        let dest_path = self.path_resolver.upper_path(&src_info.0);

        let _src_metadata = match fs::symlink_metadata(&src_path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                tracing::debug!(
                    "Source file disappeared during copy-up, checking if already in upper"
                );
                if self.inode_manager.is_in_upper_layer(ino) {
                    return Ok(());
                }
                return Err(libc::ENOENT);
            }
            Err(_e) => {
                return Err(libc::EIO);
            }
        };

        // Track directories we create so we can clean them up on failure.
        // We only track new directories that didn't exist before copy-up.
        let mut created_dirs: Vec<PathBuf> = Vec::new();
        let upper_layer = &self.path_resolver.upper_layer;

        // Copy-up may occur for files in nested directories. Ensure all parent
        // directories exist in the upper layer before copying the file.
        if let Some(parent_dir) = dest_path.parent() {
            // Walk up the path to find which directories need to be created
            let mut dirs_to_create: Vec<PathBuf> = Vec::new();
            let mut current = parent_dir.to_path_buf();

            while !current.exists() && current.starts_with(upper_layer) {
                dirs_to_create.push(current.clone());
                if let Some(parent) = current.parent() {
                    current = parent.to_path_buf();
                } else {
                    break;
                }
            }

            // Create directories from top to bottom
            for dir in dirs_to_create.iter().rev() {
                if let Err(e) = fs::create_dir(dir) {
                    // If it already exists, that's fine (race condition with another copy-up)
                    if e.kind() != io::ErrorKind::AlreadyExists {
                        // Clean up any directories we created before failing
                        for created in created_dirs.iter().rev() {
                            let _ = fs::remove_dir(created);
                        }
                        return Err(libc::EIO);
                    }
                } else {
                    created_dirs.push(dir.clone());
                }
            }
        }

        // Helper to clean up on error: remove the destination file/dir and any created parent dirs
        let cleanup_on_error = |created_dirs: &[PathBuf], dest: &Path| {
            // Remove the destination if it was partially created
            let _ = fs::remove_file(dest).or_else(|_| fs::remove_dir(dest));
            // Remove created parent directories in reverse order
            for dir in created_dirs.iter().rev() {
                let _ = fs::remove_dir(dir);
            }
        };

        if src_info.2.kind == fuser::FileType::Directory {
            if let Err(e) = fs::create_dir(&dest_path) {
                if e.kind() != io::ErrorKind::AlreadyExists {
                    cleanup_on_error(&created_dirs, &dest_path);
                    return Err(libc::EIO);
                }
            }
        } else if let Err(_e) = Self::clone_file_optimized(&src_path, &dest_path) {
            cleanup_on_error(&created_dirs, &dest_path);
            return Err(libc::EIO);
        }

        let new_attrs = match fs::metadata(&dest_path) {
            Ok(m) => m,
            Err(_e) => {
                cleanup_on_error(&created_dirs, &dest_path);
                return Err(libc::EIO);
            }
        };

        let file_attrs = metadata_to_fileattr(&new_attrs, ino);
        self.inode_manager
            .update_after_copy_up(ino, src_info.0.clone(), file_attrs);

        if src_info.2.kind == fuser::FileType::RegularFile {
            self.mutations
                .write()
                .insert(src_info.0, MutationType::CopiedUp);
        }

        Ok(())
    }

    pub(crate) fn copy_up(&mut self, ino: u64) -> Result<u64, i32> {
        let lock = self.inode_manager.get_copy_up_lock(ino);
        let _guard = lock.lock();

        self.copy_up_internal(ino)?;

        // Clean up the lock after successful copy-up
        self.inode_manager.remove_copy_up_lock(ino);

        Ok(ino)
    }

    /// Create a whiteout marker using AUFS-style `.wh.` prefix files.
    ///
    /// This creates an empty file named `.wh.<filename>` in the upper layer's
    /// corresponding directory to mark that the original file should be hidden.
    pub(crate) fn create_whiteout(
        &mut self,
        parent: u64,
        name: &std::ffi::OsStr,
    ) -> Result<(), i32> {
        let parent_path = {
            let inodes = self.inode_manager.inodes.read();
            let parent_inode = inodes.peek(parent);
            match parent_inode {
                Some(p) => self.path_resolver.upper_path(&p.path),
                None => return Err(libc::ENOENT),
            }
        };

        // Ensure the parent directory exists in the upper layer
        if !parent_path.exists() {
            fs::create_dir_all(&parent_path).map_err(|_| libc::EIO)?;
        }

        // Create .wh.<filename> whiteout marker
        let mut whiteout_name = OsString::from(".wh.");
        whiteout_name.push(name);
        let whiteout_path = parent_path.join(whiteout_name);

        // Create an empty file as the whiteout marker
        File::create(&whiteout_path).map_err(|e| e.raw_os_error().unwrap_or(libc::EIO))?;

        Ok(())
    }

    pub(crate) fn do_delete(&mut self, ino: u64) {
        let path = {
            let inodes = self.inode_manager.inodes.read();
            let inode = inodes.peek(ino);
            match inode {
                Some(i) if i.layer == LayerType::Upper => {
                    Some(self.path_resolver.upper_path(&i.path))
                }
                _ => None,
            }
        };

        if let Some(p) = path {
            if let Err(e) = fs::remove_file(&p).or_else(|_| fs::remove_dir(&p)) {
                tracing::warn!("Failed to delete during cleanup: {} ({})", p.display(), e);
            }
        }
    }

    pub(crate) fn do_gc(&mut self, ino: u64) {
        let (layer, path) = {
            let inodes = self.inode_manager.inodes.read();
            let inode = inodes.peek(ino);
            match inode {
                Some(i) => (i.layer, self.path_resolver.upper_path(&i.path)),
                None => return,
            }
        };

        if layer == LayerType::Upper {
            if let Err(e) = fs::remove_file(&path).or_else(|_| fs::remove_dir(&path)) {
                tracing::warn!(
                    "Failed to delete during garbage collection: {} ({})",
                    path.display(),
                    e
                );
            }
        }

        self.inode_manager.remove(ino);
    }

    /// Attempt to look up a file via passthrough (lower layer only, ignoring upper layer).
    ///
    /// This is used for paths that match passthrough patterns - they should always
    /// read directly from the lower layer, ignoring any modifications in the upper layer.
    ///
    /// # Returns
    /// - `Some(Ok((inode, attrs)))` - File found, inode data and attributes returned
    /// - `Some(Err(errno))` - Error occurred during lookup
    /// - `None` - Path is not a passthrough path, should use normal overlay lookup
    pub(crate) fn lookup_passthrough(
        &self,
        parent: u64,
        child_name: OsString,
        relative_path: PathBuf,
    ) -> Option<Result<(InodeData, FileAttr), i32>> {
        use crate::overlay::convert::{io_error_to_libc, metadata_to_fileattr};

        if !self.path_resolver.is_passthrough(&relative_path) {
            return None;
        }

        let child_lower = self.path_resolver.lower_path(&relative_path);
        if !child_lower.exists() {
            return Some(Err(libc::ENOENT));
        }

        match fs::metadata(&child_lower) {
            Ok(attrs) => {
                let new_ino = self.inode_manager.alloc_inode();
                let file_attrs = metadata_to_fileattr(&attrs, new_ino);
                let inode = InodeManager::create_inode_data(
                    new_ino,
                    parent,
                    child_name,
                    LayerType::Lower,
                    relative_path,
                    file_attrs,
                );
                Some(Ok((inode, file_attrs)))
            }
            Err(e) => Some(Err(io_error_to_libc(&e))),
        }
    }

    /// Attempt to look up a file using overlay semantics (upper layer shadows lower).
    ///
    /// This implements standard overlay filesystem lookup:
    /// 1. Check for whiteout in upper layer (file was deleted)
    /// 2. Check upper layer first (modifications take precedence)
    /// 3. Fall back to lower layer
    ///
    /// # Returns
    /// - `Ok(Some((inode, attrs)))` - File found, inode data and attributes returned
    /// - `Ok(None)` - File not found in either layer
    /// - `Err(errno)` - Error occurred during lookup (e.g., metadata read failure)
    pub(crate) fn lookup_overlay(
        &self,
        parent: u64,
        child_name: OsString,
        relative_path: PathBuf,
    ) -> Result<Option<(InodeData, FileAttr)>, i32> {
        use crate::overlay::convert::metadata_to_fileattr;

        let child_upper = self.path_resolver.upper_path(&relative_path);
        let child_lower = self.path_resolver.lower_path(&relative_path);

        tracing::debug!(
            "lookup_overlay: checking paths - upper={} (exists={}), lower={} (exists={})",
            child_upper.display(),
            child_upper.exists(),
            child_lower.display(),
            child_lower.exists()
        );

        // Check if there's a whiteout in the upper layer (file was deleted)
        if self.path_resolver.is_whiteout(&child_upper) {
            tracing::debug!("lookup_overlay: whiteout found for {:?}", child_name);
            return Ok(None);
        }

        // Try each layer in order: upper first (overlay semantics)
        let layers_to_check = [
            (&child_upper, LayerType::Upper, "upper"),
            (&child_lower, LayerType::Lower, "lower"),
        ];

        for (child_path, layer, layer_name) in layers_to_check {
            if child_path.exists() {
                tracing::debug!(
                    "lookup_overlay: file exists in {} layer: {}",
                    layer_name,
                    child_path.display()
                );
                match fs::metadata(child_path) {
                    Ok(attrs) => {
                        let new_ino = self.inode_manager.alloc_inode();
                        let file_attrs = metadata_to_fileattr(&attrs, new_ino);
                        tracing::debug!(
                            "lookup_overlay: created inode {} for {:?} ({} layer, size={})",
                            new_ino,
                            child_name,
                            layer_name,
                            file_attrs.size
                        );

                        let inode = InodeManager::create_inode_data(
                            new_ino,
                            parent,
                            child_name,
                            layer,
                            relative_path,
                            file_attrs,
                        );

                        return Ok(Some((inode, file_attrs)));
                    }
                    Err(e) => {
                        tracing::error!("lookup_overlay metadata error ({}): {}", layer_name, e);
                        return Err(libc::ENOENT);
                    }
                }
            }
        }

        // File not found in either layer
        Ok(None)
    }

    /// Check if a cached inode is still valid and return its attributes.
    ///
    /// Even for cached inodes, we need to verify they haven't been invalidated
    /// by a whiteout in the upper layer. Passthrough paths skip this check
    /// since they ignore the upper layer entirely.
    ///
    /// # Returns
    /// - `Ok(attrs)` - Cached inode is valid, return its attributes
    /// - `Err(errno)` - Cached inode is invalid (whited-out) or not found
    pub(crate) fn lookup_check_cached(
        &self,
        ino: u64,
        name: &std::ffi::OsStr,
    ) -> Result<FileAttr, i32> {
        let inodes = self.inode_manager.inodes.read();
        if let Some(inode) = inodes.peek(ino) {
            // Passthrough paths ignore whiteouts and upper layer entirely
            if self.path_resolver.is_passthrough(&inode.path) {
                return Ok(inode.attrs);
            }

            // Even for cached inodes, check if a whiteout has been created
            let upper_path = self.path_resolver.upper_path(&inode.path);
            if self.path_resolver.is_whiteout(&upper_path) {
                tracing::debug!(
                    "lookup_check_cached: cached inode {} for {:?} is now whited-out",
                    ino,
                    name
                );
                return Err(libc::ENOENT);
            }

            tracing::debug!(
                "lookup_check_cached: returning cached inode {} for {:?} (layer={:?}, path={:?})",
                ino,
                name,
                inode.layer,
                inode.path
            );
            Ok(inode.attrs)
        } else {
            tracing::warn!("lookup_check_cached: inode {} disappeared from table!", ino);
            Err(libc::ENOENT)
        }
    }

    /// Scan a directory layer and collect entries.
    ///
    /// This helper is used by readdir() to scan both the lower and upper layers.
    /// It collects existing children from the inode table, then processes directory
    /// entries, creating new inodes as needed.
    ///
    /// # Arguments
    /// * `layer_dir` - The directory path to scan
    /// * `layer` - The layer type (Upper or Lower)
    /// * `parent_ino` - The parent inode number
    /// * `dir_path` - The relative path within the overlay
    /// * `entries` - HashMap to collect entries into
    /// * `whiteouts` - HashSet to track whiteout markers (only updated for upper layer)
    /// * `new_inodes` - Vec to collect new InodeData for batch insertion
    /// * `layer_updates` - Vec to collect layer updates for existing inodes
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn scan_directory_layer(
        &self,
        layer_dir: &Path,
        layer: LayerType,
        parent_ino: u64,
        dir_path: &Path,
        entries: &mut HashMap<OsString, (u64, fuser::FileType, LayerType)>,
        whiteouts: &mut HashSet<OsString>,
        new_inodes: &mut Vec<InodeData>,
        layer_updates: &mut Vec<(u64, FileAttr)>,
    ) {
        let layer_name = match layer {
            LayerType::Upper => "upper",
            LayerType::Lower => "lower",
        };
        tracing::debug!(
            "readdir: scanning {}_dir={} (exists={})",
            layer_name,
            layer_dir.display(),
            layer_dir.exists()
        );

        if !layer_dir.exists() {
            return;
        }

        let Ok(read_dir) = fs::read_dir(layer_dir) else {
            return;
        };

        let dir_entries: Vec<std::fs::DirEntry> = read_dir.flatten().collect();

        // Take a single read lock snapshot for all lookups in this layer
        let existing_children: HashMap<OsString, u64> = {
            let inodes = self.inode_manager.inodes.read();
            dir_entries
                .iter()
                .filter_map(|entry| {
                    let name = entry.file_name();
                    inodes
                        .lookup_child(parent_ino, &name)
                        .map(|ino| (name, ino))
                })
                .collect()
        };

        for entry in dir_entries {
            let name = entry.file_name();

            // Handle whiteouts only in upper layer
            if layer == LayerType::Upper {
                #[cfg(unix)]
                {
                    let name_bytes = name.as_bytes();
                    let wh_prefix = b".wh.";
                    if name_bytes.starts_with(wh_prefix) {
                        let target_bytes = &name_bytes[wh_prefix.len()..];
                        let target = OsString::from(OsStr::from_bytes(target_bytes));
                        entries.remove(&target);
                        whiteouts.insert(target);
                        continue;
                    }
                }

                #[cfg(not(unix))]
                {
                    let name_str = name.to_string_lossy();
                    if let Some(target_name) = name_str.strip_prefix(".wh.") {
                        let target = OsString::from(target_name);
                        entries.remove(&target);
                        whiteouts.insert(target);
                        continue;
                    }
                }
            }

            // Use file_type() which doesn't require a full stat on most filesystems
            let file_type = match entry.file_type() {
                Ok(ft) => std_filetype_to_fuser(ft),
                Err(_) => continue,
            };

            let child_ino = match existing_children.get(&name) {
                Some(&existing_ino) => {
                    // For upper layer, check if we need to update layer from Lower to Upper
                    if layer == LayerType::Upper {
                        let needs_update = {
                            let inodes = self.inode_manager.inodes.read();
                            inodes
                                .peek(existing_ino)
                                .is_some_and(|inode| inode.layer == LayerType::Lower)
                        };

                        if needs_update {
                            if let Ok(metadata) = entry.metadata() {
                                let file_attrs = metadata_to_fileattr(&metadata, existing_ino);
                                layer_updates.push((existing_ino, file_attrs));
                            }
                        }
                    }
                    existing_ino
                }
                None => {
                    // Need full metadata only for new inodes
                    let metadata = match entry.metadata() {
                        Ok(m) => m,
                        Err(_) => continue,
                    };

                    let new_ino = self.inode_manager.alloc_inode();
                    let file_attrs = metadata_to_fileattr(&metadata, new_ino);
                    let relative_path = dir_path.join(&name);

                    let inode = InodeManager::create_inode_data(
                        new_ino,
                        parent_ino,
                        name.clone(),
                        layer,
                        relative_path,
                        file_attrs,
                    );

                    new_inodes.push(inode);
                    new_ino
                }
            };

            entries.insert(name, (child_ino, file_type, layer));
        }
    }

    pub(crate) fn do_remove<F>(
        &mut self,
        parent: u64,
        name: &OsStr,
        reply: fuser::ReplyEmpty,
        remove_fn: F,
    ) where
        F: FnOnce(PathBuf) -> std::io::Result<()>,
    {
        use crate::overlay::convert::io_error_to_libc;

        let lookup_result = self.inode_manager.lookup_child(parent, name);
        let ino = match lookup_result {
            Some(ino) => ino,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let Some((path, layer, has_open)) = self.inode_manager.get_inode_info(ino) else {
            reply.error(libc::ENOENT);
            return;
        };

        let is_passthrough = self.path_resolver.is_passthrough(&path);

        // Update inode state
        self.inode_manager.remove_child(parent, name);
        self.inode_manager.decrement_hardlinks(ino);

        if is_passthrough {
            let lower_path = self.path_resolver.lower_path(&path);
            if let Err(e) = remove_fn(lower_path) {
                reply.error(io_error_to_libc(&e));
                return;
            }
        } else {
            match layer {
                LayerType::Upper => {
                    if has_open {
                        self.inode_manager.mark_deleted(ino);
                    } else {
                        self.do_delete(ino);
                    }
                }
                LayerType::Lower => {
                    if let Err(e) = self.create_whiteout(parent, name) {
                        reply.error(e);
                        return;
                    }
                }
            }

            self.signal_mutation(&path);
        }

        reply.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clone_file_optimized() {
        let temp_dir = tempfile::tempdir().unwrap();
        let src_path = temp_dir.path().join("source.txt");
        let dest_path = temp_dir.path().join("dest.txt");

        let test_content = b"Hello, Treebeard!";
        fs::write(&src_path, test_content).unwrap();

        TreebeardFs::clone_file_optimized(&src_path, &dest_path).unwrap();

        assert!(dest_path.exists(), "Destination file should exist");
        let dest_content = fs::read(&dest_path).unwrap();
        assert_eq!(dest_content, test_content, "Content should match");
    }

    #[test]
    fn test_copy_up_locks_count() {
        let temp_dir = tempfile::tempdir().unwrap();
        let upper_layer = temp_dir.path().join("upper");
        let lower_layer = temp_dir.path().join("lower");

        fs::create_dir_all(&lower_layer).unwrap();

        // Create filesystem with 1 second TTL (the default from config)
        let fs = TreebeardFs::new(upper_layer, lower_layer, None, 1, vec![]).unwrap();

        // Initially, copy_up_locks should be empty
        assert_eq!(
            fs.copy_up_locks_count(),
            0,
            "copy_up_locks should be empty initially"
        );
    }
}
