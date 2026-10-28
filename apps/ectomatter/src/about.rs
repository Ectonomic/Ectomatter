//! Ectomatter's About, Settings' last section: its version, who made it and whose it is, its
//! license, and the notices of what it's built from and carries, each to read.
//! These are the GPLv3's Appropriate Legal Notices (its section 0): a copyright
//! notice, that it comes with no warranty, that it may be shared under the
//! license, and how to read the license.

use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};

/// Its version, as its manifest has it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Who made Ectomatter, as its manifest names them, in its order.
pub fn authors() -> Vec<&'static str> {
    env!("CARGO_PKG_AUTHORS").split(':').filter(|n| !n.is_empty()).collect()
}

/// Whose Ectomatter is, as REUSE.toml's first table says (every file is
/// theirs unless a later table names it): its year and name.
pub fn copyright() -> &'static str {
    // (the key split, so this file doesn't show another's work: tools/notices.py)
    const KEY: &str = concat!("SPDX-File", "CopyrightText");
    static REUSE: &str = include_str!("../../../REUSE.toml");
    REUSE.lines().find_map(|l| l.strip_prefix(KEY)?.trim_start().strip_prefix('=')?.trim().strip_prefix('"')?.strip_suffix('"')).unwrap_or_default()
}

/// Its license's name, and where its text is read when this copy came
/// without it.
pub fn license() -> (&'static str, &'static str) {
    match env!("CARGO_PKG_LICENSE") {
        "GPL-3.0-only" => ("GNU General Public License, version 3", "https://www.gnu.org/licenses/gpl-3.0.html"),
        other => (other, "https://spdx.org/licenses/"),
    }
}

/// Where its license and notices are: the source folder, run from there;
/// else share/licenses/ectomatter beside its packages' share/ectomatter, as
/// tools/install.sh lays them out. Said in the log once.
pub fn folder() -> &'static Path {
    static AT: OnceLock<PathBuf> = OnceLock::new();
    AT.get_or_init(|| {
        let at = folder_of(crate::cores::home(), crate::cores::from_source());
        eprintln!("ectomatter: its license and notices are read from {}", at.display());
        at
    })
}

fn folder_of(home: &Path, source: bool) -> PathBuf {
    if source {
        home.to_path_buf()
    } else {
        home.parent().unwrap_or(home).join("licenses/ectomatter")
    }
}

/// Read: its license (0) or its notices (1), opened as the desktop opens a
/// file of its kind, away from the window's thread; the channel gets what
/// went wrong, if anything.
pub fn read(which: i32) -> mpsc::Receiver<Result<(), String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(open(which));
    });
    rx
}

fn open(which: i32) -> Result<(), String> {
    open_in(which, folder(), crate::cores::from_source(), &crate::cache_dir(), &|t| crate::games::open(t))
}

/// Read's file opened by `opener` from the folder `dir` (the source folder when
/// `source`): the license where this copy has it, else its text online. The
/// notices are the install's; run from the source folder, tools/notices.py
/// writes them into `cache` first.
fn open_in(which: i32, dir: &Path, source: bool, cache: &Path, opener: &dyn Fn(&str) -> Result<(), String>) -> Result<(), String> {
    if which == 0 {
        let f = dir.join("LICENSE");
        return opener(if f.is_file() { f.to_str().unwrap_or_default() } else { license().1 });
    }
    let f = if source { written(dir, cache)? } else { dir.join("NOTICES") };
    if !f.is_file() {
        return Err("This copy of Ectomatter came without its notices.".into());
    }
    opener(&f.to_string_lossy())
}

/// The notices tools/notices.py writes from the source folder `src`, kept
/// in `cache`: where they are.
fn written(src: &Path, cache: &Path) -> Result<PathBuf, String> {
    let out = cache.join("NOTICES");
    let run = std::process::Command::new("python3")
        .args(["-I", "tools/notices.py", "Cargo.toml"])
        .current_dir(src)
        .env("CARGO", cargo())
        .output()
        .map_err(|e| format!("The notices weren’t written: python3 didn’t start ({e})."))?;
    if !run.status.success() {
        // (what it said last: a failure's own words, under any trace before them)
        let said = String::from_utf8_lossy(&run.stderr);
        return Err(format!("The notices weren’t written: {}", said.trim().lines().last().unwrap_or("tools/notices.py failed")));
    }
    std::fs::create_dir_all(cache).and_then(|_| std::fs::write(&out, &run.stdout)).map_err(|e| format!("The notices weren’t written to {}: {e}", out.display()))?;
    Ok(out)
}

