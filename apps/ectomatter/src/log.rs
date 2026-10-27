//! Ectomatter's log: everything it, its libraries and its packages write to
//! stderr goes into a file in the person's state folder
//! (`$XDG_STATE_HOME/com.ectonomic.ectomatter/ectomatter.log`), the run
//! before's kept beside it as `ectomatter.log.1`. The terminal Ectomatter was
//! started from is told where the log is, once, and gets nothing more.
//!
//! The file is stderr itself (its descriptor put in stderr's place), not a
//! copy of it: nothing is held back to be lost when Ectomatter quits, and a
//! program Ectomatter starts that keeps running after it (a game) writes its
//! own errors into the same file rather than into a pipe gone with Ectomatter.

use std::path::PathBuf;

/// The folder the log is kept in.
pub fn folder() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"))
        .join("com.ectonomic.ectomatter")
}

/// The file the log is written to.
pub fn file() -> PathBuf {
    folder().join("ectomatter.log")
}

/// Starts the log: the last run's moved aside, and stderr from here on the
/// log's file. Says why on stderr as it was when it can't.
pub fn start() {
    let path = file();
    if let Err(e) = std::fs::create_dir_all(folder()) {
        eprintln!("ectomatter: no log, its folder {} can't be made: {e}", folder().display());
        return;
    }
    let _ = std::fs::rename(&path, path.with_extension("log.1"));
    let file = match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("ectomatter: no log, {} can't be written: {e}", path.display());
            return;
        }
    };
    eprintln!("ectomatter: its log is {}", path.display());
    if let Err(e) = become_stderr(file) {
        eprintln!("ectomatter: no log, stderr can't be the log's file: {e}");
    }
}

#[cfg(unix)]
fn become_stderr(file: std::fs::File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    // (the file's own descriptor is closed when `file` goes; stderr's copy stays)
    if unsafe { libc::dup2(file.as_raw_fd(), 2) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(unix))]
fn become_stderr(_file: std::fs::File) -> std::io::Result<()> {
    // (deferred, stage 5: on other systems stderr isn't the log's file yet)
    Err(std::io::Error::other("not on this system yet"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_is_kept_in_the_state_folder() {
        assert!(file().ends_with("com.ectonomic.ectomatter/ectomatter.log"));
    }
}
