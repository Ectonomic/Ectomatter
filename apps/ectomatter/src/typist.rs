//! Ectomatter's own keyboard, for the macros it plays: a virtual keyboard
//! (Linux's uinput) that types a macro's steps into the computer as though
//! they were typed, for a device that can't play macros itself, as the
//! Sensei 310's buttons can't. A macro plays on a thread of its own, its
//! waits as its steps say, and whatever it holds down is let go when it stops.
//! A step that does one of Ectomatter's jobs (a profile switched, a screen job)
//! is handed to the window, which does it, as the macro reaches it.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use ectomatter_protocol::{keys, media, Plays};
use serde_json::Value;

// Linux's uinput and input interfaces (linux/uinput.h, linux/input.h)
const UI_DEV_CREATE: libc::c_ulong = 0x5501;
const UI_DEV_DESTROY: libc::c_ulong = 0x5502;
const UI_DEV_SETUP: libc::c_ulong = 0x405c_5503;
const UI_SET_EVBIT: libc::c_ulong = 0x4004_5564;
const UI_SET_KEYBIT: libc::c_ulong = 0x4004_5565;
#[cfg(test)]
const UI_GET_SYSNAME_64: libc::c_ulong = 0x8040_552c;
const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const SYN_REPORT: u16 = 0;
/// the highest key code the keyboard says it has (KEY_MAX's range for keys)
const KEYS: u16 = 0x2ff;

#[repr(C)]
struct Setup {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
    name: [u8; 80],
    ff_effects_max: u32,
}

#[repr(C)]
struct Event {
    sec: i64,
    usec: i64,
    kind: u16,
    code: u16,
    value: i32,
}

/// The virtual keyboard, while it exists.
pub struct Keyboard {
    fd: i32,
}

impl Keyboard {
    /// Ectomatter's keyboard, made: one that has every key a step can press.
    pub fn new() -> Result<Keyboard, String> {
        let path = c"/dev/uinput";
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(format!("Ectomatter can't make its keyboard: /dev/uinput won't open ({})", std::io::Error::last_os_error()));
        }
        let k = Keyboard { fd };
        let ok = |r: i32, what: &str| if r < 0 { Err(format!("Ectomatter can't make its keyboard: {what} ({})", std::io::Error::last_os_error())) } else { Ok(()) };
        unsafe {
            ok(libc::ioctl(fd, UI_SET_EVBIT, EV_KEY as libc::c_int), "its keys")?;
            for code in 1..KEYS {
                ok(libc::ioctl(fd, UI_SET_KEYBIT, code as libc::c_int), "a key")?;
            }
            let mut name = [0u8; 80];
            let words = b"Ectomatter macros";
            name[..words.len()].copy_from_slice(words);
            // (a virtual device's bus, Ectomatter's own numbers)
            let setup = Setup { bustype: 0x06, vendor: 0xEC70, product: 0x0001, version: 1, name, ff_effects_max: 0 };
            ok(libc::ioctl(fd, UI_DEV_SETUP, &setup), "its name")?;
            ok(libc::ioctl(fd, UI_DEV_CREATE), "the device")?;
        }
        Ok(k)
    }

    fn send(&self, kind: u16, code: u16, value: i32) -> Result<(), String> {
        let e = Event { sec: 0, usec: 0, kind, code, value };
        let n = unsafe { libc::write(self.fd, &e as *const Event as *const libc::c_void, std::mem::size_of::<Event>()) };
        if n as usize == std::mem::size_of::<Event>() {
            Ok(())
        } else {
            Err(format!("Ectomatter's keyboard didn't type ({})", std::io::Error::last_os_error()))
        }
    }

    /// A key going down or up, by its Linux key code.
    pub fn key(&self, code: u16, down: bool) -> Result<(), String> {
        self.send(EV_KEY, code, down as i32)?;
        self.send(EV_SYN, SYN_REPORT, 0)
    }

    /// The keyboard's name in the system's list of input devices ("input23"), for the live test.
    #[cfg(test)]
    pub fn sysname(&self) -> Option<String> {
        let mut buf = [0u8; 64];
        let n = unsafe { libc::ioctl(self.fd, UI_GET_SYSNAME_64, buf.as_mut_ptr()) };
        (n >= 0).then(|| String::from_utf8_lossy(&buf).trim_end_matches('\0').to_string())
    }
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.fd, UI_DEV_DESTROY);
            libc::close(self.fd);
        }
    }
}

