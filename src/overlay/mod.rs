mod convert;
mod file_handle;
mod helpers;
pub mod mount;
pub mod setup;
pub mod types;

pub use mount::{cleanup_stale_mounts, perform_fuse_cleanup};
pub use setup::setup_overlay_and_watcher;
pub use types::{MutationTracker, MutationType};

use convert::{io_error_to_libc, metadata_to_fileattr};
use file_handle::{FileHandle, READ_BUFFER};
use types::{InodeData, InodeTable, LayerType};

use fuser::{
    FileAttr, FileType, Filesystem, KernelConfig, ReplyAttr, ReplyCreate, ReplyData,
    ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyWrite, ReplyXattr, Request, TimeOrNow,
    FUSE_ROOT_ID,
};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

pub struct TreebeardFs {
    pub(crate) upper_layer: PathBuf,
    pub(crate) lower_layer: PathBuf,
    pub(crate) inodes: Arc<RwLock<InodeTable>>,
    next_ino: Arc<Mutex<u64>>,
    deleted: Arc<RwLock<HashSet<u64>>>,
    file_handles: Arc<RwLock<HashMap<u64, FileHandle>>>,
    next_fh: Arc<Mutex<u64>>,
    pub(crate) mutations: MutationTracker,
    pub(crate) copy_up_locks: Arc<RwLock<HashMap<u64, Arc<Mutex<()>>>>>,
    /// Channel for signaling file mutations to the commit task.
    /// Unbounded because mutation events should never block FUSE operations.
    mutation_tx: Option<tokio::sync::mpsc::UnboundedSender<PathBuf>>,
    /// Attribute and entry cache timeout for FUSE. Configurable via config file.
    /// Higher values reduce kernel-userspace round trips but may delay visibility
    /// of external filesystem changes.
    ttl: Duration,
    /// Glob patterns for paths that bypass the upper layer entirely.
    passthrough_patterns: Vec<glob::Pattern>,
}

impl TreebeardFs {
    pub fn new(
        upper_layer: PathBuf,
        lower_layer: PathBuf,
        mutation_tx: Option<tokio::sync::mpsc::UnboundedSender<PathBuf>>,
        ttl_secs: u64,
        passthrough_patterns: Vec<String>,
    ) -> crate::error::Result<Self> {
        let compiled_patterns = passthrough_patterns
            .into_iter()
            .map(|p| {
                glob::Pattern::new(&p).map_err(|e| {
                    crate::error::TreebeardError::Config(format!(
                        "Invalid passthrough glob pattern '{}': {}",
                        p, e
                    ))
                })
            })
            .collect::<crate::error::Result<Vec<glob::Pattern>>>()?;

        let mut fs = TreebeardFs {
            upper_layer,
            lower_layer,
            inodes: Arc::new(RwLock::new(InodeTable::new())),
            // Start at 2 because FUSE reserves inode 1 (FUSE_ROOT_ID) for the root directory
            next_ino: Arc::new(Mutex::new(2)),
            deleted: Arc::new(RwLock::new(HashSet::new())),
            file_handles: Arc::new(RwLock::new(HashMap::new())),
            next_fh: Arc::new(Mutex::new(1)),
            mutations: Arc::new(RwLock::new(HashMap::new())),
            copy_up_locks: Arc::new(RwLock::new(HashMap::new())),
            mutation_tx,
            ttl: Duration::from_secs(ttl_secs),
            passthrough_patterns: compiled_patterns,
        };

        fs.initialize_root()?;
        Ok(fs)
    }

    #[cfg(test)]
    pub fn copy_up_locks_count(&self) -> usize {
        self.copy_up_locks.read().len()
    }

    /// Signal that a file was mutated. Called from FUSE callbacks.
    /// Failures are logged at debug level and don't affect FUSE operations.
    fn signal_mutation(&self, relative_path: &std::path::Path) {
        if let Some(ref tx) = self.mutation_tx {
            if let Err(e) = tx.send(relative_path.to_path_buf()) {
                tracing::debug!("Failed to signal mutation for {:?}: {}", relative_path, e);
            }
        }
    }

    fn initialize_root(&mut self) -> crate::error::Result<()> {
        let root_attrs = if self.upper_layer.exists() {
            fs::metadata(&self.upper_layer).map_err(|e| {
                crate::error::TreebeardError::Config(format!(
                    "Failed to get metadata for {}: {}",
                    self.upper_layer.display(),
                    e
                ))
            })?
        } else {
            fs::create_dir_all(&self.upper_layer).map_err(|e| {
                crate::error::TreebeardError::Config(format!(
                    "Failed to create directory {}: {}",
                    self.upper_layer.display(),
                    e
                ))
            })?;
            fs::metadata(&self.upper_layer).map_err(|e| {
                crate::error::TreebeardError::Config(format!(
                    "Failed to get metadata for {}: {}",
                    self.upper_layer.display(),
                    e
                ))
            })?
        };

        let file_attrs = metadata_to_fileattr(&root_attrs, FUSE_ROOT_ID);
        let inode = Self::create_inode_data(
            FUSE_ROOT_ID,
            0,
            OsString::new(),
            LayerType::Upper,
            PathBuf::from("."),
            file_attrs,
        );

        self.inodes.write().insert(inode);
        Ok(())
    }

