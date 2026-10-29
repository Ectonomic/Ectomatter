//! Ectomatter's About, Settings' last tab: its version, who made it and whose it is, its
//! license, and the notices of what it's built from and carries, each to read; its
//! credits, from the project's history (tools/credits.py); and the lead inviting a
//! person to build it with us, with the guide to making a package.
//! These are the GPLv3's Appropriate Legal Notices (its section 0): a copyright
//! notice, that it comes with no warranty, that it may be shared under the
//! license, and how to read the license.

use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};

/// Its version, as its manifest has it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// About's lead, over Build with us, a line each as it's set (broken by hand, so no line
/// is left alone and the last stands on its own).
pub const LEAD: [&str; 5] = [
    "Ectomatter is free and open source, and we\u{2019}d love to build it",
    "with the people who use it. If there\u{2019}s a device you\u{2019}d like to add,",
    "something you\u{2019}d like to improve, or an idea you want to try,",
    "you\u{2019}re invited to be part of what comes next.",
    "We\u{2019}d be glad to have you.",
];

/// What the license lets a person do, and that it comes with no warranty: the GPLv3's
/// notice, under About's buttons.
pub fn legal() -> String {
    format!("You can share and change Ectomatter under the {}. It comes with no warranty.", license().0)
}

/// Ectomatter's credits, as tools/credits.py makes them from the project's history: each
/// person who made it, each AI model that wrote with them, by commits; and what it's
/// built on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Credits {
    /// (name, commits, lines added, lines taken out), most commits first
    pub authors: Vec<(String, u64, u64, u64)>,
    /// (name, commits), most first
    pub models: Vec<(String, u64)>,
    pub commits: u64,
    pub libraries: u64,
    pub typefaces: u64,
}

impl Credits {
    fn from_json(v: &serde_json::Value) -> Result<Credits, String> {
        let n = |v: &serde_json::Value, k: &str| v[k].as_u64().unwrap_or(0);
        let list = |k: &str| v[k].as_array().cloned().ok_or_else(|| format!("the credits have no {k}"));
        Ok(Credits {
            authors: list("authors")?.iter().map(|a| (a["name"].as_str().unwrap_or("").to_string(), n(a, "commits"), n(a, "added"), n(a, "removed"))).filter(|a| !a.0.is_empty()).collect(),
            models: list("models")?.iter().map(|m| (m["name"].as_str().unwrap_or("").to_string(), n(m, "commits"))).filter(|m| !m.0.is_empty()).collect(),
            commits: n(v, "commits"),
            libraries: n(v, "libraries"),
            typefaces: n(v, "typefaces"),
        })
    }
}

/// The credits, read away from the window's thread: the file ECTOMATTER_CREDITS names (a
/// snapshot's, or a test's); run from the source folder, tools/credits.py's from the
/// history; else the install's credits.json beside its license.
pub fn credits() -> mpsc::Receiver<Result<Credits, String>> {
    let (tx, rx) = mpsc::channel();
    let named = std::env::var_os("ECTOMATTER_CREDITS").map(PathBuf::from);
    // (a named file is read at once, so a snapshot draws About with its credits)
    if let Some(f) = named {
        let _ = tx.send(read_credits(&f));
        return rx;
    }
    std::thread::spawn(move || {
        let got = if crate::cores::from_source() { made(folder()) } else { read_credits(&folder().join("credits.json")) };
        if let Err(e) = &got {
            eprintln!("ectomatter: About's credits: {e}");
        }
        let _ = tx.send(got);
    });
    rx
}

fn read_credits(f: &Path) -> Result<Credits, String> {
    let text = std::fs::read_to_string(f).map_err(|e| format!("This copy of Ectomatter came without its credits ({}: {e}).", f.display()))?;
    Credits::from_json(&serde_json::from_str(&text).map_err(|e| format!("Its credits can\u{2019}t be read ({}: {e}).", f.display()))?)
}

/// The credits tools/credits.py makes from the source folder `src`'s history.
fn made(src: &Path) -> Result<Credits, String> {
    let run = std::process::Command::new("python3")
        .args(["-I", "tools/credits.py", "Cargo.toml"])
        .current_dir(src)
        .env("CARGO", cargo())
        .output()
        .map_err(|e| format!("The credits weren\u{2019}t made: python3 didn\u{2019}t start ({e})."))?;
    if !run.status.success() {
        let said = String::from_utf8_lossy(&run.stderr);
        return Err(format!("The credits weren\u{2019}t made: {}", said.trim().lines().last().unwrap_or("tools/credits.py failed")));
    }
    Credits::from_json(&serde_json::from_slice(&run.stdout).map_err(|e| format!("tools/credits.py said what isn\u{2019}t JSON: {e}"))?)
}

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

/// Read: its license (0), its notices (1) or the guide to making a package (2, Build with
/// us), opened as the desktop opens a file of its kind, away from the window's thread; the
/// channel gets what went wrong, if anything.
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
    if which == 2 {
        // (the source folder's docs/, or the install's share/doc/ectomatter beside its licenses)
        let f = if source { dir.join("docs/making-a-package.md") } else { dir.join("../../doc/ectomatter/making-a-package.md") };
        if !f.is_file() {
            return Err("This copy of Ectomatter came without its guide to making a package.".into());
        }
        return opener(&f.to_string_lossy());
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

    /// The credits come from the project's own history and files: its authors and the
    /// models its commits name, and the libraries counted as the notices count them; and a
    /// made-up credits file reads as it says.
    #[test]
    fn the_credits_come_from_the_history() {
        let source = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        let c = made(source).unwrap();
        assert!(!c.authors.is_empty() && c.commits > 0 && c.authors.iter().all(|a| a.1 <= c.commits), "{c:?}");
        assert!(c.libraries > 100 && c.typefaces > 0, "{c:?}");
        let f = read_credits(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/renders/desk/credits.json")).unwrap();
        assert_eq!((f.authors.len(), f.models.len(), f.commits, f.libraries), (5, 2, 470, 512));
        assert_eq!(f.authors[0], ("Robin Example".to_string(), 412, 98120, 30544));
        assert!(read_credits(Path::new("/nonexistent/credits.json")).is_err());
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
        // (Build with us: the guide, where the install puts it, or said missing)
        assert_eq!(open_in(2, &installed, false, &scratch, &opener), Err("This copy of Ectomatter came without its guide to making a package.".into()));
        let doc = scratch.join("share/doc/ectomatter");
        std::fs::create_dir_all(&doc).unwrap();
        std::fs::write(doc.join("making-a-package.md"), "the guide").unwrap();
        open_in(2, &installed, false, &scratch, &opener).unwrap();
        assert_eq!(std::fs::read_to_string(last()).ok().as_deref(), Some("the guide"));
        // (run from the source folder: its LICENSE, and the notices written into the cache)
        let source = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        open_in(0, source, true, &scratch, &opener).unwrap();
        assert_eq!(last(), source.join("LICENSE").to_string_lossy());
        open_in(2, source, true, &scratch, &opener).unwrap();
        assert_eq!(last(), source.join("docs/making-a-package.md").to_string_lossy());
        open_in(1, source, true, &scratch.join("cache"), &opener).unwrap();
        let notices = std::fs::read_to_string(last()).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&scratch);
        assert!(last() == scratch.join("cache/NOTICES").to_string_lossy() && notices.starts_with("The notices of what") && notices.contains("\nslint "), "not the notices: {}", last());
    }
}
