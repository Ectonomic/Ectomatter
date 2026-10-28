//! Whether Ectomatter starts when the person logs in, into the tray
//! (`--hide`), kept where each system keeps the programs that start at
//! login, as other programs keep theirs there:
//!
//! - Linux: an entry in the person's autostart folder (freedesktop's XDG
//!   Autostart), `$XDG_CONFIG_HOME/autostart/com.ectonomic.ectomatter.desktop`.
//!   systemd starts each such entry as a unit of its own
//!   (systemd-xdg-autostart-generator), after `graphical-session.target`; on
//!   Fedora KDE 44 that can be before KWin has told systemd where its display
//!   is, so a program started then has no WAYLAND_DISPLAY or DISPLAY (as on
//!   2026-10-28, when Ectomatter and KDE's own video bridge both failed). The
//!   entry's unit is ordered after KWin's (`plasma-kwin_wayland.service`), whose
//!   wrapper takes its bus name only once it has told systemd the display (KWin's
//!   kwin_wrapper.cpp): the fix that Fedora's forum found for the video bridge.
//!   Where KWin isn't, the order names a unit that isn't there, which systemd passes over.
//! - Windows: a value under the person's Run key,
//!   `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, and none of Task
//!   Manager's turning it off under `…\Explorer\StartupApproved\Run`.
//! - macOS: a launch agent in the person's `~/Library/LaunchAgents`, run at load.
//!
//! The copy of Ectomatter that starts is the program the entry names.

use std::path::{Path, PathBuf};

/// What starts at login.
#[derive(Clone, Debug, PartialEq)]
pub enum Starts {
    /// no Ectomatter
    No,
    /// this copy of Ectomatter
    This,
    /// another copy: the program the entry names
    Other(PathBuf),
}

/// The entry's name, the app's id, on every system.
const ID: &str = "com.ectonomic.ectomatter";

/// This copy of Ectomatter's program, as an entry names it.
fn me() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("Ectomatter can’t tell where its own program is ({e})"))
}

/// What an entry naming `program` (or none) starts.
fn starts(program: Option<PathBuf>) -> Starts {
    let Some(p) = program else { return Starts::No };
    let same = |a: &Path, b: &Path| a == b || std::fs::canonicalize(a).ok().is_some_and(|a| std::fs::canonicalize(b).is_ok_and(|b| a == b));
    match me() {
        Ok(m) if same(&p, &m) => Starts::This,
        _ => Starts::Other(p),
    }
}

/// What starts at login now.
pub fn now() -> Starts {
    starts(named())
}

/// Start this copy of Ectomatter at login (`on`), or no Ectomatter.
pub fn set(on: bool) -> Result<(), String> {
    if on { write(&me()?) } else { remove() }
}

/// At Ectomatter's start: an entry left naming a program that's gone (an
/// Ectomatter moved or installed anew) names this copy instead; and on Linux
/// the entry's order after KWin, which an entry made before Ectomatter wrote
/// it wants, is put beside it.
pub fn keep() {
    let Some(p) = named() else { return };
    let gone = !p.exists();
    if gone || cfg!(target_os = "linux") {
        let to = if gone { me() } else { Ok(p.clone()) };
        match to.and_then(|to| write(&to)) {
            Ok(()) if gone => eprintln!("ectomatter: starting at login named {}, which is gone; it starts this copy now", p.display()),
            Ok(()) => {}
            Err(e) => eprintln!("ectomatter: starting at login wasn't kept up: {e}"),
        }
    }
}

// ---- Linux: the autostart folder's entry, and its unit's order after KWin

/// The person's folder of settings.
#[cfg(target_os = "linux")]
fn config_home() -> PathBuf {
    #[cfg(test)]
    if let Some(b) = tests::BASE.with(|b| b.borrow().clone()) {
        return b;
    }
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"))
}

#[cfg(target_os = "linux")]
fn entry() -> PathBuf {
    config_home().join("autostart").join(format!("{ID}.desktop"))
}

/// The order of the entry's unit after KWin's: a drop-in for the unit systemd
/// makes of the entry (named for the entry's file).
#[cfg(target_os = "linux")]
fn order() -> PathBuf {
    config_home().join("systemd/user").join(format!("app-{ID}@autostart.service.d")).join("after-kwin.conf")
}

