//! Ectomatter's own files (its settings, the Library's choices, its macros,
//! the mouse's profiles, the window's state, what it puts back): written
//! whole and safely, and read so that one that can't be read is kept, not
//! lost.
//!
//! A file is written beside itself under a name of this process's own, made
//! to reach the disk, then moved over the old one, and the move made to
//! reach the disk too: a crash or a power cut leaves the old file or the new
//! one, never half of either, and two Ectomatters writing at once don't
//! write into each other's. A file that's there but isn't JSON (damaged, or
//! changed by hand and broken) is moved aside as `<name>.damaged-<time>` and
//! read as empty, so the next write doesn't take it away: the person can
//! still look at it, and the log says where it went.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Write `bytes` as the whole of `path`, safely (above).
pub fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let written = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return written;
    }
    // (the move itself on the disk: the folder synced, where the system lets
    // a folder be opened to be)
    #[cfg(unix)]
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// `value` written as `path`'s pretty JSON, safely.
pub fn write_json(path: &Path, value: &Value) -> std::io::Result<()> {
    write_whole(path, &serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?)
}

/// What `path` holds as JSON: none where there's no file; and where there's
/// one that isn't JSON, none too, the file moved aside first (above).
pub fn read_json(path: &Path) -> Option<Value> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(v) => Some(v),
        // (a snapshot or a test changes none of the person's files)
        Err(e) if crate::desk::offscreen() => {
            eprintln!("ectomatter: {} couldn’t be read ({e}); left as it is", path.display());
            None
        }
        Err(e) => {
            let kept = damaged_name(path);
            match std::fs::rename(path, &kept) {
                Ok(()) => eprintln!("ectomatter: {} couldn’t be read ({e}); it’s kept as {}, and Ectomatter starts it afresh", path.display(), kept.display()),
                Err(m) => eprintln!("ectomatter: {} couldn’t be read ({e}), nor kept aside ({m})", path.display()),
            }
            None
        }
    }
}

/// Where a damaged file is kept: beside it, named for when it was found.
fn damaged_name(path: &Path) -> PathBuf {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{name}.damaged-{t}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ectomatter-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_file_is_written_whole_and_nothing_is_left_beside_it() {
        let d = dir("whole");
        let p = d.join("settings.json");
        write_json(&p, &serde_json::json!({ "a": 1 })).unwrap();
        write_json(&p, &serde_json::json!({ "a": 2 })).unwrap();
        assert_eq!(read_json(&p), Some(serde_json::json!({ "a": 2 })));
        let names: Vec<String> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["settings.json".to_string()], "no temporary file left");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_damaged_file_is_kept_aside_not_written_over() {
        let d = dir("damaged");
        let p = d.join("settings.json");
        std::fs::write(&p, b"{ \"scenes\": [ half a file").unwrap();
        assert_eq!(read_json(&p), None, "read as empty");
        let kept: Vec<PathBuf> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.path()).filter(|q| q.to_string_lossy().contains(".damaged-")).collect();
        assert_eq!(kept.len(), 1, "kept aside");
        assert_eq!(std::fs::read(&kept[0]).unwrap(), b"{ \"scenes\": [ half a file");
        write_json(&p, &serde_json::json!({})).unwrap();
        assert!(kept[0].exists(), "a write afterwards leaves it");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn no_file_is_none_and_nothing_is_made() {
        let d = dir("none");
        assert_eq!(read_json(&d.join("absent.json")), None);
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&d);
    }
}