/// The Linux key code for a step's key (a USB HID usage, by its name or
/// number), or for its media key.
pub fn code_of(step: &Value) -> Option<u16> {
    if let Some(m) = step.get("media") {
        let usage = m.as_str().and_then(media::code).or_else(|| m.as_u64().and_then(|u| u16::try_from(u).ok()))?;
        return MEDIA.iter().find(|(u, _)| *u == usage).map(|(_, c)| *c);
    }
    let k = step.get("key")?;
    let usage = k.as_str().and_then(keys::usage).or_else(|| k.as_u64().and_then(|u| u8::try_from(u).ok()))?;
    if keys::is_modifier(usage) {
        return Some(MODIFIERS[(usage - keys::FIRST_MODIFIER) as usize]);
    }
    // (the page's table of the kernel's codes for each usage, turned round)
    (1..256u32).find(|c| crate::z10::evdev_to_hid(*c) == Some((0, usage as u32))).map(|c| c as u16)
}

/// The kernel's codes for the modifiers, in the order of their usages (0xE0 up).
const MODIFIERS: [u16; 8] = [29, 42, 56, 125, 97, 54, 100, 126];

/// Each media key's consumer usage and the kernel's code for it.
const MEDIA: [(u16, u16); 18] = [
    (0xB5, 163),
    (0xB6, 165),
    (0xB7, 166),
    (0xCD, 164),
    (0xE2, 113),
    (0xE9, 115),
    (0xEA, 114),
    (0x183, 171),
    (0x18A, 155),
    (0x192, 140),
    (0x194, 144),
    (0x221, 217),
    (0x223, 172),
    (0x224, 158),
    (0x225, 159),
    (0x226, 128),
    (0x227, 173),
    (0x22A, 156),
];

/// What the player types on: Ectomatter's keyboard, or a test's.
trait Keys {
    fn key(&self, code: u16, down: bool) -> Result<(), String>;
}

impl Keys for Keyboard {
    fn key(&self, code: u16, down: bool) -> Result<(), String> {
        Keyboard::key(self, code, down)
    }
}

/// The least a pass of a repeating macro takes: the player hears its control
/// between passes, so a macro with no waits of its own still lets go.
const PASS_GAP: Duration = Duration::from_millis(20);

/// What the player is told.
enum Cmd {
    /// a control running a macro went down: its steps, how it plays
    Down(String, Vec<Value>, Plays),
    /// the control came back up
    Up(String),
}

/// Ectomatter's macros played, on a thread of their own, as the controls
/// running them are pressed and let go.
pub struct Player {
    tell: mpsc::Sender<Cmd>,
}

impl Player {
    /// The player, with its keyboard made when it first plays, and where it
    /// hands the jobs its macros' steps do (their ids, jobs.rs).
    pub fn start(jobs: Option<smithay_client_toolkit::reexports::calloop::channel::Sender<String>>) -> Player {
        let (tell, told) = mpsc::channel::<Cmd>();
        std::thread::Builder::new()
            .name("macros".into())
            .spawn(move || {
                play(told, Keyboard::new, |id: &str| {
                    if let Some(j) = &jobs {
                        let _ = j.send(id.to_string());
                    }
                })
            })
            .expect("the macro player's thread");
        Player { tell }
    }

    /// A control running a macro pressed (`control`, which device's which).
    pub fn down(&self, control: &str, steps: Vec<Value>, plays: Plays) {
        let _ = self.tell.send(Cmd::Down(control.to_string(), steps, plays));
    }

    pub fn up(&self, control: &str) {
        let _ = self.tell.send(Cmd::Up(control.to_string()));
    }
}

/// What is playing: which control's macro, its steps, how many passes are
/// left (none for over and over), where it is, and what it holds down.
struct Playing {
    control: String,
    steps: Vec<Value>,
    left: Option<u32>,
    at: usize,
    /// when this pass began, and when the next step is due
    began: Instant,
    until: Instant,
    held: Vec<u16>,
    /// a press of its own control ends it (Repeat until pressed again)
    toggles: bool,
    /// its control's release ends it (Repeat while held)
    holds: bool,
}

