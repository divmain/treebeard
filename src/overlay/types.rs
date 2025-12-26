use fuser::{FileAttr, FUSE_ROOT_ID};
use fxhash::hash64;
use lru::LruCache;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::num::NonZeroUsize;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_INODE_CACHE_CAPACITY: usize = 10000;

/// Represents a mutation type for tracking
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationType {
    CopiedUp,
    Created,
    Deleted,
}

/// Tracks all file mutations in the overlay
pub type MutationTracker = Arc<RwLock<HashMap<PathBuf, MutationType>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayerType {
    Upper,
    Lower,
}

#[derive(Debug, Clone)]
pub(crate) struct InodeData {
    pub inode: u64,
    pub parent: u64,
    pub name: OsString,
    pub layer: LayerType,
    pub path: PathBuf,
    pub attrs: FileAttr,
    pub open_file_handles: u64,
    pub hardlinks: u32,
}

#[derive(Debug)]
pub(crate) struct InodeTable {
    inodes: LruCache<u64, InodeData>,
    children: HashMap<u64, HashMap<u64, u64>>,
}

impl InodeTable {
    pub fn new() -> Self {
        InodeTable {
            inodes: LruCache::new(NonZeroUsize::new(DEFAULT_INODE_CACHE_CAPACITY).unwrap()),
            children: HashMap::new(),
        }
    }

    fn children_map(&self, parent: u64) -> Option<&HashMap<u64, u64>> {
        self.children.get(&parent)
    }

    fn children_map_mut(&mut self, parent: u64) -> &mut HashMap<u64, u64> {
        self.children.entry(parent).or_default()
    }

    pub fn insert(&mut self, inode: InodeData) {
        if inode.inode != FUSE_ROOT_ID {
            let name_hash = hash64(inode.name.as_bytes());
            self.children_map_mut(inode.parent)
                .insert(name_hash, inode.inode);
        }
        self.inodes.put(inode.inode, inode);
    }

    pub fn peek(&self, ino: u64) -> Option<&InodeData> {
        self.inodes.peek(&ino)
    }

    pub fn get_mut(&mut self, ino: u64) -> Option<&mut InodeData> {
        self.inodes.get_mut(&ino)
    }

    pub fn update_attrs(&mut self, ino: u64, attrs: FileAttr) {
        if let Some(inode) = self.get_mut(ino) {
            inode.attrs = attrs;
        }
    }

    pub fn update_size(&mut self, ino: u64, new_size: u64) {
        if let Some(inode) = self.get_mut(ino) {
            inode.attrs.size = new_size;
        }
    }

    /// Look up a child inode by name within a parent directory.
    ///
    /// Uses 64-bit FxHash for name lookups. We don't verify that the inode's
    /// stored name matches the requested name because hard links allow the same
    /// inode to have multiple names in different directories. The children map
    /// is authoritative for (parent, name) -> inode mappings.
    ///
    /// Hash collisions are theoretically possible but astronomically unlikely
    /// with 64-bit hashes scoped to individual directories.
    pub fn lookup_child(&self, parent: u64, name: &OsStr) -> Option<u64> {
        let name_hash = hash64(name.as_bytes());
        self.children_map(parent)
            .and_then(|map| map.get(&name_hash).copied())
            .and_then(|ino| self.peek(ino).map(|_| ino))
    }

    pub fn add_child(&mut self, parent: u64, name: OsString, ino: u64) {
        let name_hash = hash64(name.as_bytes());
        self.children_map_mut(parent).insert(name_hash, ino);
    }

    pub fn remove_child(&mut self, parent: u64, name: &OsStr) {
        let name_hash = hash64(name.as_bytes());
        if let Some(map) = self.children.get_mut(&parent) {
            map.remove(&name_hash);
        }
    }

    pub fn remove(&mut self, ino: u64) {
        if let Some(inode) = self.inodes.pop(&ino) {
            let name_hash = hash64(inode.name.as_bytes());
            if let Some(map) = self.children.get_mut(&inode.parent) {
                map.remove(&name_hash);
            }
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inodes.len()
    }

    #[cfg(test)]
    pub fn cap(&self) -> NonZeroUsize {
        self.inodes.cap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuser::FileType;
    use std::time::SystemTime;

    #[test]
    fn test_inode_table_lru_cache_capacity() {
        let table = InodeTable::new();
        let capacity = table.cap().get();

        assert_eq!(
            capacity, DEFAULT_INODE_CACHE_CAPACITY,
            "InodeTable should have capacity of DEFAULT_INODE_CACHE_CAPACITY"
        );
    }

    #[test]
    fn test_inode_table_insert_and_peek() {
        let mut table = InodeTable::new();

        let inode = InodeData {
            inode: 100,
            parent: 1,
            name: OsString::from("test.txt"),
            layer: LayerType::Upper,
            path: PathBuf::from("test.txt"),
            attrs: FileAttr {
                ino: 100,
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
            },
            open_file_handles: 0,
            hardlinks: 1,
        };

        table.insert(inode.clone());
        assert_eq!(table.len(), 1);

        let retrieved = table.peek(100);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, OsString::from("test.txt"));
    }

    #[test]
    fn test_inode_table_lru_eviction() {
        let mut table = InodeTable::new();

        let capacity = table.cap().get();

        for i in 1..=capacity as u64 + 1 {
            let inode = InodeData {
                inode: i,
                parent: 1,
                name: OsString::from(format!("file{}.txt", i)),
                layer: LayerType::Upper,
                path: PathBuf::from(format!("file{}.txt", i)),
                attrs: FileAttr {
                    ino: i,
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
                },
                open_file_handles: 0,
                hardlinks: 1,
            };
            table.insert(inode);
        }

        // Cache should be at capacity, so one entry should have been evicted
        assert_eq!(table.len(), capacity);

        // The first entry should have been evicted (least recently used)
        let first_entry = table.peek(1);
        assert!(
            first_entry.is_none(),
            "First entry should be evicted after exceeding capacity"
        );

        // The most recent entry should still be present
        let last_entry = table.peek(capacity as u64 + 1);
        assert!(
            last_entry.is_some(),
            "Most recent entry should still be in cache"
        );
    }
}
