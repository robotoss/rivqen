// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The directory layout of ADR-004 section 1 and the file operations of the
//! driver. Every function here gives a [`StoreError`] with a static
//! operation name only: no path, no file id.
//!
//! ```text
//! <root>/schema-1/meta.sqlite
//! <root>/schema-1/files/<hh>/<file-id>.rqf
//! <root>/schema-1/tmp/<file-id>.tmp
//! ```

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use rivqen_cache::driver::StoreError;
use rivqen_cache::txn::SCHEMA_VERSION;

use super::{corrupted, unavailable};
use crate::crypto::FileId;

/// The directory of the entry files, in the schema directory.
pub const FILES_DIR: &str = "files";
/// The directory of the files that are not promoted yet, in the schema
/// directory.
pub const TMP_DIR: &str = "tmp";
/// The extension of an entry file.
pub const FILE_EXT: &str = "rqf";
/// The extension of a temporary file.
pub const TEMP_EXT: &str = "tmp";
/// The prefix of the name of a schema directory (`schema-<version>`).
pub const SCHEMA_PREFIX: &str = "schema-";

/// The schema directory of the store at `root`: `<root>/schema-1` for
/// `rivqen_cache::txn::SCHEMA_VERSION` 1.
#[must_use]
pub fn schema_dir(root: &Path) -> PathBuf {
    root.join(format!("{SCHEMA_PREFIX}{SCHEMA_VERSION}"))
}

/// The path of the entry file of `id` in the store at `root`:
/// `<root>/schema-1/files/<hh>/<file-id>.rqf`, where `<hh>` is the hex
/// text of the first byte of the id.
#[must_use]
pub fn entry_file_path(root: &Path, id: &FileId) -> PathBuf {
    Layout::new(root).file_path(id)
}

/// The paths of one store.
#[derive(Clone)]
pub(super) struct Layout {
    pub(super) root: PathBuf,
    pub(super) schema: PathBuf,
    pub(super) files: PathBuf,
    pub(super) tmp: PathBuf,
}

impl Layout {
    pub(super) fn new(root: &Path) -> Self {
        let schema = schema_dir(root);
        Self {
            root: root.to_path_buf(),
            files: schema.join(FILES_DIR),
            tmp: schema.join(TMP_DIR),
            schema,
        }
    }

    /// `tmp/<file-id>.tmp`.
    pub(super) fn temp_path(&self, id: &FileId) -> PathBuf {
        self.tmp.join(format!("{}.{TEMP_EXT}", id.to_hex()))
    }

    /// `files/<hh>`.
    pub(super) fn bucket(&self, id: &FileId) -> PathBuf {
        self.files.join(bucket_name(id))
    }

    /// `files/<hh>/<file-id>.rqf`.
    pub(super) fn file_path(&self, id: &FileId) -> PathBuf {
        self.bucket(id).join(format!("{}.{FILE_EXT}", id.to_hex()))
    }
}

/// `<hh>`: the first two hex characters of the id.
pub(super) fn bucket_name(id: &FileId) -> String {
    let hex = id.to_hex();
    hex.get(..2).unwrap_or_default().to_owned()
}

/// The file id of an entry file name `<file-id>.rqf` in the bucket
/// `bucket`, or `None` for any other name (also an id of another bucket).
pub(super) fn parse_file_name(bucket: &str, name: &str) -> Option<FileId> {
    let stem = name.strip_suffix(FILE_EXT)?.strip_suffix('.')?;
    let id = FileId::from_hex(stem)?;
    (bucket_name(&id) == bucket).then_some(id)
}

/// True for a bucket name: two lower-case hex characters.
pub(super) fn is_bucket_name(name: &str) -> bool {
    name.len() == 2
        && name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// True for the name of the directory of another schema version:
/// `schema-` and decimal digits, not the current one.
pub(super) fn is_other_schema_name(name: &str, current: &Path) -> bool {
    let is_schema = name
        .strip_prefix(SCHEMA_PREFIX)
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()));
    is_schema && current.file_name().is_none_or(|own| own != name)
}

/// Makes the entries of the directory `dir` durable: `sync_all` on the
/// directory (`F_FULLFSYNC` on Apple targets).
pub(super) fn sync_dir(dir: &Path, op: &'static str) -> Result<(), StoreError> {
    File::open(dir)
        .and_then(|handle| handle.sync_all())
        .map_err(|_| unavailable(op))
}