/// The player's thread: one macro at a time, the newest press taking over;
/// each job a step does handed on (`jobs`, its id) as the macro reaches it.
fn play<K: Keys>(told: mpsc::Receiver<Cmd>, make: impl Fn() -> Result<K, String>, jobs: impl Fn(&str)) {
    let mut kb: Option<K> = None;
    let mut now: Option<Playing> = None;
    loop {
        // the next step's moment, or nothing to do till told
        let cmd = match &now {
            Some(p) => match told.recv_timeout(p.until.saturating_duration_since(Instant::now())) {
                Ok(c) => Some(c),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
            None => match told.recv() {
                Ok(c) => Some(c),
                Err(_) => break,
            },
        };
        match cmd {
            Some(Cmd::Down(control, steps, plays)) => {
                let again = now.as_ref().is_some_and(|p| p.control == control && p.toggles);
                if let (Some(p), Some(k)) = (now.take(), &kb) {
                    let_go(k, &p.held);
                }
                if again || steps.is_empty() {
                    continue;
                }
                // (the keyboard made for a macro that types: one that only does
                // jobs plays without it)
                if kb.is_none() && steps.iter().any(|s| code_of(s).is_some()) {
                    match make() {
                        Ok(k) => kb = Some(k),
                        Err(e) => {
                            eprintln!("ectomatter: {e}");
                            continue;
                        }
                    }
                }
                let left = match plays {
                    Plays::Once => Some(1),
                    Plays::Times(n) => Some(n.max(1)),
                    Plays::WhileHeld | Plays::UntilPressedAgain => None,
                };
                let t = Instant::now();
                now = Some(Playing { control, steps, left, at: 0, began: t, until: t, held: Vec::new(), toggles: plays == Plays::UntilPressedAgain, holds: plays == Plays::WhileHeld });
            }
            Some(Cmd::Up(control)) => {
                if now.as_ref().is_some_and(|p| p.control == control && p.holds) {
                    if let (Some(p), Some(k)) = (now.take(), &kb) {
                        let_go(k, &p.held);
                    }
                }
            }
            None => {}
        }
        // the steps due now
        let Some(p) = &mut now else { continue };
        while Instant::now() >= p.until {
            let Some(step) = p.steps.get(p.at).cloned() else {
                // a pass done: again, or the end
                p.left = p.left.map(|l| l - 1);
                if p.left == Some(0) {
                    if let Some(k) = &kb {
                        let_go(k, &p.held);
                    }
                    now = None;
                    break;
                }
                p.at = 0;
                // (the next pass after a moment at least, and what the player was
                // told heard first)
                p.began = (p.began + PASS_GAP).max(Instant::now());
                p.until = p.began;
                break;
            };
            p.at += 1;
            let wait = step.get("wait").and_then(Value::as_u64).unwrap_or(0) + step.get("after").and_then(Value::as_u64).unwrap_or(0);
            if let Some(id) = step.get("job").and_then(Value::as_str) {
                jobs(id);
            } else if let (Some(code), Some(k)) = (code_of(&step), &kb) {
                let down = step["down"].as_bool().unwrap_or(false);
                if k.key(code, down).is_ok() {
                    if down {
                        p.held.push(code);
                    } else {
                        p.held.retain(|c| *c != code);
                    }
                }
            }
            p.until = Instant::now() + Duration::from_millis(wait);
        }
    }
    if let (Some(p), Some(k)) = (now, &kb) {
        let_go(k, &p.held);
    }
}

/// Let go of everything a macro holds down.
fn let_go(k: &impl Keys, held: &[u16]) {
    for c in held.iter().rev() {
        let _ = k.key(*c, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Each step's key and media key is the kernel's code for it.
    #[test]
    fn each_step_types_the_key_it_names() {
        assert_eq!(code_of(&json!({ "key": "A" })), Some(30));
        assert_eq!(code_of(&json!({ "key": "LeftCtrl" })), Some(29));
        assert_eq!(code_of(&json!({ "key": "LeftMeta" })), Some(125));
        assert_eq!(code_of(&json!({ "key": "Enter" })), Some(28));
        assert_eq!(code_of(&json!({ "key": "F13" })), Some(183));
        assert_eq!(code_of(&json!({ "key": "ArrowLeft" })), Some(105));
        assert_eq!(code_of(&json!({ "key": 4 })), Some(30));
        assert_eq!(code_of(&json!({ "media": "Mute" })), Some(113));
        assert_eq!(code_of(&json!({ "media": 0xE9 })), Some(115));
        assert_eq!(code_of(&json!({ "wait": 40 })), None);
    }

    /// A keyboard of the tests' own: what was typed on it.
    #[derive(Clone, Default)]
    struct Typed(std::sync::Arc<std::sync::Mutex<Vec<(u16, bool)>>>);

    impl Keys for Typed {
        fn key(&self, code: u16, down: bool) -> Result<(), String> {
            self.0.lock().unwrap().push((code, down));
            Ok(())
        }
    }

    impl Typed {
        fn now(&self) -> Vec<(u16, bool)> {
            self.0.lock().unwrap().clone()
        }
    }

    /// The player on a test keyboard: what to tell it, what it typed, and
    /// whether it stopped once there was nothing more to tell it.
    fn player() -> (mpsc::Sender<Cmd>, Typed, mpsc::Receiver<()>) {
        let (tell, typed, stopped, _) = player_with_jobs();
        (tell, typed, stopped)
    }

    /// The same, with the jobs its steps hand on, in order.
    #[allow(clippy::type_complexity)]
    fn player_with_jobs() -> (mpsc::Sender<Cmd>, Typed, mpsc::Receiver<()>, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        let typed = Typed::default();
        let jobs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (tell, told) = mpsc::channel();
        let (done, stopped) = mpsc::channel();
        let (t, j) = (typed.clone(), jobs.clone());
        std::thread::spawn(move || {
            play(told, move || Ok(t.clone()), move |id: &str| j.lock().unwrap().push(id.to_string()));
            let _ = done.send(());
        });
        (tell, typed, stopped, jobs)
    }

    /// A step that does one of Ectomatter's jobs is handed on as the macro
    /// reaches it, between the keys around it, and types nothing.
    #[test]
    fn a_macros_jobs_are_handed_on_in_order() {
        let (tell, typed, stopped, jobs) = player_with_jobs();
        let steps = vec![
            json!({ "job": "profile:z10:2", "after": 0 }),
            json!({ "key": "A", "down": true, "after": 0 }),
            json!({ "key": "A", "down": false, "after": 150 }),
            json!({ "job": "profile:sensei310:pq", "after": 0 }),
        ];
        tell.send(Cmd::Down("b".into(), steps, Plays::Once)).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(*jobs.lock().unwrap(), ["profile:z10:2"], "the second job waits for the wait before it");
        assert_eq!(typed.now(), [(30, true), (30, false)]);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(*jobs.lock().unwrap(), ["profile:z10:2", "profile:sensei310:pq"]);
        drop(tell);
        stopped.recv_timeout(Duration::from_secs(2)).expect("the player stops");
    }

    /// A macro that only does jobs plays where Ectomatter can't type (no
    /// keyboard can be made), as it types nothing; one that types waits for
    /// the keyboard, as before.
    #[test]
    fn a_macro_of_jobs_alone_needs_no_keyboard() {
        let jobs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (tell, told) = mpsc::channel();
        let j = jobs.clone();
        let t = std::thread::spawn(move || play::<Typed>(told, || Err("no keyboard here".into()), move |id: &str| j.lock().unwrap().push(id.to_string())));
        tell.send(Cmd::Down("a".into(), vec![json!({ "job": "profile:z10:2", "after": 0 }), json!({ "job": "profile:sensei310:pq", "after": 0 })], Plays::Once)).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        tell.send(Cmd::Down("b".into(), vec![json!({ "job": "profile:z10:3", "after": 0 }), json!({ "key": "A", "down": true, "after": 0 })], Plays::Once)).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(*jobs.lock().unwrap(), ["profile:z10:2", "profile:sensei310:pq"]);
        drop(tell);
        t.join().unwrap();
    }

    /// Each way a macro plays: once, a number of times, while its control is
    /// held and until it's pressed again. A repeating macro with no waits of
    /// its own still hears its control let go, or pressed again, and stops.
    #[test]
    fn a_macro_plays_each_way() {
        let tap = vec![json!({ "key": "A", "down": true, "after": 0 }), json!({ "key": "A", "down": false, "after": 0 })];
        let pass = [(30, true), (30, false)];
        let wait = |ms| std::thread::sleep(Duration::from_millis(ms));

        let (tell, typed, stopped) = player();
        tell.send(Cmd::Down("b".into(), tap.clone(), Plays::Once)).unwrap();
        wait(60);
        assert_eq!(typed.now(), pass);
        tell.send(Cmd::Down("b".into(), tap.clone(), Plays::Times(3))).unwrap();
        wait(60 + 3 * PASS_GAP.as_millis() as u64);
        assert_eq!(typed.now(), pass.repeat(4));
        drop(tell);
        stopped.recv_timeout(Duration::from_secs(2)).expect("the player stops");

        for plays in [Plays::WhileHeld, Plays::UntilPressedAgain] {
            let (tell, typed, stopped) = player();
            tell.send(Cmd::Down("b".into(), tap.clone(), plays)).unwrap();
            wait(100);
            // (let go, or pressed again)
            tell.send(if plays == Plays::WhileHeld { Cmd::Up("b".into()) } else { Cmd::Down("b".into(), tap.clone(), plays) }).unwrap();
            wait(40);
            let got = typed.now();
            let passes = got.len() / 2;
            assert!((2..=8).contains(&passes), "{plays:?}: {passes} passes in 100 ms");
            assert_eq!(got, pass.repeat(passes), "{plays:?}: whole passes, and nothing left down");
            wait(100);
            assert_eq!(typed.now().len(), got.len(), "{plays:?}: nothing more once stopped");
            drop(tell);
            stopped.recv_timeout(Duration::from_secs(2)).expect("the player stops");
        }
    }

    /// A macro played on Ectomatter's real keyboard types what it says, in
    /// order, and lets go of what it holds; heard on the keyboard's own device,
    /// held by this test alone so nothing it types reaches any app.
    /// (Run by hand: it makes a keyboard on this computer for a moment.)
    #[test]
    #[ignore]
    fn a_macro_types_on_ectomatters_keyboard() {
        let kb = Keyboard::new().expect("the keyboard");
        let name = kb.sysname().expect("its name");
        std::thread::sleep(Duration::from_millis(300));
        let node = std::fs::read_dir(format!("/sys/devices/virtual/input/{name}")).unwrap().flatten().find_map(|e| e.file_name().to_str().filter(|n| n.starts_with("event")).map(String::from)).expect("its event device");
        let path = std::ffi::CString::new(format!("/dev/input/{node}")).unwrap();
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
        assert!(fd >= 0, "its event device opens");
        // (held by this test alone: nothing typed reaches the desktop)
        assert_eq!(unsafe { libc::ioctl(fd, 0x4004_4590, 1 as libc::c_int) }, 0, "held by the test alone");
        let steps = vec![json!({ "key": "LeftShift", "down": true, "after": 0 }), json!({ "key": "A", "down": true, "after": 5 }), json!({ "key": "A", "down": false, "after": 0 })];
        // the shift is never let go by the steps: the player lets go of it
        let (tell, told) = mpsc::channel();
        let made = std::sync::Mutex::new(Some(kb));
        let t = std::thread::spawn(move || play(told, || made.lock().unwrap().take().ok_or_else(|| "made once".to_string()), |_: &str| {}));
        tell.send(Cmd::Down("test".into(), steps, Plays::Times(2))).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        // (read while the keyboard still exists)
        let mut got = Vec::new();
        let mut e = Event { sec: 0, usec: 0, kind: 0, code: 0, value: 0 };
        while unsafe { libc::read(fd, &mut e as *mut Event as *mut libc::c_void, std::mem::size_of::<Event>()) } == std::mem::size_of::<Event>() as isize {
            if e.kind == EV_KEY {
                got.push((e.code, e.value));
            }
        }
        drop(tell);
        t.join().unwrap();
        unsafe { libc::close(fd) };
        // (the second pass's shift is already down, so the system passes on no second press)
        assert_eq!(got, [(42, 1), (30, 1), (30, 0), (30, 1), (30, 0), (42, 0)], "twice, then the shift let go");
    }
}