#[cfg(target_os = "linux")]
const ORDER: &str = "# Written by Ectomatter with its entry in the autostart folder: started at login\n\
# only once KWin has told systemd where its display is (see src/autostart.rs).\n\
[Unit]\n\
After=plasma-kwin_wayland.service\n";

#[cfg(target_os = "linux")]
fn named() -> Option<PathBuf> {
    let text = std::fs::read_to_string(entry()).ok()?;
    desktop_program(&text)
}

#[cfg(target_os = "linux")]
fn write(program: &Path) -> Result<(), String> {
    put(&entry(), &desktop_entry(program))?;
    put(&order(), ORDER)
}

#[cfg(target_os = "linux")]
fn remove() -> Result<(), String> {
    for f in [entry(), order()] {
        match std::fs::remove_file(&f) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{} can’t be taken out ({e})", f.display())),
        }
    }
    let _ = order().parent().map(std::fs::remove_dir);
    Ok(())
}

// ---- Windows: the Run key's value

#[cfg(windows)]
const RUN: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const APPROVED: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
#[cfg(windows)]
const NAME: &str = "Ectomatter";

/// `reg` run with these arguments, with no console window: what it printed, or why it didn't.
#[cfg(windows)]
fn reg(args: &[&str]) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("reg").args(args).creation_flags(0x0800_0000).output().map_err(|e| format!("Windows’ reg didn’t start ({e})"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(windows)]
fn named() -> Option<PathBuf> {
    // (turned off in Task Manager: kept, but nothing starts)
    if reg(&["query", APPROVED, "/v", NAME]).ok().is_some_and(|o| run_approved_off(&o)) {
        return None;
    }
    let out = reg(&["query", RUN, "/v", NAME]).ok()?;
    run_program(&out)
}

#[cfg(windows)]
fn write(program: &Path) -> Result<(), String> {
    reg(&["add", RUN, "/v", NAME, "/t", "REG_SZ", "/d", &run_value(program), "/f"])?;
    let _ = reg(&["delete", APPROVED, "/v", NAME, "/f"]);
    Ok(())
}

#[cfg(windows)]
fn remove() -> Result<(), String> {
    if reg(&["query", RUN, "/v", NAME]).is_ok() {
        reg(&["delete", RUN, "/v", NAME, "/f"])?;
    }
    Ok(())
}

// ---- macOS: the launch agent

#[cfg(target_os = "macos")]
fn agent() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Library/LaunchAgents").join(format!("{ID}.plist"))
}

#[cfg(target_os = "macos")]
fn named() -> Option<PathBuf> {
    plist_program(&std::fs::read_to_string(agent()).ok()?)
}

#[cfg(target_os = "macos")]
fn write(program: &Path) -> Result<(), String> {
    put(&agent(), &plist(program))
}

#[cfg(target_os = "macos")]
fn remove() -> Result<(), String> {
    match std::fs::remove_file(agent()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{} can’t be taken out ({e})", agent().display())),
    }
}

// ---- elsewhere: nothing to keep it in

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
fn named() -> Option<PathBuf> {
    None
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
fn write(_: &Path) -> Result<(), String> {
    Err("this system keeps no programs to start at login that Ectomatter knows of".into())
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
fn remove() -> Result<(), String> {
    Ok(())
}

/// A file written whole, its folder made first.
#[allow(dead_code)]
fn put(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{} can’t be made ({e})", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{} can’t be written ({e})", path.display()))
}

// ---- each system's entry, as text (on every system, so each is tested here)

/// The autostart entry that starts `program` into the tray.
#[allow(dead_code)]
fn desktop_entry(program: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Ectomatter\nComment=Starts in the tray when you log in\nExec={} --hide\nIcon={ID}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exec_arg(&program.to_string_lossy())
    )
}

/// A program's path as an entry's Exec takes it (the Desktop Entry
/// Specification's quoting, then its escapes for a string): in double quotes,
/// with `"`, `` ` ``, `$` and `\` escaped, where it holds anything but plain signs;
/// and `%` doubled.
fn exec_arg(path: &str) -> String {
    let plain = path.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+@,:".contains(c));
    let quoted = if plain {
        path.to_string()
    } else {
        let mut q = String::from("\"");
        for c in path.chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                q.push('\\');
            }
            q.push(c);
        }
        q.push('"');
        q
    };
    quoted.replace('\\', "\\\\").replace('%', "%%")
}

