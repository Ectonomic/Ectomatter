//! Custom Scripts, Ectodeck's own section of a game's page: the person's own
//! commands, each a command to run as a game starts and another to undo it
//! as the game quits, the way Sunshine runs an app's "do" and "undo". The
//! Library keeps them (games.rs); Ectodeck runs them itself, here, and never
//! as an administrator: a command that would ask to be one is refused.
//!
//! A command runs in the system's own shell (sh, or cmd on Windows) in a
//! process group of its own, so stopping it stops what it started too. It may
//! be waited for, up to its time limit (stopped past it), or kept running
//! until the game quits.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// One of the person's commands, as the Library keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    pub id: u64,
    pub name: String,
    /// what runs as the game starts, and its undo as the game quits (may be empty)
    pub start: String,
    pub quit: String,
    /// wait for it to finish before going on; keep it running until the game quits
    pub wait: bool,
    pub keep: bool,
    /// seconds it may run before it's stopped (and the game goes on anyway)
    pub limit: u64,
}

impl Script {
    pub fn of(v: &serde_json::Value) -> Option<Script> {
        Some(Script {
            id: v["id"].as_u64()?,
            name: v["name"].as_str().unwrap_or("").to_string(),
            start: v["start"].as_str().unwrap_or("").to_string(),
            quit: v["quit"].as_str().unwrap_or("").to_string(),
            wait: v["wait"].as_bool().unwrap_or(true),
            keep: v["keep"].as_bool().unwrap_or(false),
            limit: v["limit"].as_u64().unwrap_or(10).clamp(1, 3600),
        })
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({ "id": self.id, "name": self.name, "start": self.start, "quit": self.quit, "wait": self.wait, "keep": self.keep, "limit": self.limit })
    }
}

/// How a command's run went.
#[derive(Clone, Debug, PartialEq)]
pub enum Ran {
    /// it finished well, in so many seconds
    Done(f64),
    /// started, and left running (kept until the game quits, or not waited for)
    Going,
    /// its program isn't on this computer (the program as the person typed it)
    Missing(String),
    /// it stopped with an error: its exit code and its last words
    Failed(i32, String),
    /// still running at its time limit, so stopped
    Stopped(u64),
    /// not run: why
    Refused(String),
}

impl Ran {
    pub fn ok(&self) -> bool {
        matches!(self, Ran::Done(_) | Ran::Going)
    }

    /// In words, for under the command: "finished in 0.4 seconds".
    pub fn words(&self) -> String {
        match self {
            Ran::Done(s) if *s < 0.95 => format!("finished in {:.1} seconds", s.max(0.1)),
            Ran::Done(s) if (s.round() - 1.0).abs() < f64::EPSILON => "finished in 1 second".into(),
            Ran::Done(s) => format!("finished in {} seconds", s.round()),
            Ran::Going => "started, and kept running".into(),
            Ran::Missing(p) => format!("couldn’t run: {p} isn’t on this computer"),
            Ran::Failed(code, why) if why.is_empty() => format!("stopped with an error (exit code {code})"),
            Ran::Failed(code, why) => format!("stopped with an error (exit code {code}): {why}"),
            Ran::Stopped(n) => format!("stopped after {n} seconds, still running"),
            Ran::Refused(why) => why.clone(),
        }
    }

    pub fn to_value(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Ran::Done(s) => json!({ "done": s }),
            Ran::Going => json!({ "going": true }),
            Ran::Missing(p) => json!({ "missing": p }),
            Ran::Failed(c, w) => json!({ "failed": c, "why": w }),
            Ran::Stopped(n) => json!({ "stopped": n }),
            Ran::Refused(w) => json!({ "refused": w }),
        }
    }

    pub fn of(v: &serde_json::Value) -> Option<Ran> {
        if let Some(s) = v["done"].as_f64() {
            return Some(Ran::Done(s));
        }
        if v["going"] == true {
            return Some(Ran::Going);
        }
        if let Some(p) = v["missing"].as_str() {
            return Some(Ran::Missing(p.into()));
        }
        if let Some(c) = v["failed"].as_i64() {
            return Some(Ran::Failed(c as i32, v["why"].as_str().unwrap_or("").into()));
        }
        if let Some(n) = v["stopped"].as_u64() {
            return Some(Ran::Stopped(n));
        }
        v["refused"].as_str().map(|w| Ran::Refused(w.into()))
    }
}

/// The words that would make a command run as an administrator.
const ELEVATE: [&str; 9] = ["sudo", "pkexec", "doas", "su", "run0", "gksudo", "kdesu", "kdesudo", "runas"];

/// Why a command can't run, if it can't: it asks to be an administrator.
pub fn refused(cmd: &str) -> Option<String> {
    let words = cmd.split(|c: char| c.is_whitespace() || ";|&()`$<>".contains(c)).filter(|w| !w.is_empty());
    for w in words {
        let base = w.rsplit(['/', '\\']).next().unwrap_or(w).trim_end_matches(".exe");
        if ELEVATE.contains(&base) {
            return Some(format!("Ectodeck doesn’t run commands as an administrator, so it won’t run {base}."));
        }
    }
    None
}

/// The program a command starts, as the person typed it.
fn program_of(cmd: &str) -> String {
    let mut words = cmd.split_whitespace().skip_while(|w| w.contains('=') && !w.starts_with('=') && !w.contains('/'));
    words.next().unwrap_or("").to_string()
}