    pub(crate) fn alloc_inode(&self) -> u64 {
        let mut next = self.next_ino.lock();
        let ino = *next;
        // wrapping_add handles overflow gracefully - if we ever exhaust u64 (unlikely),
        // we wrap to 0 rather than panicking. This is acceptable since very old inodes
        // will have been freed by then.
        *next = next.wrapping_add(1);
        ino
    }

    fn alloc_fh(&self) -> u64 {
        let mut next = self.next_fh.lock();
        let fh = *next;
        // wrapping_add for consistency with alloc_inode - file handles are ephemeral
        // and recycled when files are closed, so overflow is not a practical concern
        *next = next.wrapping_add(1);
        fh
    }
}

impl Filesystem for TreebeardFs {
    fn init(
        &mut self,
        _req: &Request,
        _config: &mut KernelConfig,
    ) -> std::result::Result<(), libc::c_int> {
        tracing::info!("Treebeard FUSE filesystem initialized");
        Ok(())
    }

    fn destroy(&mut self) {
        tracing::info!("Treebeard FUSE filesystem destroyed");
    }

    fn forget(&mut self, _req: &Request, ino: u64, _nlookup: u64) {
        // The kernel is releasing nlookup references to this inode.
        // We track open file handles separately, so we only need to check
        // if the inode should be garbage collected when it's in the deleted set.
        let should_gc = {
            let inodes = self.inodes.read();
            if let Some(inode) = inodes.peek(ino) {
                inode.hardlinks == 0 && inode.open_file_handles == 0
            } else {
                false
            }
        };

        if should_gc && self.deleted.read().contains(&ino) {
            self.do_gc(ino);
            self.deleted.write().remove(&ino);
        }
    }

    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        tracing::debug!("lookup(parent={}, name={:?})", parent, name);

        // First check if we already have this inode cached
        let cached_ino = {
            let inodes = self.inodes.read();
            inodes.lookup_child(parent, name)
        };

        if let Some(ino) = cached_ino {
            tracing::debug!("lookup: found cached inode {} for {:?}", ino, name);
            match self.lookup_check_cached(ino, name) {
                Ok(attrs) => reply.entry(&self.ttl, &attrs, 0),
                Err(errno) => reply.error(errno),
            }
            return;
        }

        // Not cached - need to look up from filesystem
        tracing::debug!(
            "lookup: no cached inode for {:?}, checking filesystem",
            name
        );

        // Get the parent path
        let parent_inode_path = {
            let inodes = self.inodes.read();
            match inodes.peek(parent) {
                Some(i) => i.path.clone(),
                None => {
                    tracing::warn!("lookup: parent inode {} not found", parent);
                    reply.error(libc::ENOENT);
                    return;
                }
            }
        };

        let child_name = name.to_os_string();
        let relative_path = parent_inode_path.join(&child_name);

        // Try passthrough lookup first (bypasses upper layer)
        if let Some(result) =
            self.lookup_passthrough(parent, child_name.clone(), relative_path.clone())
        {
            match result {
                Ok((inode, file_attrs)) => {
                    self.inodes.write().insert(inode);
                    reply.entry(&self.ttl, &file_attrs, 0);
                }
                Err(errno) => reply.error(errno),
            }
            return;
        }

        // Use standard overlay lookup (upper shadows lower)
        match self.lookup_overlay(parent, child_name, relative_path.clone()) {
            Ok(Some((inode, file_attrs))) => {
                self.inodes.write().insert(inode);
                reply.entry(&self.ttl, &file_attrs, 0);
            }
            Ok(None) => {
                tracing::debug!("lookup: file {:?} not found in either layer", name);
                reply.error(libc::ENOENT);
            }
            Err(errno) => reply.error(errno),
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        tracing::trace!("getattr(ino={})", ino);
        let inodes = self.inodes.read();

        if let Some(inode) = inodes.peek(ino) {
            tracing::trace!(
                "getattr: ino={} -> path={:?}, layer={:?}, size={}",
                ino,
                inode.path,
                inode.layer,
                inode.attrs.size
            );
            reply.attr(&self.ttl, &inode.attrs);
        } else {
            tracing::warn!("getattr: inode {} not found", ino);
            reply.error(libc::ENOENT);
        }
    }

