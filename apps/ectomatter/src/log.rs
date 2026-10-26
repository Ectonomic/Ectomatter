//! Ectomatter's log: everything it and its libraries write to stderr as it
//! runs goes to the terminal as before and into a file in the person's state
//! folder (`$XDG_STATE_HOME/com.ectonomic.ectomatter/ectomatter.log`), the run
//! before's kept beside it as `ectomatter.log.1`. Each line is stamped with the
//! time since the start. A panic's message is written to the file at once, so
//! a crash leaves its reason.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

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

static FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// Lines as they go into the file: each stamped with the seconds since the
/// start, the stamp put where a line begins (a write can end mid-line).
struct Stamped {
    start: Instant,
    at_line_start: bool,
}

impl Stamped {
    fn stamp(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len() + 16);
        for &b in bytes {
            if self.at_line_start {
                out.extend_from_slice(format!("[{:>9.3}] ", self.start.elapsed().as_secs_f64()).as_bytes());
                self.at_line_start = false;
            }
            out.push(b);
            if b == b'\n' {
                self.at_line_start = true;
            }
        }
        out
    }
}

/// Starts the log: the last run's moved aside, and stderr from here on
/// written to the file as well as where it went. Says why when it can't.
pub fn start() {
    let path = file();
    if let Err(e) = std::fs::create_dir_all(folder()) {
        eprintln!("ectomatter: no log, its folder {} can't be made: {e}", folder().display());
        return;
    }
    let _ = std::fs::rename(&path, path.with_extension("log.1"));
    let file = match std::fs::File::create(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("ectomatter: no log, {} can't be written: {e}", path.display());
            return;
        }
    };
    *FILE.lock().unwrap() = Some(file);
    let start = Instant::now();
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(f) = FILE.lock().ok().as_mut().and_then(|f| f.as_mut()) {
            let _ = writeln!(f, "[{:>9.3}] ectomatter stopped: {info}", start.elapsed().as_secs_f64());
            let _ = f.flush();
        }
        before(info);
    }));
    tee(start);
}

#[cfg(unix)]
fn tee(start: Instant) {
    use std::io::Read;
    use std::os::fd::FromRawFd;
    // stderr becomes a pipe; a thread copies what comes down it to where
    // stderr went before and to the file
    let mut ends = [0; 2];
    if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
        eprintln!("ectomatter: no log, stderr can't be copied: {}", std::io::Error::last_os_error());
        *FILE.lock().unwrap() = None;
        return;
    }
    let was = unsafe { libc::dup(2) };
    if was < 0 || unsafe { libc::dup2(ends[1], 2) } < 0 {
        eprintln!("ectomatter: no log, stderr can't be copied: {}", std::io::Error::last_os_error());
        *FILE.lock().unwrap() = None;
        return;
    }
    unsafe { libc::close(ends[1]) };
    let mut from = unsafe { std::fs::File::from_raw_fd(ends[0]) };
    let mut to = unsafe { std::fs::File::from_raw_fd(was) };
    let mut stamped = Stamped { start, at_line_start: true };
    std::thread::Builder::new()
        .name("log".into())
        .spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                let n = match from.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let _ = to.write_all(&buf[..n]);
                if let Some(f) = FILE.lock().unwrap().as_mut() {
                    let _ = f.write_all(&stamped.stamp(&buf[..n]));
                }
            }
        })
        .expect("the log's thread");
}

#[cfg(not(unix))]
fn tee(_start: Instant) {
    // (deferred, stage 5: stderr isn't copied to the file on other systems yet)
    *FILE.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_line_is_stamped_where_it_begins() {
        let mut s = Stamped { start: Instant::now(), at_line_start: true };
        let a = String::from_utf8(s.stamp(b"one\ntw")).unwrap();
        let b = String::from_utf8(s.stamp(b"o\nthree\n")).unwrap();
        let all = format!("{a}{b}");
        let lines: Vec<&str> = all.lines().collect();
        assert_eq!(lines.len(), 3, "{all}");
        for (line, word) in lines.iter().zip(["one", "two", "three"]) {
            assert!(line.starts_with('[') && line.ends_with(&format!("] {word}")), "{line}");
        }
    }

    #[test]
    fn the_log_is_kept_in_the_state_folder() {
        assert!(file().ends_with("com.ectonomic.ectomatter/ectomatter.log"));
    }
}
