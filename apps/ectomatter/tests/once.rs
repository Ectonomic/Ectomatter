//! One Ectomatter for each person, which comes to them when it's opened
//! again. On an off-screen KWin with two virtual desktops and a session bus
//! of the test's own: Ectomatter opened on the first desktop, the person gone
//! to the second, and Ectomatter opened again. The new one stops, and the one
//! running is now on the second desktop. Opened again where it can be seen,
//! it stays where it is; started as at login (`--hide`), the new one stops
//! without bringing it forward. The desktop's tray comes after it, as at
//! login: its icon goes in, its menu has Open and Quit, a press on it with
//! the window on the other desktop brings the window, and Quit quits.
//! Each program runs in a sandbox of its own
//! with no devices (as in compositor.rs), Ectomatter with no cores or plugins
//! and fresh folders. Needs `bwrap`, `kwin_wayland` and `dbus-daemon`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
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

/// A method of KWin's called on the test's bus, and its answer.
fn kwin<R: serde::de::DeserializeOwned + zbus::wire::Type, B: serde::Serialize + zbus::wire::DynamicType>(bus: &zbus::Connection, path: &str, iface: &str, method: &str, body: &B) -> R {
    zbus::block_on(async { bus.call_method(Some("org.kde.KWin"), path, Some(iface), method, body).await.unwrap().body().deserialize::<R>().unwrap() })
}

/// The virtual desktops a window is on, as KWin says.
fn desktops(bus: &zbus::Connection, window: &str) -> Vec<String> {
    let info: HashMap<String, zbus::wire::OwnedVariant> = kwin(bus, "/KWin", "org.kde.KWin", "getWindowInfo", &(window,));
    info.get("desktops").and_then(|d| Vec::<String>::try_from(d.try_clone().unwrap()).ok()).unwrap_or_default()
}

/// The virtual desktop the person is on.
fn current(bus: &zbus::Connection) -> String {
    let v: zbus::wire::OwnedVariant = kwin(bus, "/VirtualDesktopManager", "org.freedesktop.DBus.Properties", "Get", &("org.kde.KWin.VirtualDesktopManager", "current"));
    String::try_from(v).unwrap()
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
    let compositor = sandboxed(&dir, &kwin_env, &kwin_program).spawn().expect("bwrap starts KWin");
    s.running.push(compositor);
    let bus = zbus::block_on(zbus::connection::Builder::address(address.as_str()).build()).expect("the test on the session bus");
    let owned = |name: &str| zbus::block_on(async { bus.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "NameHasOwner", &(name,)).await.ok()?.body().deserialize::<bool>().ok() }).unwrap_or(false);
    assert!(until(30, || owned("org.kde.KWin")), "KWin didn't come on the bus");

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
    let went: bool = kwin(&bus, "/KWin", "org.kde.KWin", "setCurrentDesktop", &(2i32,));
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

    // the desktop's tray comes after Ectomatter, as at login: the icon goes in it
    let registered = Arc::new(Mutex::new(Vec::<String>::new()));
    let _tray = zbus::block_on(zbus::connection::Builder::address(address.as_str()).name("org.kde.StatusNotifierWatcher").serve_at("/StatusNotifierWatcher", Watcher(registered.clone())).build()).expect("a tray on the test's bus");
    assert!(until(10, || !registered.lock().unwrap().is_empty()), "Ectomatter's icon didn't go in the tray that came:\n{}", said());
    let item = registered.lock().unwrap()[0].clone();
    let title: String = property(&bus, &item, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "Title");
    assert_eq!(title, "Ectomatter");
    let icons: Vec<(i32, i32, Vec<u8>)> = property(&bus, &item, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "IconPixmap");
    assert!(!icons.is_empty() && icons.iter().all(|(w, h, d)| *w > 0 && d.len() == (w * h * 4) as usize), "the icon isn't one a tray can draw");
    let menu: zbus::wire::OwnedObjectPath = property(&bus, &item, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "Menu");
    let (_, layout): (u32, Entry) = call(&bus, &item, menu.as_str(), "com.canonical.dbusmenu", "GetLayout", &(0i32, -1i32, Vec::<String>::new()));
    let entries = layout.flat();
    let id_of = |label: &str| entries.iter().find(|(_, l)| l == label).map(|(id, _)| *id);
    assert!(id_of("Open Ectomatter").is_some() && id_of("Quit Ectomatter").is_some(), "the tray's menu: {entries:?}");

    // the person on the first desktop, the window on the second: a press on the icon brings it
    let went: bool = kwin(&bus, "/KWin", "org.kde.KWin", "setCurrentDesktop", &(1i32,));
    assert!(went && current(&bus) == here, "KWin didn't go back to the first desktop");
    std::thread::sleep(Duration::from_secs(1));
    let () = call(&bus, &item, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "ProvideXdgActivationToken", &("a-token-of-the-tests",));
    let () = call(&bus, &item, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "Activate", &(0i32, 0i32));
    assert!(until(20, || windows(&said()).len() >= 3), "a press on the tray's icon didn't bring the window:\n{}", said());
    let brought = windows(&said())[2].clone();
    assert_eq!(desktops(&bus, &brought), vec![here.clone()], "the tray's icon didn't bring the window to the desktop the person is on:\n{}", said());

    // Quit, from the tray's menu: Ectomatter quits whole
    let () = call(&bus, &item, menu.as_str(), "com.canonical.dbusmenu", "Event", &(id_of("Quit Ectomatter").unwrap(), "clicked", zbus::wire::Variant::from(""), 0u32));
    assert!(until(15, || s.running[2].try_wait().unwrap().is_some()), "Quit in the tray's menu didn't quit Ectomatter:\n{}", said());
    assert!(s.running[2].try_wait().unwrap().is_some_and(|e| e.success()), "Ectomatter didn't quit cleanly:\n{}", said());
    assert!(!said().contains("panicked"), "{}", said());
}