    fn setattr(
        &mut self,
        _req: &Request,
        ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<TimeOrNow>,
        _mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        let Some((rel_path, _layer, _has_open)) = self.get_inode_info(ino) else {
            reply.error(libc::ENOENT);
            return;
        };

        let is_passthrough = self.is_passthrough(&rel_path);

        if !is_passthrough {
            if let Err(e) = self.copy_up(ino) {
                reply.error(e);
                return;
            }
        }

        let inodes = self.inodes.read();
        if let Some(inode) = inodes.peek(ino) {
            let base = self.layer_base_path(inode.layer);
            let path = base.join(&inode.path);
            drop(inodes);

            if let Some(s) = size {
                if let Ok(f) = OpenOptions::new().write(true).open(&path) {
                    let _ = f.set_len(s);
                }
            }

            match fs::metadata(&path) {
                Ok(attrs) => {
                    let file_attrs = metadata_to_fileattr(&attrs, ino);
                    self.inodes.write().update_attrs(ino, file_attrs);
                    reply.attr(&self.ttl, &file_attrs);
                }
                Err(e) => reply.error(io_error_to_libc(&e)),
            }
        } else {
            reply.error(libc::ENOENT);
        }
    }

    /// Open a file for reading or writing.
    ///
    /// # Concurrent Open/Copy-Up Semantics
    ///
    /// This function acquires a copy-up lock for the inode and holds it through
    /// the entire operation to prevent race conditions between checking the layer
    /// and opening the file. This ensures that no other thread can delete or
    /// modify the file between the layer check and the file open.
    ///
    /// When multiple threads open the same file concurrently, they may see different
    /// layer states. However, the copy-up operation is atomic per-file and the lock
    /// ensures consistency during the open sequence.
    fn open(&mut self, _req: &Request, ino: u64, flags: i32, reply: ReplyOpen) {
        tracing::debug!("open(ino={}, flags={:#x})", ino, flags);

        // Acquire the copy-up lock for this inode before checking the layer.
        // This prevents race conditions where another thread could delete or
        // modify the file between the layer check and the file open.
        use std::sync::Arc;
        let copy_up_lock = {
            let mut locks = self.copy_up_locks.write();
            locks
                .entry(ino)
                .or_insert_with(|| Arc::new(parking_lot::Mutex::new(())))
                .clone()
        };

        let Some((rel_path, layer, _has_open)) = self.get_inode_info(ino) else {
            tracing::warn!("open: inode {} not found in table", ino);
            reply.error(libc::ENOENT);
            return;
        };

        tracing::debug!(
            "open: inode {} -> path={:?}, layer={:?}",
            ino,
            rel_path,
            layer
        );

        // Determine if write access is requested
        // Note: O_RDONLY is 0, so we check if it's NOT read-only
        let wants_write = (flags & libc::O_ACCMODE) != libc::O_RDONLY;
        tracing::debug!("open: wants_write={}", wants_write);

        // Check if this is a passthrough file (writes go directly to lower layer, no copy-up)
        let is_passthrough = self.is_passthrough(&rel_path);

        // If file is in lower layer and write access is requested, do COW first
        // (but NOT for passthrough files, which write directly to lower layer)
        if layer == LayerType::Lower && wants_write && !is_passthrough {
            let _copy_up_guard = copy_up_lock.lock();
            if let Err(e) = self.copy_up_internal(ino) {
                tracing::error!("open: copy-up failed with error {}", e);
                reply.error(e);
                return;
            }
        }

        // Acquire the lock again to protect the file open operation
        let _copy_up_guard = copy_up_lock.lock();

        // Get the actual path and verify the current layer (in case copy_up changed it)
        let (actual_path, current_layer) = {
            let inodes = self.inodes.read();
            if let Some(inode) = inodes.peek(ino) {
                // Use resolve_path to find where the file actually exists
                // This handles cases where the inode's layer might be stale
                tracing::debug!(
                    "open: resolving path {:?} with layer {:?}",
                    inode.path,
                    inode.layer
                );
                match self.resolve_path(&inode.path, inode.layer) {
                    Some((path, actual_layer)) => {
                        tracing::debug!(
                            "open: resolve_path returned path={}, layer={:?}",
                            path.display(),
                            actual_layer
                        );
                        (path, actual_layer)
                    }
                    None => {
                        // Fall back to computing path from inode's layer
                        let fallback_path = self.layer_base_path(inode.layer).join(&inode.path);
                        tracing::warn!(
                            "open: resolve_path returned None, falling back to {}",
                            fallback_path.display()
                        );
                        (fallback_path, inode.layer)
                    }
                }
            } else {
                tracing::warn!("open: inode {} disappeared after copy-up check", ino);
                reply.error(libc::ENOENT);
                return;
            }
        };

        // For lower layer files (read-only), only allow read access
        // UNLESS it's a passthrough file, in which case we allow write to lower layer
        let (can_read, can_write) = match current_layer {
            LayerType::Lower => (true, is_passthrough),
            LayerType::Upper => (true, true),
        };

        tracing::debug!(
            "open: opening {} with read={}, write={}, path_exists={}",
            actual_path.display(),
            can_read,
            can_write,
            actual_path.exists()
        );

        let file = match File::options()
            .read(can_read)
            .write(can_write)
            .create(flags & libc::O_CREAT != 0)
            .truncate(can_write && (flags & libc::O_TRUNC != 0))
            .append(can_write && (flags & libc::O_APPEND != 0))
            .open(&actual_path)
        {
            Ok(f) => {
                tracing::debug!("open: successfully opened file");
                f
            }
            Err(e) => {
                reply.error(io_error_to_libc(&e));
                return;
            }
        };

        let fh = self.alloc_fh();
        let handle = FileHandle {
            file: Arc::new(Mutex::new(file)),
        };

        self.file_handles.write().insert(fh, handle);

        // Update the inode's layer if it changed (e.g., file was found in different layer)
        {
            let mut inodes = self.inodes.write();
            if let Some(inode) = inodes.get_mut(ino) {
                inode.open_file_handles += 1;
                if inode.layer != current_layer {
                    tracing::debug!(
                        "Correcting layer for inode {}: {:?} -> {:?}",
                        ino,
                        inode.layer,
                        current_layer
                    );
                    inode.layer = current_layer;
                }
            }
        }

        reply.opened(fh, 0);
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        tracing::debug!(
            "read(ino={}, fh={}, offset={}, size={})",
            ino,
            fh,
            offset,
            size
        );
        // Clone the file handle Arc to release the file_handles lock before I/O.
        // This prevents slow disk I/O from blocking other file open/close operations.
        let file_arc = {
            let handles = self.file_handles.read();
            match handles.get(&fh) {
                Some(h) => Arc::clone(&h.file),
                None => {
                    tracing::warn!("read: file handle {} not found", fh);
                    reply.error(libc::EBADF);
                    return;
                }
            }
        };
        let mut file = file_arc.lock();

        READ_BUFFER.with(|buffer| {
            let mut buf = buffer.borrow_mut();

            let requested_size = size as usize;
            let current_capacity = buf.capacity();
            if current_capacity < requested_size {
                buf.reserve(requested_size - current_capacity);
            }
            buf.resize(requested_size, 0u8);

            match std::io::Seek::seek(&mut *file, std::io::SeekFrom::Start(offset as u64)) {
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("read: seek failed - {}", e);
                    reply.error(io_error_to_libc(&e));
                    return;
                }
            }

            match std::io::Read::read(&mut *file, &mut buf[..]) {
                Ok(n) => {
                    tracing::debug!("read: successfully read {} bytes", n);
                    reply.data(&buf[..n]);
                }
                Err(e) => {
                    tracing::error!("read: read failed - {}", e);
                    reply.error(io_error_to_libc(&e));
                }
            }
        });
    }

    fn write(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        tracing::debug!(
            "write(ino={}, fh={}, offset={}, len={})",
            ino,
            fh,
            offset,
            data.len()
        );

        let Some((rel_path, layer, _has_open)) = self.get_inode_info(ino) else {
            tracing::warn!("write: inode {} not found", ino);
            reply.error(libc::ENOENT);
            return;
        };

        tracing::debug!(
            "write: inode {} -> path={:?}, layer={:?}",
            ino,
            rel_path,
            layer
        );

        if layer == LayerType::Lower && !self.is_passthrough(&rel_path) {
            tracing::debug!("write: performing copy-up for lower layer file");
            if let Err(e) = self.copy_up(ino) {
                tracing::error!("write: copy-up failed with error {}", e);
                reply.error(e);
                return;
            }
        }

        // Clone the file handle Arc to release the file_handles lock before I/O.
        // This prevents slow disk I/O from blocking other file open/close operations.
        let file_arc = {
            let handles = self.file_handles.read();
            match handles.get(&fh) {
                Some(h) => Arc::clone(&h.file),
                None => {
                    reply.error(libc::EBADF);
                    return;
                }
            }
        };

        let mut file = file_arc.lock();
        match std::io::Seek::seek(&mut *file, std::io::SeekFrom::Start(offset as u64)) {
            Ok(_) => {}
            Err(e) => {
                tracing::error!("write: seek failed - {}", e);
                reply.error(io_error_to_libc(&e));
                return;
            }
        }

        match std::io::Write::write(&mut *file, data) {
            Ok(n) => {
                tracing::debug!("write: successfully wrote {} bytes", n);

                // Update the inode size incrementally based on the write offset and size
                // to avoid expensive stat() syscall on every write. The size is refreshed
                // from the filesystem on flush() for accuracy.
                {
                    let inodes = self.inodes.read();
                    if let Some(inode) = inodes.peek(ino) {
                        let new_size = std::cmp::max(inode.attrs.size, offset as u64 + n as u64);
                        drop(inodes);
                        self.inodes.write().update_size(ino, new_size);
                        tracing::debug!("write: updated inode {} size to {}", ino, new_size);
                    }
                }

                reply.written(n as u32);
            }
            Err(e) => {
                tracing::error!("write: write failed - {}", e);
                reply.error(io_error_to_libc(&e));
            }
        }
    }

    fn flush(&mut self, _req: &Request, ino: u64, fh: u64, _lock_owner: u64, reply: ReplyEmpty) {
        // Verify the file handle exists
        let handles = self.file_handles.read();
        if !handles.contains_key(&fh) {
            reply.error(libc::EBADF);
            return;
        }
        drop(handles);

        // Refresh inode attributes from the filesystem as defense-in-depth.
        // This ensures that even if a write path missed updating attributes,
        // the correct size will be visible after flush.

        if let Some((rel_path, layer, _has_open)) = self.get_inode_info(ino) {
            let is_passthrough = self.is_passthrough(&rel_path);
            if layer == LayerType::Upper || is_passthrough {
                let base = self.layer_base_path(layer);
                let actual_path = base.join(&rel_path);
                if let Ok(metadata) = fs::metadata(&actual_path) {
                    let new_attrs = metadata_to_fileattr(&metadata, ino);
                    self.inodes.write().update_attrs(ino, new_attrs);
                    tracing::trace!(
                        "flush: refreshed inode {} attrs, size={}",
                        ino,
                        new_attrs.size
                    );
                }

                // Signal that this file was flushed (a write completed)
                if layer == LayerType::Upper && !is_passthrough {
                    self.signal_mutation(&rel_path);
                }
            }
        };

        reply.ok();
    }

    fn release(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        self.file_handles.write().remove(&fh);

        // Check deleted status first, before acquiring inodes lock, to avoid
        // nested lock acquisition (which increases contention).
        let is_deleted = self.deleted.read().contains(&ino);

        let should_gc = {
            let mut inodes = self.inodes.write();
            if let Some(inode) = inodes.get_mut(ino) {
                inode.open_file_handles = inode.open_file_handles.saturating_sub(1);
                inode.hardlinks == 0 && inode.open_file_handles == 0
            } else {
                false
            }
        };

        if should_gc && is_deleted {
            self.do_gc(ino);
            self.deleted.write().remove(&ino);
        }

        reply.ok();
    }

    fn create(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        tracing::debug!(
            "create(parent={}, name={:?}, mode={:#o})",
            parent,
            name,
            mode
        );
        let (parent_path, is_passthrough) = {
            let inodes = self.inodes.read();
            let parent_inode = match inodes.peek(parent) {
                Some(i) => i.path.clone(),
                None => {
                    tracing::warn!("create: parent inode {} not found", parent);
                    reply.error(libc::ENOENT);
                    return;
                }
            };

            if inodes.lookup_child(parent, name).is_some() {
                tracing::debug!("create: file {:?} already exists", name);
                reply.error(libc::EEXIST);
                return;
            }

            let rel_path = parent_inode.join(name);
            let is_pt = self.is_passthrough(&rel_path);
            (parent_inode, is_pt)
        };

        let layer = if is_passthrough {
            LayerType::Lower
        } else {
            LayerType::Upper
        };
        let base_path = self.layer_base_path(layer);
        let child_path = base_path.join(&parent_path).join(name);

        tracing::debug!(
            "create: creating file at {} (layer={:?})",
            child_path.display(),
            layer
        );

        // The FUSE create() callback is specifically for creating regular files.
        // The mode parameter contains permission bits (e.g., 0644), not necessarily
        // the file type bits (S_IFREG). We should always create a regular file here.
        //
        // Note: mkdir() is used for directories, mknod() for special files.
        //
        // Extract only the permission bits from mode, ignoring any file type bits.
        // 0o7777 includes: setuid (4000), setgid (2000), sticky (1000), and rwxrwxrwx (777)
        let perm_mode = mode & 0o7777;

        // Create the file with the specified permissions
        if let Err(e) = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(perm_mode)
            .open(&child_path)
        {
            tracing::error!("create file error: {}", e);
            reply.error(io_error_to_libc(&e));
            return;
        }

        match fs::metadata(&child_path) {
            Ok(attrs) => {
                let new_ino = self.alloc_inode();
                let file_attrs = metadata_to_fileattr(&attrs, new_ino);
                let relative_path = parent_path.join(name);

                let inode = Self::create_inode_data(
                    new_ino,
                    parent,
                    name.to_os_string(),
                    layer,
                    relative_path.clone(),
                    file_attrs,
                );

                self.inodes.write().insert(inode);

                if !is_passthrough {
                    self.mutations
                        .write()
                        .insert(relative_path.clone(), MutationType::Created);

                    // Signal that a new file was created
                    self.signal_mutation(&relative_path);
                }

                let flags = libc::O_RDWR | libc::O_CREAT;
                match File::options().read(true).write(true).open(&child_path) {
                    Ok(file) => {
                        let fh = self.alloc_fh();
                        let handle = FileHandle {
                            file: Arc::new(Mutex::new(file)),
                        };
                        self.file_handles.write().insert(fh, handle);

                        {
                            let mut inodes = self.inodes.write();
                            if let Some(inode) = inodes.get_mut(new_ino) {
                                inode.open_file_handles += 1;
                            }
                        }

                        reply.created(&self.ttl, &file_attrs, 0, fh, flags as u32);
                    }
                    Err(e) => reply.error(io_error_to_libc(&e)),
                }
            }
            Err(e) => reply.error(io_error_to_libc(&e)),
        }
    }

    fn unlink(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let lookup_result = {
            let inodes = self.inodes.read();
            inodes.lookup_child(parent, name)
        };
        let ino = match lookup_result {
            Some(ino) => ino,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let Some((rel_path, layer, _has_open)) = self.get_inode_info(ino) else {
            reply.error(libc::ENOENT);
            return;
        };

        let is_passthrough = self.is_passthrough(&rel_path);

        if layer == LayerType::Lower && !is_passthrough {
            self.mutations
                .write()
                .insert(rel_path.clone(), MutationType::Deleted);
        }

        self.do_remove(parent, name, reply, std::fs::remove_file);
    }

    fn rmdir(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        self.do_remove(parent, name, reply, std::fs::remove_dir);
    }

    fn rename(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: ReplyEmpty,
    ) {
        let lookup_result = {
            let inodes = self.inodes.read();
            inodes.lookup_child(parent, name)
        };
        let ino = match lookup_result {
            Some(ino) => ino,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        {
            let inodes = self.inodes.read();
            if inodes.lookup_child(newparent, newname).is_some() {
                reply.error(libc::EEXIST);
                return;
            }
        }

        let Some((rel_path, _layer, _has_open)) = self.get_inode_info(ino) else {
            reply.error(libc::ENOENT);
            return;
        };

        let Some((new_parent_rel_path, _, _new_parent_has_open)) = self.get_inode_info(newparent)
        else {
            reply.error(libc::ENOENT);
            return;
        };

        let new_rel_path = new_parent_rel_path.join(newname);
        let src_is_passthrough = self.is_passthrough(&rel_path);
        let dest_is_passthrough = self.is_passthrough(&new_rel_path);

        // If either is passthrough, we operate on lower layer and skip COW
        if !src_is_passthrough && !dest_is_passthrough {
            if let Err(e) = self.copy_up(ino) {
                reply.error(e);
                return;
            }
        }

        // Get source and dest paths in one block to avoid borrow issues
        let (src_path, dest_path, new_parent_path, old_relative_path, actual_layer) = {
            let inodes = self.inodes.read();
            let inode = inodes.peek(ino);
            let newparent_inode = inodes.peek(newparent);

            match (inode, newparent_inode) {
                (Some(i), Some(np)) => {
                    let layer = if src_is_passthrough || dest_is_passthrough {
                        LayerType::Lower
                    } else {
                        LayerType::Upper
                    };
                    let base = self.layer_base_path(layer);
                    let src = base.join(&i.path);
                    let dest = base.join(&np.path).join(newname);
                    let parent_path = np.path.clone();
                    let old_rel = i.path.clone();
                    (src, dest, parent_path, old_rel, layer)
                }
                _ => {
                    reply.error(libc::ENOENT);
                    return;
                }
            }
        };

        if let Err(e) = fs::rename(&src_path, &dest_path) {
            tracing::error!("rename error: {}", e);
            reply.error(io_error_to_libc(&e));
            return;
        }

        let new_relative_path = new_parent_path.join(newname);

        {
            let mut inodes = self.inodes.write();
            inodes.remove_child(parent, name);
            inodes.add_child(newparent, newname.to_os_string(), ino);
            if let Some(inode) = inodes.get_mut(ino) {
                inode.path = new_relative_path.clone();
                inode.parent = newparent;
                inode.name = newname.to_os_string();
                inode.layer = actual_layer;
            }
        }

        if !src_is_passthrough && !dest_is_passthrough {
            // Signal both old and new paths for rename
            self.signal_mutation(&old_relative_path);
            self.signal_mutation(&new_relative_path);
        }

        reply.ok();
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        tracing::debug!("readdir(ino={}, offset={})", ino, offset);
        let Some((dir_path, _layer, _has_open)) = self.get_inode_info(ino) else {
            tracing::warn!("readdir: inode {} not found", ino);
            reply.error(libc::ENOENT);
            return;
        };

        tracing::debug!("readdir: dir_path={:?}", dir_path);
        let is_passthrough = self.is_passthrough(&dir_path);

        // Collect entries from both layers, keyed by name
        // Value is (inode_number, file_type, layer_type)
        let mut entries: HashMap<OsString, (u64, FileType, LayerType)> = HashMap::new();

        // Track whiteouts so we can filter them out from lower layer
        let mut whiteouts: HashSet<OsString> = HashSet::new();

        // Collect new inodes to batch-insert at the end (reduces lock contention)
        let mut new_inodes: Vec<InodeData> = Vec::new();

        // Track inodes that need layer updates from Lower to Upper
        let mut layer_updates: Vec<(u64, FileAttr)> = Vec::new();

        // Scan lower layer first, then upper layer (upper overwrites lower entries)
        let lower_dir = self.lower_layer.join(&dir_path);
        self.scan_directory_layer(
            &lower_dir,
            LayerType::Lower,
            ino,
            &dir_path,
            &mut entries,
            &mut whiteouts,
            &mut new_inodes,
            &mut layer_updates,
        );

        if !is_passthrough {
            let upper_dir = self.upper_layer.join(&dir_path);
            self.scan_directory_layer(
                &upper_dir,
                LayerType::Upper,
                ino,
                &dir_path,
                &mut entries,
                &mut whiteouts,
                &mut new_inodes,
                &mut layer_updates,
            );
        }

        // Batch insert all new inodes with a single write lock
        if !new_inodes.is_empty() || !layer_updates.is_empty() {
            let mut inodes = self.inodes.write();
            for inode in new_inodes {
                inodes.insert(inode);
            }
            for (ino, attrs) in layer_updates {
                if let Some(inode) = inodes.get_mut(ino) {
                    inode.layer = LayerType::Upper;
                    inode.attrs = attrs;
                }
            }
        }

        // Convert to sorted vector for consistent ordering
        let mut entries_vec: Vec<_> = entries.into_iter().collect();
        entries_vec.sort_by(|a, b| a.0.cmp(&b.0));

        tracing::debug!(
            "readdir: found {} entries: {:?}",
            entries_vec.len(),
            entries_vec.iter().map(|(name, _)| name).collect::<Vec<_>>()
        );

        // Return merged entries with offset handling
        let mut idx = 0;
        for (name, (child_ino, file_type, _layer)) in entries_vec {
            if idx < offset as usize {
                idx += 1;
                continue;
            }

            // Skip Linux overlayfs-style whiteouts (character device with rdev=0).
            // This is a defensive check for cross-platform compatibility; our primary
            // whiteout mechanism uses AUFS-style .wh.* prefix files.
            if file_type == FileType::CharDevice {
                let inodes = self.inodes.read();
                if let Some(inode) = inodes.peek(child_ino) {
                    if inode.attrs.rdev == 0 {
                        idx += 1;
                        continue;
                    }
                }
            }

            if reply.add(child_ino, (idx + 1) as i64, file_type, &name) {
                break;
            }
            idx += 1;
        }

        reply.ok();
    }

    fn mkdir(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let Some((parent_path, _layer, _has_open)) = self.get_inode_info(parent) else {
            reply.error(libc::ENOENT);
            return;
        };

        {
            let inodes = self.inodes.read();
            if inodes.lookup_child(parent, name).is_some() {
                reply.error(libc::EEXIST);
                return;
            }
        }

        let rel_path = parent_path.join(name);
        let is_passthrough = self.is_passthrough(&rel_path);

        let layer = if is_passthrough {
            LayerType::Lower
        } else {
            LayerType::Upper
        };
        let base_path = self.layer_base_path(layer);
        let child_path = base_path.join(&parent_path).join(name);

        if let Err(e) = fs::create_dir(&child_path) {
            tracing::error!("mkdir error: {}", e);
            reply.error(io_error_to_libc(&e));
            return;
        }

        match fs::metadata(&child_path) {
            Ok(attrs) => {
                let new_ino = self.alloc_inode();
                let file_attrs = metadata_to_fileattr(&attrs, new_ino);
                let relative_path = parent_path.join(name);

                let inode = Self::create_inode_data(
                    new_ino,
                    parent,
                    name.to_os_string(),
                    layer,
                    relative_path.clone(),
                    file_attrs,
                );

                self.inodes.write().insert(inode);

                if !is_passthrough {
                    // Signal that a new directory was created
                    self.signal_mutation(&relative_path);
                }

                reply.entry(&self.ttl, &file_attrs, 0);
            }
            Err(e) => reply.error(io_error_to_libc(&e)),
        }
    }

    fn symlink(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        link: &std::path::Path,
        reply: ReplyEntry,
    ) {
        let Some((parent_path, _layer, _has_open)) = self.get_inode_info(parent) else {
            reply.error(libc::ENOENT);
            return;
        };

        {
            let inodes = self.inodes.read();
            if inodes.lookup_child(parent, name).is_some() {
                reply.error(libc::EEXIST);
                return;
            }
        }

        let child_path = self.upper_layer.join(&parent_path).join(name);

        if let Err(e) = std::os::unix::fs::symlink(link, &child_path) {
            tracing::error!("symlink error: {}", e);
            reply.error(io_error_to_libc(&e));
            return;
        }

        match fs::symlink_metadata(&child_path) {
            Ok(attrs) => {
                let new_ino = self.alloc_inode();
                let file_attrs = metadata_to_fileattr(&attrs, new_ino);
                let relative_path = parent_path.join(name);

                let inode = Self::create_inode_data(
                    new_ino,
                    parent,
                    name.to_os_string(),
                    LayerType::Upper,
                    relative_path,
                    file_attrs,
                );

                self.inodes.write().insert(inode);
                reply.entry(&self.ttl, &file_attrs, 0);
            }
            Err(e) => reply.error(io_error_to_libc(&e)),
        }
    }

    fn readlink(&mut self, _req: &Request, ino: u64, reply: ReplyData) {
        let Some((rel_path, layer, _has_open)) = self.get_inode_info(ino) else {
            reply.error(libc::ENOENT);
            return;
        };

        let path = self.layer_base_path(layer).join(&rel_path);

        match fs::read_link(&path) {
            Ok(target) => {
                if let Some(os_str) = target.as_os_str().to_str() {
                    let bytes = os_str.as_bytes();
                    reply.data(bytes);
                } else {
                    // Non-UTF8 symlink target - this is valid but rare
                    reply.error(libc::EINVAL);
                }
            }
            Err(e) => reply.error(io_error_to_libc(&e)),
        }
    }

    fn link(
        &mut self,
        _req: &Request,
        ino: u64,
        newparent: u64,
        newname: &OsStr,
        reply: ReplyEntry,
    ) {
        // Check if target doesn't already exist
        {
            let inodes = self.inodes.read();
            if inodes.lookup_child(newparent, newname).is_some() {
                reply.error(libc::EEXIST);
                return;
            }
        }

        let Some((_path, layer, _has_open)) = self.get_inode_info(ino) else {
            reply.error(libc::ENOENT);
            return;
        };

        // COW if needed (copy source file to upper layer)
        if layer == LayerType::Lower {
            if let Err(e) = self.copy_up(ino) {
                reply.error(e);
                return;
            }
        }

        // Get the actual path in upper layer and new parent path
        let (src_actual_path, dest_actual_path) = {
            let inodes = self.inodes.read();
            let src_inode = inodes.peek(ino);
            let newparent_inode = inodes.peek(newparent);

            match (src_inode, newparent_inode) {
                (Some(s), Some(np)) => {
                    let src = self.upper_layer.join(&s.path);
                    let dest = self.upper_layer.join(&np.path).join(newname);
                    (src, dest)
                }
                _ => {
                    reply.error(libc::ENOENT);
                    return;
                }
            }
        };

        // Create the hard link on the filesystem
        if let Err(e) = fs::hard_link(&src_actual_path, &dest_actual_path) {
            tracing::error!("link error: {}", e);
            reply.error(io_error_to_libc(&e));
            return;
        }

        // Create directory entry pointing to SAME inode (reuse existing ino)
        // and increment hardlink count
        {
            let mut inodes = self.inodes.write();
            inodes.add_child(newparent, newname.to_os_string(), ino);
            if let Some(inode) = inodes.get_mut(ino) {
                inode.hardlinks += 1;
                inode.attrs.nlink += 1; // Update the cached nlink so getattr returns correct value
                reply.entry(&self.ttl, &inode.attrs, 0);
                return;
            }
        }

        reply.error(libc::ENOENT);
    }

    fn setxattr(
        &mut self,
        _req: &Request,
        ino: u64,
        name: &OsStr,
        value: &[u8],
        _flags: i32,
        _position: u32,
        reply: ReplyEmpty,
    ) {
        // Copy-up if file is in lower layer
        if let Err(e) = self.copy_up(ino) {
            reply.error(e);
            return;
        }

        let Some((rel_path, _layer, _has_open)) = self.get_inode_info(ino) else {
            reply.error(libc::ENOENT);
            return;
        };

        let path = self.upper_layer.join(&rel_path);

        match xattr::set(&path, name, value) {
            Ok(_) => reply.ok(),
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    fn getxattr(&mut self, _req: &Request, ino: u64, name: &OsStr, size: u32, reply: ReplyXattr) {
        let Some((rel_path, layer, _has_open)) = self.get_inode_info(ino) else {
            reply.error(libc::ENOENT);
            return;
        };

        let path = self.layer_base_path(layer).join(&rel_path);

        match xattr::get(&path, name) {
            Ok(Some(value)) => {
                if size == 0 {
                    reply.size(value.len() as u32);
                } else if size >= value.len() as u32 {
                    reply.data(&value);
                } else {
                    reply.error(libc::ERANGE);
                }
            }
            Ok(None) => {
                // Attribute not found - use ENOATTR on macOS (same as ENODATA on Linux)
                #[cfg(target_os = "macos")]
                reply.error(libc::ENOATTR);
                #[cfg(not(target_os = "macos"))]
                reply.error(libc::ENODATA);
            }
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    fn listxattr(&mut self, _req: &Request, ino: u64, size: u32, reply: ReplyXattr) {
        let Some((rel_path, layer, _has_open)) = self.get_inode_info(ino) else {
            reply.error(libc::ENOENT);
            return;
        };

        let path = self.layer_base_path(layer).join(&rel_path);

        match xattr::list(&path) {
            Ok(attrs) => {
                let mut data = Vec::new();
                for attr in attrs {
                    data.extend_from_slice(attr.as_bytes());
                    data.push(0); // null separator
                }

                if size == 0 {
                    reply.size(data.len() as u32);
                } else if size >= data.len() as u32 {
                    reply.data(&data);
                } else {
                    reply.error(libc::ERANGE);
                }
            }
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    fn removexattr(&mut self, _req: &Request, ino: u64, name: &OsStr, reply: ReplyEmpty) {
        // Copy-up if file is in lower layer
        if let Err(e) = self.copy_up(ino) {
            reply.error(e);
            return;
        }

        let Some((rel_path, _layer, _has_open)) = self.get_inode_info(ino) else {
            reply.error(libc::ENOENT);
            return;
        };

        let path = self.upper_layer.join(&rel_path);

        match xattr::remove(&path, name) {
            Ok(_) => reply.ok(),
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }
}
