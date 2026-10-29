//! One Ectomatter for each person, which comes to them when it's opened
//! again. On an off-screen KWin with two virtual desktops and a session bus
//! of the test's own: Ectomatter opened on the first desktop, the person gone
//! to the second, and Ectomatter opened again. The new one stops, and the one
//! running is now on the second desktop. Opened again where it can be seen,
//! it stays where it is; started as at login (`--hide`), the new one stops
//! without bringing it forward. Each program runs in a sandbox of its own
//! with no devices (as in compositor.rs), Ectomatter with no cores or plugins
//! and fresh folders. Needs `bwrap`, `kwin_wayland` and `dbus-daemon`.

use std::collections::HashMap;
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
        for c in self.running.iter_mut().rev() {
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

/// Wait until `ok` (or `secs` go by): whether it came.
fn until(secs: u64, mut ok: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        if ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/// KWin's own id for each of Ectomatter's windows it has said opened, in order.
fn windows(log: &str) -> Vec<String> {
    log.lines().filter(|l| l.contains("KWin: opened com.ectonomic.ectomatter ")).filter_map(|l| l.split_whitespace().last().map(str::to_string)).collect()
}

/// The virtual desktops a window is on, as KWin says.
fn desktops(bus: &zbus::blocking::Connection, window: &str) -> Vec<String> {
    let kwin = zbus::blocking::Proxy::new(bus, "org.kde.KWin", "/KWin", "org.kde.KWin").unwrap();
    let info: HashMap<String, zbus::zvariant::OwnedValue> = kwin.call("getWindowInfo", &(window,)).unwrap();
    info.get("desktops").and_then(|d| Vec::<String>::try_from(d.try_clone().unwrap()).ok()).unwrap_or_default()
}

/// The virtual desktop the person is on.
fn current(bus: &zbus::blocking::Connection) -> String {
    let vd = zbus::blocking::Proxy::new(bus, "org.kde.KWin", "/VirtualDesktopManager", "org.kde.KWin.VirtualDesktopManager").unwrap();
    vd.get_property("current").unwrap()
}

#[test]
fn ectomatter_runs_once_and_comes_to_the_desktop_its_opened_on() {
    for tool in ["bwrap", "kwin_wayland", "dbus-daemon"] {
        assert!(Command::new("sh").args(["-c", &format!("command -v {tool}")]).stdout(Stdio::null()).status().is_ok_and(|s| s.success()), "this test needs {tool}");
    }
    let dir = std::env::temp_dir().join(format!("ectomatter-once-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = Scratch { dir: dir.clone(), running: Vec::new() };
    let (run, home, share, kwin_home) = (dir.join("run"), dir.join("home"), dir.join("share"), dir.join("kwin-home"));
    for d in [&run, &home, &share.join("cores"), &share.join("plugins"), &kwin_home.join("config")] {
        std::fs::create_dir_all(d).unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    // (two virtual desktops)
    std::fs::write(kwin_home.join("config/kwinrc"), "[Desktops]\nNumber=2\nRows=1\n").unwrap();
    let socket = run.join("bus");
    let address = format!("unix:path={}", socket.display());

    // a session bus, and KWin on it, off-screen on the CPU
    let base = [("PATH", "/usr/bin:/bin".to_string()), ("XDG_RUNTIME_DIR", run.display().to_string())];
    let bus_program = ["dbus-daemon", "--session", "--nofork", "--nopidfile", &format!("--address={address}")];
    let daemon = sandboxed(&dir, &base, &bus_program).spawn().expect("bwrap starts the bus");
    s.running.push(daemon);
    assert!(until(10, || socket.exists()), "the session bus didn't start");
    let mut kwin_env = base.to_vec();
    kwin_env.extend([
        ("HOME", kwin_home.display().to_string()),
        ("XDG_CONFIG_HOME", kwin_home.join("config").display().to_string()),
        ("DBUS_SESSION_BUS_ADDRESS", address.clone()),
        ("KWIN_COMPOSE", "Q".to_string()),
        ("LIBGL_ALWAYS_SOFTWARE", "1".to_string()),
    ]);
    let kwin_program = ["kwin_wayland", "--virtual", "--no-lockscreen", "--no-global-shortcuts", "--no-kactivities", "--socket", "wayland-once", "--width", "1280", "--height", "800"];
    let kwin = sandboxed(&dir, &kwin_env, &kwin_program).spawn().expect("bwrap starts KWin");
    s.running.push(kwin);
    let bus = zbus::blocking::connection::Builder::address(address.as_str()).unwrap().build().expect("the test on the session bus");
    let names = zbus::blocking::fdo::DBusProxy::new(&bus).unwrap();
    assert!(until(30, || names.name_has_owner("org.kde.KWin".try_into().unwrap()).unwrap_or(false)), "KWin didn't come on the bus");

    // Ectomatter on the first desktop
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
        ("WAYLAND_DISPLAY", "wayland-once".to_string()),
        ("DBUS_SESSION_BUS_ADDRESS", address.clone()),
        ("ECTOMATTER_PACKAGES", share.display().to_string()),
        ("ECTOMATTER_FPS", "1".to_string()),
        // (KWin's id for each window, in the log)
        ("ECTOMATTER_WINDOWS", "1".to_string()),
    ];
    let app = sandboxed(&dir, &env, &[exe.to_str().unwrap()]).spawn().expect("bwrap starts Ectomatter");
    s.running.push(app);
    let said = || std::fs::read_to_string(&log).unwrap_or_default();
    assert!(until(60, || said().contains("frames a second") && !windows(&said()).is_empty()), "Ectomatter's window wasn't drawn and told:\n{}", said());
    let first = windows(&said())[0].clone();
    let here = current(&bus);
    assert_eq!(desktops(&bus, &first), vec![here.clone()], "Ectomatter didn't open on the desktop the person was on");

    // the person goes to the second desktop, and opens Ectomatter again
    let kwin = zbus::blocking::Proxy::new(&bus, "org.kde.KWin", "/KWin", "org.kde.KWin").unwrap();
    let went: bool = kwin.call("setCurrentDesktop", &(2i32,)).unwrap();
    let there = current(&bus);
    assert!(went && there != here, "KWin didn't go to the second desktop");
    std::thread::sleep(Duration::from_secs(1));
    let again = |args: &[&str]| {
        let mut program = vec![exe.to_str().unwrap()];
        program.extend(args);
        let mut c = sandboxed(&dir, &env, &program);
        let mut child = c.stderr(Stdio::piped()).spawn().expect("bwrap starts Ectomatter again");
        let start = Instant::now();
        while child.try_wait().unwrap().is_none() && start.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let ended = child.try_wait().unwrap();
        if ended.is_none() {
            let _ = child.kill();
        }
        let out = child.wait_with_output().unwrap();
        (ended.is_some_and(|e| e.success()), String::from_utf8_lossy(&out.stderr).to_string())
    };
    let (stopped, told) = again(&[]);
    assert!(stopped, "the Ectomatter opened again didn't stop:\n{told}");
    assert!(told.contains("it was asked to come forward"), "the Ectomatter opened again didn't ask the one running:\n{told}");
    assert!(until(20, || windows(&said()).len() >= 2), "the window wasn't opened again where the person is:\n{}", said());
    let moved = windows(&said())[1].clone();
    assert_eq!(desktops(&bus, &moved), vec![there.clone()], "Ectomatter's window isn't on the desktop the person is on:\n{}", said());
    // (it stopped before it began a log of its own, which would put the running one's aside)
    assert!(!log.with_extension("log.1").exists(), "the Ectomatter opened again began a log of its own");

    // opened again where it can be seen: it stays where it is
    let (stopped, told) = again(&[]);
    assert!(stopped && told.contains("it was asked to come forward"), "the Ectomatter opened again didn't stop and ask:\n{told}");
    assert!(until(10, || said().matches("opened again, so this one comes forward").count() >= 2), "the one running didn't hear it:\n{}", said());
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(said().matches("out of sight, so it's opened again").count(), 1, "a window the person can see was opened again:\n{}", said());
    assert_eq!(windows(&said()).len(), 2, "a window the person can see was opened again:\n{}", said());

    // started as at login while one runs: it stops, and asks nothing
    let (stopped, told) = again(&["--hide"]);
    assert!(stopped && told.contains("runs already, so this one stops"), "the Ectomatter started at login didn't stop:\n{told}");
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(said().matches("opened again, so this one comes forward").count(), 2, "starting at login brought the one running forward:\n{}", said());
    assert!(!said().contains("panicked"), "{}", said());
}
