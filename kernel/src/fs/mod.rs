pub mod fat32;

use fat32::{DirCursor, DirEntry, Fat32State, FsError, OpenFile};
use core::cell::UnsafeCell;
use crate::storage;

const MAX_OPEN: usize = 16;

struct FsState {
    fat32: Fat32State,
    files: [OpenFile; MAX_OPEN],
}

struct FsCell(UnsafeCell<FsState>);
// SAFETY: Single-task access enforced by caller (shell task only).
unsafe impl Sync for FsCell {}

static FS: FsCell = FsCell(UnsafeCell::new(FsState {
    fat32: Fat32State::empty(),
    files: [const { OpenFile::empty() }; MAX_OPEN],
}));

fn state() -> &'static mut FsState {
    // SAFETY: Single-task access — only shell/boot code calls fs functions.
    unsafe { &mut *FS.0.get() }
}

pub fn init() -> Result<(), FsError> {
    let (part_lba, _part_size) = storage::find_fat32_partition()
        .ok_or(FsError::NotMounted)?;
    let s = state();
    fat32::mount(&mut s.fat32, part_lba)?;
    Ok(())
}

pub fn is_mounted() -> bool {
    state().fat32.mounted
}

pub fn cluster_count() -> u32 {
    state().fat32.cluster_count
}

fn alloc_fd(s: &mut FsState) -> Result<usize, FsError> {
    for i in 0..MAX_OPEN {
        if !s.files[i].active {
            return Ok(i);
        }
    }
    Err(FsError::TooManyOpen)
}

pub fn open(path: &str, writable: bool) -> Result<usize, FsError> {
    let s = state();
    if !s.fat32.mounted {
        return Err(FsError::NotMounted);
    }

    let (cluster, size, is_dir, parent_cluster, entry_idx) =
        fat32::resolve_path(&s.fat32, path)?;

    if is_dir {
        return Err(FsError::IsADirectory);
    }

    if writable && (s.fat32.bpb.bytes_per_sector == 0) {
        return Err(FsError::ReadOnly);
    }

    let fd = alloc_fd(s)?;
    s.files[fd] = OpenFile {
        active: true,
        cluster_start: cluster,
        size,
        position: 0,
        is_dir,
        writable,
        dir_cluster: parent_cluster,
        dir_entry_idx: entry_idx,
        ..OpenFile::empty()
    };
    s.files[fd].cur_cluster = cluster;
    s.files[fd].cur_cluster_offset = 0;

    Ok(fd)
}

pub fn create(path: &str) -> Result<usize, FsError> {
    let s = state();
    if !s.fat32.mounted {
        return Err(FsError::NotMounted);
    }

    // Split path into parent dir and filename
    let path = path.trim_start_matches('/');
    let (dir_path, filename) = match path.rfind('/') {
        Some(pos) => (&path[..pos], &path[pos + 1..]),
        None => ("", path),
    };

    if filename.is_empty() {
        return Err(FsError::InvalidName);
    }

    // Check if file already exists
    let full_path = if dir_path.is_empty() {
        path
    } else {
        path
    };
    if fat32::resolve_path(&s.fat32, full_path).is_ok() {
        // File exists — open it for writing
        let (cluster, size, _, parent_cluster, entry_idx) =
            fat32::resolve_path(&s.fat32, full_path)?;

        let fd = alloc_fd(s)?;
        s.files[fd] = OpenFile {
            active: true,
            cluster_start: cluster,
            size,
            position: 0,
            is_dir: false,
            writable: true,
            dir_cluster: parent_cluster,
            dir_entry_idx: entry_idx,
            ..OpenFile::empty()
        };
        s.files[fd].cur_cluster = cluster;
        s.files[fd].cur_cluster_offset = 0;
        return Ok(fd);
    }

    // Resolve parent directory
    let dir_cluster = if dir_path.is_empty() {
        s.fat32.bpb.root_cluster
    } else {
        let (cluster, _, is_dir, _, _) = fat32::resolve_path(&s.fat32, dir_path)?;
        if !is_dir {
            return Err(FsError::NotADirectory);
        }
        cluster
    };

    // Create the file
    let (file_cluster, entry_idx) = fat32::create_file(&s.fat32, dir_cluster, filename)?;

    let fd = alloc_fd(s)?;
    s.files[fd] = OpenFile {
        active: true,
        cluster_start: file_cluster,
        size: 0,
        position: 0,
        is_dir: false,
        writable: true,
        dir_cluster,
        dir_entry_idx: entry_idx,
        ..OpenFile::empty()
    };
    s.files[fd].cur_cluster = file_cluster;
    s.files[fd].cur_cluster_offset = 0;

    Ok(fd)
}

