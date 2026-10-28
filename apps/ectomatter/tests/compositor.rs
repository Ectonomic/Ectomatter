//! Ectomatter with no compositor to start with, as at login when it starts
//! before KWin is ready: it works on without its window and shows the window
//! once a compositor is there; when that compositor goes away, it goes on
//! without, and shows the window on the next. Ectomatter and KWin each run in
//! a sandbox of their own with no devices (bwrap's own /dev: no GPU, no USB)
//! and their own processes (the sandbox gone, all of them are), KWin drawing
//! off-screen (`--virtual`) on the CPU, Ectomatter with no cores or plugins,
//! fresh folders and no session bus. Needs `bwrap`, `kwin_wayland` and
//! `dbus-run-session`.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A folder of the test's own, empty, gone after it, with what runs in it.
struct Scratch {
    dir: PathBuf,
    running: Vec<Child>,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for c in &mut self.running {
            let _ = c.kill();
            let _ = c.wait();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A program run in a sandbox of its own: this computer's files, the
/// scratch folder writable, no devices, its own processes, and only `env`.
fn sandboxed(dir: &Path, env: &[(&str, String)], program: &[&str]) -> Command {
    let mut c = Command::new("bwrap");
    c.args(["--bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--bind"]).arg(dir).arg(dir);
    c.args(["--unshare-pid", "--die-with-parent", "--", "env", "-i"]);
    for (k, v) in env {
        c.arg(format!("{k}={v}"));
    }
    c.args(program).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    c
}

/// KWin, off-screen on the CPU, on its own bus, at `socket` in `run`.
fn kwin(s: &Scratch, run: &Path, socket: &str) -> Child {
    let home = s.dir.join("kwin-home");
    let env = [
        ("PATH", "/usr/bin:/bin".to_string()),
        ("HOME", home.display().to_string()),
        ("XDG_CONFIG_HOME", home.join("config").display().to_string()),
        ("XDG_RUNTIME_DIR", run.display().to_string()),
        ("KWIN_COMPOSE", "Q".to_string()),
        ("LIBGL_ALWAYS_SOFTWARE", "1".to_string()),
    ];
    let program = ["dbus-run-session", "--", "kwin_wayland", "--virtual", "--no-lockscreen", "--no-global-shortcuts", "--no-kactivities", "--socket", socket, "--width", "1280", "--height", "800"];
    sandboxed(&s.dir, &env, &program).spawn().expect("bwrap starts KWin")
}

/// Wait until the log holds `words` a `times`th time (or `secs` go by): whether it did.
fn heard(log: &Path, words: &str, times: usize, secs: u64) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        if std::fs::read_to_string(log).unwrap_or_default().matches(words).count() >= times {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/// The process under `pid` running `exe`, looked for among its children's children.
fn under(pid: u32, exe: &Path) -> Option<u32> {
    let exe = std::fs::canonicalize(exe).ok()?;
    let mut look = vec![pid];
    while let Some(p) = look.pop() {
        if std::fs::read_link(format!("/proc/{p}/exe")).is_ok_and(|e| e == exe) {
            return Some(p);
        }
        for t in std::fs::read_dir(format!("/proc/{p}/task")).into_iter().flatten().flatten() {
            let kids = std::fs::read_to_string(t.path().join("children")).unwrap_or_default();
            look.extend(kids.split_whitespace().filter_map(|k| k.parse::<u32>().ok()));
        }
    }
    None
}

#[test]
fn ectomatter_starts_without_a_compositor_and_shows_its_window_when_one_comes() {
    for tool in ["bwrap", "kwin_wayland", "dbus-run-session"] {
        assert!(Command::new("sh").args(["-c", &format!("command -v {tool}")]).stdout(Stdio::null()).status().is_ok_and(|s| s.success()), "this test needs {tool}");
    }
    let dir = std::env::temp_dir().join(format!("ectomatter-compositor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = Scratch { dir: dir.clone(), running: Vec::new() };
    let (run, home, share) = (dir.join("run"), dir.join("home"), dir.join("share"));
    for d in [&run, &home, &share.join("cores"), &share.join("plugins")] {
        std::fs::create_dir_all(d).unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let exe = Path::new(env!("CARGO_BIN_EXE_ectomatter"));
    let log = home.join("state/com.ectonomic.ectomatter/ectomatter.log");
    let env = [
        ("PATH", "/usr/bin:/bin".to_string()),
        ("HOME", home.display().to_string()),
        ("XDG_CONFIG_HOME", home.join("config").display().to_string()),
        ("XDG_DATA_HOME", home.join("data").display().to_string()),
        ("XDG_STATE_HOME", home.join("state").display().to_string()),
        ("XDG_CACHE_HOME", home.join("cache").display().to_string()),
        ("XDG_RUNTIME_DIR", run.display().to_string()),
        ("WAYLAND_DISPLAY", "wayland-later".to_string()),
        // (no session bus: no tray, so the window shows; nothing of this computer's desktop told)
        ("DBUS_SESSION_BUS_ADDRESS", format!("unix:path={}", run.join("no-bus").display())),
        ("ECTOMATTER_PACKAGES", share.display().to_string()),
        ("ECTOMATTER_FPS", "1".to_string()),
    ];
    let app = sandboxed(&dir, &env, &[exe.to_str().unwrap()]).spawn().expect("bwrap starts Ectomatter");
    let app_pid = app.id();
    s.running.push(app);
    let said = || std::fs::read_to_string(&log).unwrap_or_default();

    // no compositor: Ectomatter works on
    assert!(heard(&log, "no compositor yet", 1, 20), "Ectomatter didn't say it waits:\n{}", said());
    std::thread::sleep(Duration::from_secs(3));
    assert!(s.running[0].try_wait().unwrap().is_none(), "Ectomatter stopped with no compositor:\n{}", said());
    assert!(!said().contains("frames a second"), "a window was drawn with no compositor:\n{}", said());

    // a compositor comes: the window is drawn on it
    let k = kwin(&s, &run, "wayland-later");
    s.running.push(k);
    assert!(heard(&log, "the compositor is there", 1, 30), "Ectomatter didn't find the compositor:\n{}", said());
    assert!(heard(&log, "frames a second", 1, 30), "the window wasn't drawn:\n{}", said());

    // the compositor goes: Ectomatter works on, and draws on the next one
    let mut gone = s.running.remove(1);
    gone.kill().unwrap();
    gone.wait().unwrap();
    assert!(heard(&log, "the compositor went away", 1, 20), "Ectomatter didn't see the compositor go:\n{}", said());
    std::thread::sleep(Duration::from_secs(2));
    assert!(s.running[0].try_wait().unwrap().is_none(), "Ectomatter stopped with the compositor:\n{}", said());
    let drawn = said().matches("frames a second").count();
    let k = kwin(&s, &run, "wayland-later");
    s.running.push(k);
    assert!(heard(&log, "the compositor is there", 2, 30), "Ectomatter didn't find the next compositor:\n{}", said());
    assert!(heard(&log, "frames a second", drawn + 1, 30), "the window wasn't drawn on the next compositor:\n{}", said());

    // asked to stop, as at log out: it quits whole
    let pid = under(app_pid, exe).expect("Ectomatter's process in its sandbox");
    assert!(Command::new("kill").args(["-TERM", &pid.to_string()]).status().unwrap().success());
    let start = Instant::now();
    while s.running[0].try_wait().unwrap().is_none() && start.elapsed() < Duration::from_secs(15) {
        std::thread::sleep(Duration::from_millis(200));
    }
    let ended = s.running[0].try_wait().unwrap();
    assert!(ended.is_some_and(|e| e.success()), "Ectomatter didn't quit when asked ({ended:?}):\n{}", said());
    assert!(!said().contains("panicked"), "{}", said());
}
