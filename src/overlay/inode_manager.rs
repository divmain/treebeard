use fuser::{FileAttr, FileType};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::sync::Arc;

use crate::overlay::types::{InodeData, InodeTable, LayerType};

/// Manages inode allocation, the inode table, and related operations.
///
/// This type encapsulates the single responsibility of tracking inodes,
/// their attributes, parent-child relationships, open file handles,
/// and deletion state. It does not perform filesystem I/O.
pub struct InodeManager {
    /// The inode table mapping inode numbers to inode data
    pub(crate) inodes: Arc<RwLock<InodeTable>>,
    /// Next inode number to allocate
    next_ino: Arc<Mutex<u64>>,
    /// Set of inodes that have been deleted but still have open handles
    deleted: Arc<RwLock<HashSet<u64>>>,
    /// Per-inode locks for copy-up synchronization
    pub(crate) copy_up_locks: Arc<RwLock<HashMap<u64, Arc<Mutex<()>>>>>,
}

impl InodeManager {
    /// Create a new InodeManager.
    pub fn new() -> Self {
        InodeManager {
            inodes: Arc::new(RwLock::new(InodeTable::new())),
            // Start at 2 because FUSE reserves inode 1 (FUSE_ROOT_ID) for the root directory
            next_ino: Arc::new(Mutex::new(2)),
            deleted: Arc::new(RwLock::new(HashSet::new())),
            copy_up_locks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Allocate a new unique inode number.
    pub fn alloc_inode(&self) -> u64 {
        let mut next = self.next_ino.lock();
        let ino = *next;
        // wrapping_add handles overflow gracefully - if we ever exhaust u64 (unlikely),
        // we wrap to 0 rather than panicking. This is acceptable since very old inodes
        // will have been freed by then.
        *next = next.wrapping_add(1);
        ino
    }

    /// Insert an inode into the table.
    pub fn insert(&self, inode: InodeData) {
        self.inodes.write().insert(inode);
    }

    /// Look up an inode by number, returning a clone of its path, layer, and open handle status.
    ///
    /// This is the common lookup pattern used throughout FUSE callbacks.
    /// Returns `(path, layer, has_open_handles)` if the inode exists, or `None` otherwise.
    pub fn get_inode_info(&self, ino: u64) -> Option<(PathBuf, LayerType, bool)> {
        let inodes = self.inodes.read();
        inodes
            .peek(ino)
            .map(|i| (i.path.clone(), i.layer, i.open_file_handles > 0))
    }

    /// Check if an inode exists and is in the upper layer.
    pub fn is_in_upper_layer(&self, ino: u64) -> bool {
        let inodes = self.inodes.read();
        inodes
            .peek(ino)
            .is_some_and(|inode| inode.layer == LayerType::Upper)
    }

    /// Look up a child inode by name within a parent directory.
    pub fn lookup_child(&self, parent: u64, name: &OsStr) -> Option<u64> {
        let inodes = self.inodes.read();
        inodes.lookup_child(parent, name)
    }

    /// Add a child to a parent directory.
    pub fn add_child(&self, parent: u64, name: OsString, ino: u64) {
        self.inodes.write().add_child(parent, name, ino);
    }

    /// Remove a child from a parent directory.
    pub fn remove_child(&self, parent: u64, name: &OsStr) {
        self.inodes.write().remove_child(parent, name);
    }

    /// Update the layer and path of an inode after copy-up.
    pub fn update_after_copy_up(&self, ino: u64, path: PathBuf, attrs: FileAttr) {
        let mut inodes = self.inodes.write();
        if let Some(inode) = inodes.get_mut(ino) {
            inode.layer = LayerType::Upper;
            inode.path = path;
            inode.attrs = attrs;
        }
    }

    /// Update the attributes of an inode.
    pub fn update_attrs(&self, ino: u64, attrs: FileAttr) {
        self.inodes.write().update_attrs(ino, attrs);
    }

    /// Update the size of an inode.
    pub fn update_size(&self, ino: u64, new_size: u64) {
        self.inodes.write().update_size(ino, new_size);
    }

    /// Increment the open file handles count for an inode.
    pub fn increment_open_handles(&self, ino: u64) {
        let mut inodes = self.inodes.write();
        if let Some(inode) = inodes.get_mut(ino) {
            inode.open_file_handles += 1;
        }
    }

    /// Decrement the open file handles count for an inode.
    /// Returns true if the inode should be garbage collected (no links or handles).
    pub fn decrement_open_handles(&self, ino: u64) -> bool {
        let mut inodes = self.inodes.write();
        if let Some(inode) = inodes.get_mut(ino) {
            inode.open_file_handles = inode.open_file_handles.saturating_sub(1);
            inode.hardlinks == 0 && inode.open_file_handles == 0
        } else {
            false
        }
    }

    /// Decrement hardlinks count for an inode.
    pub fn decrement_hardlinks(&self, ino: u64) {
        let mut inodes = self.inodes.write();
        if let Some(inode) = inodes.get_mut(ino) {
            inode.hardlinks = inode.hardlinks.saturating_sub(1);
            inode.attrs.nlink = inode.attrs.nlink.saturating_sub(1);
        }
    }

    /// Increment hardlinks count for an inode.
    pub fn increment_hardlinks(&self, ino: u64) {
        let mut inodes = self.inodes.write();
        if let Some(inode) = inodes.get_mut(ino) {
            inode.hardlinks += 1;
            inode.attrs.nlink += 1;
        }
    }

    /// Update the layer of an inode if it was stale.
    pub fn update_layer_if_needed(&self, ino: u64, layer: LayerType) {
        let mut inodes = self.inodes.write();
        if let Some(inode) = inodes.get_mut(ino) {
            if inode.layer != layer {
                tracing::debug!(
                    "Correcting layer for inode {}: {:?} -> {:?}",
                    ino,
                    inode.layer,
                    layer
                );
                inode.layer = layer;
            }
        }
    }

    /// Update inode after rename operation.
    #[allow(clippy::too_many_arguments)]
    pub fn update_after_rename(
        &self,
        ino: u64,
        old_parent: u64,
        old_name: &OsStr,
        new_parent: u64,
        new_name: OsString,
        new_path: PathBuf,
        layer: LayerType,
    ) {
        let mut inodes = self.inodes.write();
        inodes.remove_child(old_parent, old_name);
        inodes.add_child(new_parent, new_name.clone(), ino);
        if let Some(inode) = inodes.get_mut(ino) {
            inode.path = new_path;
            inode.parent = new_parent;
            inode.name = new_name;
            inode.layer = layer;
        }
    }

    /// Mark an inode as deleted (has open handles, will be garbage collected later).
    pub fn mark_deleted(&self, ino: u64) {
        self.deleted.write().insert(ino);
    }

    /// Check if an inode is marked as deleted.
    pub fn is_deleted(&self, ino: u64) -> bool {
        self.deleted.read().contains(&ino)
    }

    /// Remove an inode from the deleted set.
    pub fn unmark_deleted(&self, ino: u64) {
        self.deleted.write().remove(&ino);
    }

    /// Remove an inode from the table entirely.
    pub fn remove(&self, ino: u64) {
        self.inodes.write().remove(ino);
    }

    /// Check if an inode should be garbage collected.
    /// Returns true if the inode has no hardlinks and no open file handles.
    pub fn should_gc(&self, ino: u64) -> bool {
        let inodes = self.inodes.read();
        if let Some(inode) = inodes.peek(ino) {
            inode.hardlinks == 0 && inode.open_file_handles == 0
        } else {
            false
        }
    }

    /// Acquire a copy-up lock for an inode.
    /// Returns a clone of the lock Arc that can be locked by the caller.
    pub fn get_copy_up_lock(&self, ino: u64) -> Arc<Mutex<()>> {
        let mut locks = self.copy_up_locks.write();
        locks
            .entry(ino)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Remove the copy-up lock for an inode after copy-up is complete.
    pub fn remove_copy_up_lock(&self, ino: u64) {
        self.copy_up_locks.write().remove(&ino);
    }

    /// Creates a new InodeData with the given parameters.
    ///
    /// This helper reduces code duplication across FUSE operations that need
    /// to create new inodes. It handles the common case of calculating the
    /// hardlink count based on file type (2 for directories, 1 for files).
    pub fn create_inode_data(
        inode: u64,
        parent: u64,
        name: OsString,
        layer: LayerType,
        path: PathBuf,
        attrs: FileAttr,
    ) -> InodeData {
        InodeData {
            inode,
            parent,
            name,
            layer,
            path,
            attrs,
            open_file_handles: 0,
            hardlinks: if attrs.kind == FileType::Directory {
                2
            } else {
                1
            },
        }
    }

    /// Get the attributes for a given inode.
    /// Returns None if the inode doesn't exist.
    pub fn get_attrs(&self, ino: u64) -> Option<FileAttr> {
        let inodes = self.inodes.read();
        inodes.peek(ino).map(|i| i.attrs)
    }

    /// Get the path for a given inode.
    /// Returns None if the inode doesn't exist.
    pub fn get_path(&self, ino: u64) -> Option<PathBuf> {
        let inodes = self.inodes.read();
        inodes.peek(ino).map(|i| i.path.clone())
    }

    /// Get the layer for a given inode.
    /// Returns None if the inode doesn't exist.
    #[allow(dead_code)]
    pub fn get_layer(&self, ino: u64) -> Option<LayerType> {
        let inodes = self.inodes.read();
        inodes.peek(ino).map(|i| i.layer)
    }

    /// Test helper: get the number of copy-up locks.
    #[cfg(test)]
    pub fn copy_up_locks_count(&self) -> usize {
        self.copy_up_locks.read().len()
    }
}

impl Default for InodeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn create_test_attrs(ino: u64) -> FileAttr {
        FileAttr {
            ino,
            size: 1000,
            blocks: 2,
            atime: SystemTime::UNIX_EPOCH,
            mtime: SystemTime::UNIX_EPOCH,
            ctime: SystemTime::UNIX_EPOCH,
            crtime: SystemTime::UNIX_EPOCH,
            kind: FileType::RegularFile,
            perm: 0o644,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            blksize: 512,
            flags: 0,
        }
    }

    #[test]
    fn test_alloc_inode() {
        let manager = InodeManager::new();

        // First allocation should be 2 (inode 1 is reserved for root)
        assert_eq!(manager.alloc_inode(), 2);
        assert_eq!(manager.alloc_inode(), 3);
        assert_eq!(manager.alloc_inode(), 4);
    }

    #[test]
    fn test_insert_and_get_inode_info() {
        let manager = InodeManager::new();

        let attrs = create_test_attrs(100);
        let inode = InodeManager::create_inode_data(
            100,
            1,
            OsString::from("test.txt"),
            LayerType::Upper,
            PathBuf::from("test.txt"),
            attrs,
        );

        manager.insert(inode);

        let info = manager.get_inode_info(100);
        assert!(info.is_some());
        let (path, layer, has_open) = info.unwrap();
        assert_eq!(path, PathBuf::from("test.txt"));
        assert_eq!(layer, LayerType::Upper);
        assert!(!has_open);
    }

    #[test]
    fn test_open_handles() {
        let manager = InodeManager::new();

        let attrs = create_test_attrs(100);
        let inode = InodeManager::create_inode_data(
            100,
            1,
            OsString::from("test.txt"),
            LayerType::Upper,
            PathBuf::from("test.txt"),
            attrs,
        );

        manager.insert(inode);

        // Initially no open handles
        let (_, _, has_open) = manager.get_inode_info(100).unwrap();
        assert!(!has_open);

        // Increment open handles
        manager.increment_open_handles(100);
        let (_, _, has_open) = manager.get_inode_info(100).unwrap();
        assert!(has_open);

        // Decrement - should not trigger GC since hardlinks > 0
        let should_gc = manager.decrement_open_handles(100);
        assert!(!should_gc);

        let (_, _, has_open) = manager.get_inode_info(100).unwrap();
        assert!(!has_open);
    }

    #[test]
    fn test_deleted_tracking() {
        let manager = InodeManager::new();

        assert!(!manager.is_deleted(100));

        manager.mark_deleted(100);
        assert!(manager.is_deleted(100));

        manager.unmark_deleted(100);
        assert!(!manager.is_deleted(100));
    }

    #[test]
    fn test_copy_up_locks() {
        let manager = InodeManager::new();

        assert_eq!(manager.copy_up_locks_count(), 0);

        // Getting a lock creates it
        let _lock = manager.get_copy_up_lock(100);
        assert_eq!(manager.copy_up_locks_count(), 1);

        // Getting the same lock doesn't create a new one
        let _lock2 = manager.get_copy_up_lock(100);
        assert_eq!(manager.copy_up_locks_count(), 1);

        // Getting a different lock creates it
        let _lock3 = manager.get_copy_up_lock(200);
        assert_eq!(manager.copy_up_locks_count(), 2);

        // Removing a lock
        manager.remove_copy_up_lock(100);
        assert_eq!(manager.copy_up_locks_count(), 1);
    }
}