/// A command started in the system's shell, in a process group of its own,
/// its output kept for its last words; `env` added to Ectodeck's own.
pub fn spawn(cmd: &str, env: &[(&str, String)]) -> std::io::Result<Child> {
    #[cfg(windows)]
    let mut c = {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("cmd");
        c.arg("/C").arg(cmd);
        // CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW
        c.creation_flags(0x0000_0200 | 0x0800_0000);
        c
    };
    #[cfg(not(windows))]
    let mut c = {
        use std::os::unix::process::CommandExt;
        let mut c = Command::new("sh");
        c.arg("-c").arg(cmd);
        c.process_group(0);
        c
    };
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        c.current_dir(home);
    }
    for (k, v) in env {
        c.env(k, v);
    }
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    c.spawn()
}

/// Stop a command and all it started: asked first, then made to.
pub fn stop(child: &mut Child) {
    #[cfg(not(windows))]
    {
        let group = child.id() as i32;
        // SAFETY: signalling the process group the command was started as the leader of
        unsafe {
            libc::kill(-group, libc::SIGTERM);
        }
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The last line a command wrote to its error output, kept short.
fn last_words(child: &mut Child) -> String {
    let mut s = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = std::io::Read::read_to_string(&mut e, &mut s);
    }
    let line = s.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let mut out: String = line.chars().take(160).collect();
    if line.chars().count() > 160 {
        out.push('…');
    }
    out
}

/// Whether a failure is the shell not finding the program.
fn missing(code: i32, words: &str) -> bool {
    code == 127 || (cfg!(windows) && code == 9009) || words.contains("command not found") || words.ends_with("not found") || (code == 126 && words.contains("No such file"))
}

/// A command run and, if it's to be, waited for up to `limit` seconds. A
/// command not waited for, or kept, comes back with its process.
pub fn run(cmd: &str, limit: u64, wait: bool, keep: bool, env: &[(&str, String)]) -> (Ran, Option<Child>) {
    if let Some(why) = refused(cmd) {
        return (Ran::Refused(why), None);
    }
    if cmd.trim().is_empty() {
        return (Ran::Done(0.0), None);
    }
    let t = Instant::now();
    let mut child = match spawn(cmd, env) {
        Ok(c) => c,
        Err(e) => return (Ran::Failed(-1, e.to_string()), None),
    };
    if keep || !wait {
        // (a failure in its first moment is still told: a program that isn't there)
        std::thread::sleep(Duration::from_millis(150));
        if let Ok(Some(status)) = child.try_wait() {
            let code = status.code().unwrap_or(-1);
            let words = last_words(&mut child);
            return (finished(cmd, code, words, t), None);
        }
        return (Ran::Going, Some(child));
    }
    let until = t + Duration::from_secs(limit.max(1));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code().unwrap_or(-1);
                let words = last_words(&mut child);
                return (finished(cmd, code, words, t), None);
            }
            Ok(None) if Instant::now() >= until => {
                stop(&mut child);
                return (Ran::Stopped(limit), None);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return (Ran::Failed(-1, e.to_string()), None),
        }
    }
}

fn finished(cmd: &str, code: i32, words: String, since: Instant) -> Ran {
    match code {
        0 => Ran::Done(since.elapsed().as_secs_f64()),
        c if missing(c, &words) => Ran::Missing(program_of(cmd)),
        c => Ran::Failed(c, words),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_administrators_command_is_refused() {
        assert!(refused("sudo systemctl stop ollama").is_some());
        assert!(refused("echo hi && /usr/bin/pkexec thing").is_some());
        assert!(refused("~/.lmstudio/bin/lms server stop").is_none());
        // (a word that only contains one isn't one)
        assert!(refused("pseudo-thing --sudoku").is_none());
        let (r, _) = run("sudo true", 5, true, false, &[]);
        assert!(matches!(r, Ran::Refused(_)));
    }

    #[cfg(unix)]
    #[test]
    fn a_run_says_how_it_went() {
        let (r, _) = run("true", 5, true, false, &[]);
        assert!(matches!(r, Ran::Done(_)), "{r:?}");
        let (r, _) = run("~/.no-such-place/bin/lms server stop", 5, true, false, &[]);
        assert_eq!(r, Ran::Missing("~/.no-such-place/bin/lms".into()));
        assert_eq!(r.words(), "couldn’t run: ~/.no-such-place/bin/lms isn’t on this computer");
        let (r, _) = run("echo nope >&2; exit 3", 5, true, false, &[]);
        assert_eq!(r, Ran::Failed(3, "nope".into()));
        let t = Instant::now();
        let (r, _) = run("sleep 30", 1, true, false, &[]);
        assert_eq!(r, Ran::Stopped(1));
        assert!(t.elapsed() < Duration::from_secs(4), "stopped at its limit");
        // the game's name reaches the command
        let f = std::env::temp_dir().join(format!("ectodeck-cmd-{}", std::process::id()));
        let (r, _) = run(&format!("printf '%s' \"$ECTODECK_GAME\" > '{}'", f.display()), 5, true, false, &[("ECTODECK_GAME", "ARC Raiders".into())]);
        assert!(r.ok());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "ARC Raiders");
        let _ = std::fs::remove_file(&f);
    }

    /// A command kept running is stopped with all it started.
    #[cfg(unix)]
    #[test]
    fn a_kept_command_is_stopped_whole() {
        let (r, child) = run("sleep 30 & sleep 30; wait", 5, true, true, &[]);
        assert_eq!(r, Ran::Going);
        let mut child = child.unwrap();
        let group = child.id() as i32;
        stop(&mut child);
        std::thread::sleep(Duration::from_millis(100));
        // nothing of its group is left
        let left = unsafe { libc::kill(-group, 0) };
        assert_eq!(left, -1);
    }
}
