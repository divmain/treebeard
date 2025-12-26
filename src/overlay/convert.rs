use fuser::{FileAttr, FileType};
use libc::S_IFMT;
use std::fs::Metadata;
use std::io;
use std::os::unix::prelude::MetadataExt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Map std::io::Error to appropriate libc error code.
/// This provides more specific error codes than generic EIO for better debugging.
pub(crate) fn io_error_to_libc(e: &io::Error) -> i32 {
    // First, try to get the raw OS error code if available - this is the most accurate
    if let Some(code) = e.raw_os_error() {
        return code;
    }

    // Fall back to mapping stable ErrorKind variants
    match e.kind() {
        io::ErrorKind::NotFound => libc::ENOENT,
        io::ErrorKind::PermissionDenied => libc::EACCES,
        io::ErrorKind::AlreadyExists => libc::EEXIST,
        io::ErrorKind::InvalidInput => libc::EINVAL,
        io::ErrorKind::InvalidData => libc::EINVAL,
        io::ErrorKind::TimedOut => libc::ETIMEDOUT,
        io::ErrorKind::Interrupted => libc::EINTR,
        io::ErrorKind::WriteZero => libc::ENOSPC,
        io::ErrorKind::OutOfMemory => libc::ENOMEM,
        io::ErrorKind::BrokenPipe => libc::EPIPE,
        io::ErrorKind::WouldBlock => libc::EAGAIN,
        // Other stable variants that don't have direct mappings
        io::ErrorKind::UnexpectedEof => libc::EIO,
        io::ErrorKind::Unsupported => libc::ENOTSUP,
        io::ErrorKind::AddrInUse => libc::EADDRINUSE,
        io::ErrorKind::AddrNotAvailable => libc::EADDRNOTAVAIL,
        io::ErrorKind::ConnectionRefused => libc::ECONNREFUSED,
        io::ErrorKind::ConnectionReset => libc::ECONNRESET,
        io::ErrorKind::ConnectionAborted => libc::ECONNABORTED,
        io::ErrorKind::NotConnected => libc::ENOTCONN,
        _ => libc::EIO,
    }
}

pub(crate) fn metadata_to_filetype(meta: &Metadata) -> FileType {
    let file_type = meta.mode();
    match file_type & (S_IFMT as u32) {
        x if x == libc::S_IFREG as u32 => FileType::RegularFile,
        x if x == libc::S_IFDIR as u32 => FileType::Directory,
        x if x == libc::S_IFLNK as u32 => FileType::Symlink,
        x if x == libc::S_IFBLK as u32 => FileType::BlockDevice,
        x if x == libc::S_IFCHR as u32 => FileType::CharDevice,
        x if x == libc::S_IFIFO as u32 => FileType::NamedPipe,
        x if x == libc::S_IFSOCK as u32 => FileType::Socket,
        _ => FileType::RegularFile,
    }
}

/// Convert std::fs::FileType to fuser FileType.
/// This is more efficient than metadata_to_filetype() when called on DirEntry::file_type()
/// because it doesn't require a full stat syscall on most filesystems.
pub(crate) fn std_filetype_to_fuser(ft: std::fs::FileType) -> FileType {
    if ft.is_file() {
        FileType::RegularFile
    } else if ft.is_dir() {
        FileType::Directory
    } else if ft.is_symlink() {
        FileType::Symlink
    } else {
        // For block/char devices, named pipes, and sockets, we need to fall back
        // to metadata. Return RegularFile as a placeholder - callers should
        // check for these cases and use metadata if needed.
        FileType::RegularFile
    }
}

pub(crate) fn metadata_to_fileattr(meta: &Metadata, ino: u64) -> FileAttr {
    let kind = metadata_to_filetype(meta);

    let atime = meta.accessed().unwrap_or(UNIX_EPOCH);

    let mtime = meta.modified().unwrap_or(UNIX_EPOCH);

    let ctime = meta.created().unwrap_or(UNIX_EPOCH);

    FileAttr {
        ino,
        size: meta.len(),
        blocks: meta.blocks(),
        atime,
        mtime,
        ctime,
        crtime: SystemTime::UNIX_EPOCH,
        kind,
        perm: (meta.mode() & 0o777) as u16,
        nlink: meta.nlink() as u32,
        uid: meta.uid(),
        gid: meta.gid(),
        rdev: meta.rdev() as u32,
        blksize: meta.blksize() as u32,
        flags: 0,
    }
}