/// The program an autostart entry starts: its Exec's first argument (none
/// where the entry is turned off, `Hidden` or GNOME's `X-GNOME-Autostart-enabled`).
#[allow(dead_code)]
fn desktop_program(text: &str) -> Option<PathBuf> {
    let mut exec = None;
    let mut in_entry = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        let Some((k, v)) = line.split_once('=').filter(|_| in_entry) else { continue };
        match (k.trim(), v.trim()) {
            ("Hidden", "true") | ("X-GNOME-Autostart-enabled", "false") => return None,
            ("Exec", v) => exec = Some(v.to_string()),
            _ => {}
        }
    }
    first_arg(&exec?)
}

/// An Exec's first argument, its string escapes and its quoting undone.
fn first_arg(exec: &str) -> Option<PathBuf> {
    // the string's escapes
    let mut s = String::new();
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => s.push(' '),
                Some('n') => s.push('\n'),
                Some('t') => s.push('\t'),
                Some('r') => s.push('\r'),
                Some(o) => s.push(o),
                None => {}
            }
        } else {
            s.push(c);
        }
    }
    // the first argument: quoted, with its escapes, or up to a space
    let mut arg = String::new();
    let mut chars = s.trim_start().chars().peekable();
    if chars.peek() == Some(&'"') {
        chars.next();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => arg.extend(chars.next()),
                c => arg.push(c),
            }
        }
    } else {
        arg = chars.take_while(|c| !c.is_whitespace()).collect();
    }
    let arg = arg.replace("%%", "%");
    (!arg.is_empty()).then(|| PathBuf::from(arg))
}

/// The Run key's value that starts `program` into the tray.
#[allow(dead_code)]
fn run_value(program: &Path) -> String {
    format!("\"{}\" --hide", program.display())
}

/// The program in what `reg query` printed of the Run key's value.
#[allow(dead_code)]
fn run_program(out: &str) -> Option<PathBuf> {
    let line = out.lines().find(|l| l.contains("REG_SZ") || l.contains("REG_EXPAND_SZ"))?;
    let value = line.split_once("_SZ")?.1.trim();
    let program = match value.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => value.split_whitespace().next()?,
    };
    (!program.is_empty()).then(|| PathBuf::from(program))
}

/// Whether what `reg query` printed of Task Manager's value has it turned off
/// (its first byte odd: 03, where 02 is on).
#[allow(dead_code)]
fn run_approved_off(out: &str) -> bool {
    out.lines().find(|l| l.contains("REG_BINARY")).and_then(|l| l.split_once("REG_BINARY")).and_then(|(_, v)| u8::from_str_radix(v.trim().get(..2)?, 16).ok()).is_some_and(|b| b & 1 == 1)
}