/// Cargo, which tools/notices.py asks what Ectomatter is built from: the one
/// on the PATH, else where rustup puts it ($CARGO_HOME, else ~/.cargo), for
/// a window started from the desktop, whose PATH may not have it.
fn cargo() -> PathBuf {
    let on_path = std::env::var_os("PATH").into_iter().flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).map(|d| d.join("cargo")).find(|c| c.is_file());
    let home = std::env::var_os("CARGO_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")));
    on_path.or_else(|| home.map(|h| h.join("bin/cargo")).filter(|c| c.is_file())).unwrap_or_else(|| "cargo".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_copyright_is_reuse_tomls_first_table() {
        let c = copyright();
        let year = c.split(' ').next().unwrap_or_default();
        assert!(year.len() == 4 && year.chars().all(|d| d.is_ascii_digit()) && c.len() > 5, "no copyright from REUSE.toml: {c:?}");
    }

    #[test]
    fn its_authors_are_its_manifests() {
        assert!(!authors().is_empty() && authors().iter().all(|n| !n.contains(':')), "{:?}", authors());
    }

    #[test]
    fn its_license_has_a_name_and_a_text() {
        // (the license the About names is the one every crate comes under; a new
        // one needs its name and its text's address here)
        assert_ne!(license().0, env!("CARGO_PKG_LICENSE"), "the About has no name for {}", env!("CARGO_PKG_LICENSE"));
    }

    /// The folder an installed Ectomatter reads its license and notices from
    /// is the one tools/install.sh puts them in, beside its packages.
    #[test]
    fn an_install_is_read_where_install_sh_puts_its_license() {
        let sh = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/install.sh")).unwrap();
        let at = |var: &str| sh.lines().find_map(|l| l.trim().strip_prefix(&format!("{var}=\"$root/"))?.strip_suffix('"').map(str::to_owned));
        let packages = sh.lines().find_map(|l| l.trim().strip_prefix("out=\"$root/")?.split("/$kind").next().map(str::to_owned)).expect("install.sh's packages folder");
        let licenses = at("licenses").expect("install.sh's licenses folder");
        let root = Path::new("/prefix");
        assert_eq!(folder_of(&root.join(&packages), false), root.join(&licenses));
        assert_eq!(folder_of(root, true), root);
    }

    /// Read opens the file it names, from an install's folder or the source
    /// folder, or says what's missing; nothing is opened, only told to the
    /// test's opener.
    #[test]
    fn read_opens_the_license_and_the_notices() {
        let scratch = std::env::temp_dir().join(format!("ectomatter-about-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let installed = scratch.join("share/licenses/ectomatter");
        std::fs::create_dir_all(&installed).unwrap();
        let opened = std::cell::RefCell::new(Vec::new());
        let opener = |t: &str| -> Result<(), String> {
            opened.borrow_mut().push(t.to_string());
            Ok(())
        };
        let last = || opened.borrow().last().cloned().unwrap_or_default();
        // (an install that came without them: the license read online, the notices said missing)
        open_in(0, &installed, false, &scratch, &opener).unwrap();
        assert_eq!(last(), license().1);
        assert_eq!(open_in(1, &installed, false, &scratch, &opener), Err("This copy of Ectomatter came without its notices.".into()));
        // (an install with them)
        std::fs::write(installed.join("LICENSE"), "the license").unwrap();
        std::fs::write(installed.join("NOTICES"), "the notices").unwrap();
        open_in(0, &installed, false, &scratch, &opener).unwrap();
        assert_eq!(last(), installed.join("LICENSE").to_string_lossy());
        open_in(1, &installed, false, &scratch, &opener).unwrap();
        assert_eq!(last(), installed.join("NOTICES").to_string_lossy());
        // (run from the source folder: its LICENSE, and the notices written into the cache)
        let source = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        open_in(0, source, true, &scratch, &opener).unwrap();
        assert_eq!(last(), source.join("LICENSE").to_string_lossy());
        open_in(1, source, true, &scratch.join("cache"), &opener).unwrap();
        let notices = std::fs::read_to_string(last()).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&scratch);
        assert!(last() == scratch.join("cache/NOTICES").to_string_lossy() && notices.starts_with("The notices of what") && notices.contains("\nslint "), "not the notices: {}", last());
    }
}
