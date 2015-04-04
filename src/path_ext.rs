use std::io;
use std::fs::{metadata, Metadata};
use std::path::Path;

pub trait PathExt {
    /// Get information on the file, directory, etc at this path.
    ///
    /// Consult the `fs::stat` documentation for more info.
    ///
    /// This call preserves identical runtime/error semantics with `file::stat`.
    fn metadata(&self) -> io::Result<Metadata>;

    /// Boolean value indicator whether the underlying file exists on the local
    /// filesystem. Returns false in exactly the cases where `fs::stat` fails.
    fn exists(&self) -> bool;

    /// Whether the underlying implementation (be it a file path, or something
    /// else) points at a "regular file" on the FS. Will return false for paths
    /// to non-existent locations or directories or other non-regular files
    /// (named pipes, etc). Follows links when making this determination.
    fn is_file(&self) -> bool;

    /// Whether the underlying implementation (be it a file path, or something
    /// else) is pointing at a directory in the underlying FS. Will return
    /// false for paths to non-existent locations or if the item is not a
    /// directory (eg files, named pipes, etc). Follows links when making this
    /// determination.
    fn is_dir(&self) -> bool;
}

impl PathExt for Path {
    fn metadata(&self) -> io::Result<Metadata> { metadata(self) }

    fn exists(&self) -> bool { metadata(self).is_ok() }

    fn is_file(&self) -> bool {
        metadata(self).map(|s| s.is_file()).unwrap_or(false)
    }
    fn is_dir(&self) -> bool {
        metadata(self).map(|s| s.is_dir()).unwrap_or(false)
    }
}