pub fn close(fd: usize) -> Result<(), FsError> {
    let s = state();
    if fd >= MAX_OPEN || !s.files[fd].active {
        return Err(FsError::BadFd);
    }

    if s.files[fd].writable {
        fat32::update_dir_entry_size(
            &s.fat32,
            s.files[fd].dir_cluster,
            s.files[fd].dir_entry_idx,
            s.files[fd].size,
            s.files[fd].cluster_start,
        )?;
        storage::flush()?;
    }

    s.files[fd].active = false;
    Ok(())
}

pub fn read(fd: usize, buf: &mut [u8]) -> Result<usize, FsError> {
    let s = state();
    if fd >= MAX_OPEN || !s.files[fd].active {
        return Err(FsError::BadFd);
    }
    fat32::read_file(&s.fat32, &mut s.files[fd], buf)
}

pub fn write(fd: usize, buf: &[u8]) -> Result<usize, FsError> {
    let s = state();
    if fd >= MAX_OPEN || !s.files[fd].active {
        return Err(FsError::BadFd);
    }
    fat32::write_file(&s.fat32, &mut s.files[fd], buf)
}

pub fn stat(path: &str) -> Result<fat32::DirEntry, FsError> {
    let s = state();
    if !s.fat32.mounted {
        return Err(FsError::NotMounted);
    }

    let (cluster, size, is_dir, _, _) = fat32::resolve_path(&s.fat32, path)?;
    let mut entry = DirEntry::empty();
    entry.cluster = cluster;
    entry.size = size;
    entry.is_dir = is_dir;
    // Name not populated for stat — caller already knows the path
    Ok(entry)
}

// --- Directory iteration ---

static mut DIR_CURSORS: [Option<DirCursor>; MAX_OPEN] = [None; MAX_OPEN];

pub fn readdir_open(path: &str) -> Result<usize, FsError> {
    let s = state();
    if !s.fat32.mounted {
        return Err(FsError::NotMounted);
    }

    let cluster = if path == "/" || path.is_empty() {
        s.fat32.bpb.root_cluster
    } else {
        let (c, _, is_dir, _, _) = fat32::resolve_path(&s.fat32, path)?;
        if !is_dir {
            return Err(FsError::NotADirectory);
        }
        c
    };

    let fd = alloc_fd(s)?;
    s.files[fd] = OpenFile {
        active: true,
        cluster_start: cluster,
        size: 0,
        position: 0,
        is_dir: true,
        writable: false,
        dir_cluster: 0,
        dir_entry_idx: 0,
        ..OpenFile::empty()
    };

    // SAFETY: Single-task access.
    unsafe { DIR_CURSORS[fd] = Some(DirCursor::new(cluster)) };

    Ok(fd)
}

pub fn readdir_next(fd: usize) -> Result<Option<DirEntry>, FsError> {
    let s = state();
    if fd >= MAX_OPEN || !s.files[fd].active {
        return Err(FsError::BadFd);
    }

    // SAFETY: Single-task access.
    let cursor = unsafe {
        DIR_CURSORS[fd].as_mut().ok_or(FsError::BadFd)?
    };

    fat32::readdir_next(&s.fat32, cursor)
}

pub fn readdir_close(fd: usize) -> Result<(), FsError> {
    let s = state();
    if fd >= MAX_OPEN || !s.files[fd].active {
        return Err(FsError::BadFd);
    }
    s.files[fd].active = false;
    // SAFETY: Single-task access.
    unsafe { DIR_CURSORS[fd] = None };
    Ok(())
}