/// The desktop's tray, as far as an icon's registering with it: each item's name.
struct Watcher(Arc<Mutex<Vec<String>>>);

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(&self, service: String) {
        self.0.lock().unwrap().push(service);
    }
}

/// A method called on the test's bus, and its answer.
fn call<R: serde::de::DeserializeOwned + zbus::wire::Type, B: serde::Serialize + zbus::wire::DynamicType>(bus: &zbus::Connection, dest: &str, path: &str, iface: &str, method: &str, body: &B) -> R {
    zbus::block_on(async { bus.call_method(Some(dest), path, Some(iface), method, body).await.unwrap().body().deserialize::<R>().unwrap() })
}

/// A property, on the test's bus.
fn property<R: TryFrom<zbus::wire::OwnedVariant>>(bus: &zbus::Connection, dest: &str, path: &str, iface: &str, name: &str) -> R
where
    R::Error: std::fmt::Debug,
{
    let v: zbus::wire::OwnedVariant = call(bus, dest, path, "org.freedesktop.DBus.Properties", "Get", &(iface, name));
    R::try_from(v).unwrap()
}

/// A menu entry as dbusmenu's GetLayout gives it.
#[derive(serde::Deserialize, zbus::wire::Type, zbus::wire::OwnedVariant)]
struct Entry {
    id: i32,
    props: HashMap<String, zbus::wire::OwnedVariant>,
    children: Vec<zbus::wire::OwnedVariant>,
}

impl Entry {
    /// Every entry under this one, with its label.
    fn flat(self) -> Vec<(i32, String)> {
        let label = self.props.get("label").and_then(|l| String::try_from(l.try_clone().ok()?).ok()).unwrap_or_default();
        let mut out = vec![(self.id, label)];
        for c in self.children {
            if let Ok(e) = Entry::try_from(c) {
                out.extend(e.flat());
            }
        }
        out
    }
}