/// Creates the directory `dir` when it does not exist and then syncs its
/// parent, so the new entry is durable. An existing entry that is not a
/// directory (also a symbolic link) is refused.
pub(super) fn ensure_dir(dir: &Path, op: &'static str) -> Result<(), StoreError> {
    let missing = match fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() => return Ok(()),
        Ok(_) => false,
        Err(error) => error.kind() == ErrorKind::NotFound,
    };
    if !missing {
        return Err(unavailable(op));
    }
    match fs::create_dir(dir) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => return Ok(()),
        Err(_) => return Err(unavailable(op)),
    }
    sync_dir(&parent_of(dir), op)
}

/// The parent directory of `path`; `.` for a relative path of one part.
fn parent_of(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Writes `bytes` to a new file at `path`. The file must not exist
/// (`create_new`), so no file is ever written twice. Gives the open file
/// for the sync of the next step.
pub(super) fn write_new(path: &Path, bytes: &[u8], op: &'static str) -> Result<File, StoreError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // The entry is sealed; the mode only keeps other users out.
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| unavailable(op))?;
    file.write_all(bytes).map_err(|_| unavailable(op))?;
    Ok(file)
}

/// Removes the file at `path`. A file that is gone already is not an
/// error: the goal of the call holds.
pub(super) fn remove_file(path: &Path, op: &'static str) -> Result<(), StoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(_) => Err(unavailable(op)),
    }
}

/// Removes the entry at `path` without following a symbolic link: a
/// directory with its content, any other entry as a file.
pub(super) fn remove_entry(path: &Path, is_dir: bool, op: &'static str) -> Result<(), StoreError> {
    let removed = if is_dir {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match removed {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(_) => Err(unavailable(op)),
    }
}

/// One entry of a directory listing: its name (when it is UTF-8), its path
/// and its type (a symbolic link is not followed).
pub(super) struct DirEntry {
    pub(super) name: Option<String>,
    pub(super) path: PathBuf,
    pub(super) is_dir: bool,
    pub(super) is_file: bool,
}

/// The entries of the directory `dir`. A directory that does not exist
/// has no entries.
pub(super) fn list_dir(dir: &Path, op: &'static str) -> Result<Vec<DirEntry>, StoreError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(unavailable(op)),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| unavailable(op))?;
        let path = entry.path();
        // `symlink_metadata`: a link is listed as a link, never followed.
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(_) => return Err(unavailable(op)),
        };
        out.push(DirEntry {
            name: entry.file_name().into_string().ok(),
            path,
            is_dir: meta.is_dir(),
            is_file: meta.is_file(),
        });
    }
    Ok(out)
}

/// The length of the regular file at `path`: `None` when there is no
/// entry, or when the entry is not a regular file (a link is not
/// followed).
pub(super) fn file_len(path: &Path, op: &'static str) -> Result<Option<u64>, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => Ok(Some(meta.len())),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(_) => Err(unavailable(op)),
    }
}

/// Reads the entry file at `path` when its length passes `fits`.
///
/// Gives `Corrupted` when the file is missing, is not a regular file, has a
/// length that `fits` refuses, or changes its length while it is read;
/// `Unavailable` for any other I/O failure. The read never allocates more
/// than the length that `fits` accepted.
pub(super) fn read_entry_file(
    path: &Path,
    fits: impl FnOnce(u64) -> bool,
    op: &'static str,
) -> Result<Vec<u8>, StoreError> {
    let Some(len) = file_len(path, op)? else {
        return Err(corrupted(op));
    };
    if !fits(len) {
        return Err(corrupted(op));
    }
    let capacity = usize::try_from(len).map_err(|_| corrupted(op))?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Err(corrupted(op)),
        Err(_) => return Err(unavailable(op)),
    };
    let mut bytes = Vec::with_capacity(capacity);
    // One byte more than the length: a file that grew is found.
    file.take(len.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error: io::Error| match error.kind() {
            ErrorKind::NotFound => corrupted(op),
            _ => unavailable(op),
        })?;
    if bytes.len() == capacity {
        Ok(bytes)
    } else {
        Err(corrupted(op))
    }
}
