//! The files the sensors are read from, as this library reaches them: the
//! computer's own, or, in the Sensor Feed plugin, Ectomatter's, which lends only
//! what the plugin's manifest names to read, read only. The calls are the
//! standard library's that the listing and the reading use, so the rest of
//! this library reads as if it reached the files itself.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// What reaches the files: a file's text, the names in a folder, and where a
/// path leads once its links are followed.
pub trait Files: Send + Sync {
    fn read(&self, path: &Path) -> io::Result<String>;
    fn entries(&self, path: &Path) -> io::Result<Vec<String>>;
    fn resolve(&self, path: &Path) -> io::Result<PathBuf>;
}

/// The computer's own files, as the standard library reaches them.
pub struct Own;

impl Files for Own {
    fn read(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }
    fn entries(&self, path: &Path) -> io::Result<Vec<String>> {
        let mut names: Vec<String> = std::fs::read_dir(path)?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        Ok(names)
    }
    fn resolve(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

static FILES: OnceLock<Box<dyn Files>> = OnceLock::new();

/// Reach the files through `f` from now on (once, as the plugin starts).
pub fn reach_through(f: Box<dyn Files>) {
    let _ = FILES.set(f);
}

fn files() -> &'static dyn Files {
    FILES.get().map_or(&Own, |f| f.as_ref())
}

/// A file's text.
pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    files().read(path.as_ref())
}

/// A folder's entries, each with its path and its name, in their names' order.
pub fn read_dir(path: impl AsRef<Path>) -> io::Result<std::vec::IntoIter<io::Result<Entry>>> {
    let dir = path.as_ref();
    let names = files().entries(dir)?;
    Ok(names.into_iter().map(|n| Ok(Entry(dir.join(n)))).collect::<Vec<_>>().into_iter())
}

/// Where a path leads, its links followed.
pub fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    files().resolve(path.as_ref())
}

/// Whether there's anything at a path.
pub fn exists(path: impl AsRef<Path>) -> bool {
    files().resolve(path.as_ref()).is_ok()
}

/// One of a folder's entries.
pub struct Entry(PathBuf);

impl Entry {
    pub fn path(&self) -> PathBuf {
        self.0.clone()
    }
    pub fn file_name(&self) -> std::ffi::OsString {
        self.0.file_name().map(|n| n.to_os_string()).unwrap_or_default()
    }
}