/// The launch agent that starts `program` into the tray at login.
#[allow(dead_code)]
fn plist(program: &Path) -> String {
    let p = xml(&program.to_string_lossy());
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n\t<key>Label</key>\n\t<string>{ID}</string>\n\t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>{p}</string>\n\t\t<string>--hide</string>\n\t</array>\n\t<key>RunAtLoad</key>\n\t<true/>\n\t<key>ProcessType</key>\n\t<string>Interactive</string>\n</dict>\n</plist>\n"
    )
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The program a launch agent runs: its ProgramArguments' first.
#[allow(dead_code)]
fn plist_program(text: &str) -> Option<PathBuf> {
    let args = &text[text.find("<key>ProgramArguments</key>")?..];
    let start = args.find("<string>")? + "<string>".len();
    let end = start + args[start..].find("</string>")?;
    let p = args[start..end].replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&");
    (!p.is_empty()).then(|| PathBuf::from(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each system's entry names the program it was written for, a path with
    /// spaces, quotes, dollars, percents and backslashes among them, and is
    /// read back as that program; an entry turned off starts nothing.
    #[test]
    fn each_systems_entry_starts_the_program_it_names() {
        for p in ["/usr/bin/ectomatter", "/home/someone/My Apps/ecto \"matter\" $5 100% \\ `x`/ectomatter"] {
            let p = Path::new(p);
            assert_eq!(desktop_program(&desktop_entry(p)).as_deref(), Some(p), "{}", desktop_entry(p));
            assert!(desktop_entry(p).contains(" --hide\n"));
            assert_eq!(plist_program(&plist(p)).as_deref(), Some(p), "{}", plist(p));
        }
        assert_eq!(desktop_program(&desktop_entry(Path::new("/usr/bin/ectomatter"))).unwrap(), Path::new("/usr/bin/ectomatter"));
        assert!(desktop_entry(Path::new("/usr/bin/ectomatter")).contains("Exec=/usr/bin/ectomatter --hide"), "a plain path isn't quoted");
        // (the Desktop Entry Specification's quoting: in double quotes, `"`, `` ` ``, `$` and
        // `\` escaped with a backslash, then each backslash doubled as a string's, and `%` as `%%`)
        assert!(desktop_entry(Path::new("/a b/$x\"y`z%\\")).contains("Exec=\"/a b/\\\\$x\\\\\"y\\\\`z%%\\\\\\\\\" --hide\n"), "{}", desktop_entry(Path::new("/a b/$x\"y`z%\\")));
        let off = desktop_entry(Path::new("/usr/bin/ectomatter")) + "Hidden=true\n";
        assert_eq!(desktop_program(&off), None, "turned off, as KDE's Autostart settings do");
        assert_eq!(desktop_program("[Desktop Entry]\nExec=/a/b --x\nX-GNOME-Autostart-enabled=false\n"), None);
        assert_eq!(desktop_program("[Other]\nExec=/a/b\n"), None, "only the entry's own group counts");
        // (as `reg query` prints them)
        let run = format!("\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    Ectomatter    REG_SZ    {}\r\n\r\n", run_value(Path::new(r"C:\Program Files\Ectomatter\ectomatter.exe")));
        assert_eq!(run_program(&run).unwrap(), Path::new(r"C:\Program Files\Ectomatter\ectomatter.exe"));
        assert!(run_approved_off("    Ectomatter    REG_BINARY    030000000000000000000000\r\n"));
        assert!(!run_approved_off("    Ectomatter    REG_BINARY    020000000000000000000000\r\n"));
    }

    /// The entry that starts at login is told apart: this copy's, another
    /// copy's, or none.
    #[test]
    fn this_copy_is_told_from_another() {
        assert_eq!(starts(None), Starts::No);
        assert_eq!(starts(Some(std::env::current_exe().unwrap())), Starts::This);
        assert_eq!(starts(Some("/nowhere/ectomatter".into())), Starts::Other("/nowhere/ectomatter".into()));
    }

    /// Turned on, the entry and its order after KWin are written into the
    /// person's own folders and name this copy; turned off, both are gone; an
    /// entry naming a program that's gone names this copy after `keep`.
    #[cfg(target_os = "linux")]
    #[test]
    fn starting_at_login_is_written_where_the_desktop_reads_it() {
        let home = std::env::temp_dir().join(format!("ectomatter-autostart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        // (this test's folders alone: the person's own are never touched)
        let in_home = |f: &Path| f.starts_with(&home);
        temp_env(&home, || {
            assert!(in_home(&entry()) && in_home(&order()));
            assert_eq!(now(), Starts::No);
            set(true).unwrap();
            assert_eq!(now(), Starts::This);
            assert!(std::fs::read_to_string(order()).unwrap().contains("After=plasma-kwin_wayland.service"));
            set(false).unwrap();
            assert_eq!(now(), Starts::No);
            assert!(!entry().exists() && !order().exists());
            put(&entry(), &desktop_entry(Path::new("/gone/ectomatter"))).unwrap();
            assert_eq!(now(), Starts::Other("/gone/ectomatter".into()));
            keep();
            assert_eq!(now(), Starts::This, "the gone program's entry names this copy");
            assert!(order().exists(), "and its order after KWin is beside it");
        });
        let _ = std::fs::remove_dir_all(&home);
    }

    thread_local! {
        /// the person's folder of settings, as this thread's test has it
        pub(super) static BASE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    }

    /// The person's folder of settings taken to be `home` for `f`, on this test's thread alone.
    #[cfg(target_os = "linux")]
    fn temp_env(home: &Path, f: impl FnOnce()) {
        BASE.with(|b| *b.borrow_mut() = Some(home.to_path_buf()));
        f();
        BASE.with(|b| *b.borrow_mut() = None);
    }
}
