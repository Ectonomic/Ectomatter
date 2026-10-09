//! Ectodeck playing the Z10's screen, as the z10-lcd service did: the Stats
//! page once a second, and game-helper's launch and exit sequences breaking
//! in when its note changes. The loop is z10lcd.py's main loop, carried over
//! step for step (see [Core::tick]), so the keyboard shows the same frames at
//! the same moments; what Ectodeck adds sits around it: a pause, whether
//! launches break in, standing aside while the old service still runs, and
//! handing the screen back to the keyboard's own screens on quitting.
//!
//! Each frame sent is also written to `$XDG_RUNTIME_DIR/z10-lcd/frame`, as the
//! service wrote it: the keyboard will not read its screen back, so that file
//! is how anything else knows what it shows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::lcd_anim::{Ask, Frame, Playing};
use crate::lcd_stats::Stats;

/// seconds between readings of the Stats page
const INTERVAL: f64 = 1.0;

pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })))
}

// ------------------------------------------------------------------ the keyboard's screen

/// Report 10 as the service built it: `0a ea 02 <cmd> 00 00 00 <checksum>` and
/// eighty Windows-1252 characters, four rows of twenty. `cmd` 1 draws them;
/// 0 hands the screen back to the keyboard's own screens.
pub fn frame(rows: &[String], cmd: u8) -> [u8; 88] {
    let mut text = Vec::with_capacity(80);
    for i in 0..4 {
        let r: Vec<char> = rows.get(i).map_or("", |s| s.as_str()).chars().take(20).collect();
        for k in 0..20 {
            let c = r.get(k).copied().unwrap_or(' ');
            text.push(match c as u32 {
                0..=0x7f | 0xa0..=0xff => c as u32 as u8,
                _ => b'?',
            });
        }
    }
    let mut buf = [0u8; 88];
    buf[0..4].copy_from_slice(&[0x0a, 0xea, 0x02, cmd]);
    buf[7] = 0u8.wrapping_sub(text.iter().fold(0u8, |a, b| a.wrapping_add(*b)));
    buf[8..88].copy_from_slice(&text);
    buf
}

/// Where frames go: the keyboard, or a test's list.
pub trait Sink: Send {
    /// Find the keyboard's screen; false if it isn't there.
    fn find(&mut self) -> bool;
    fn send(&mut self, rows: &[String], cmd: u8) -> Result<(), ()>;
}

/// The Z10's screen: the hidraw node of its display (the one whose report
/// descriptor has report 10), written with a feature report, opened for each
/// frame as the service did.
#[derive(Default)]
pub struct Hidraw {
    dev: Option<PathBuf>,
}

impl Hidraw {
    fn look() -> Option<PathBuf> {
        let mut nodes: Vec<PathBuf> = std::fs::read_dir("/sys/class/hidraw").ok()?.flatten().map(|e| e.path()).collect();
        nodes.sort();
        nodes.into_iter().find_map(|h| {
            let uevent = std::fs::read_to_string(h.join("device/uevent")).ok()?;
            if !uevent.contains("HID_ID=0003:00003842:00002611") {
                return None;
            }
            let d = std::fs::read(h.join("device/report_descriptor")).ok()?;
            let name = h.file_name()?.to_owned();
            d.windows(2).any(|w| w == [0x85, 0x0a]).then(|| PathBuf::from("/dev").join(name))
        })
    }
}

impl Sink for Hidraw {
    fn find(&mut self) -> bool {
        if self.dev.is_none() {
            self.dev = Hidraw::look();
        }
        self.dev.is_some()
    }

    fn send(&mut self, rows: &[String], cmd: u8) -> Result<(), ()> {
        use std::os::fd::AsRawFd;
        let Some(dev) = &self.dev else { return Err(()) };
        let f = std::fs::OpenOptions::new().read(true).write(true).open(dev).map_err(|_| ())?;
        let mut buf = frame(rows, cmd);
        // HIDIOCSFEATURE(88)
        let req = (3u64 << 30) | (88u64 << 16) | ((b'H' as u64) << 8) | 0x06;
        let rc = unsafe { libc::ioctl(f.as_raw_fd(), req as _, buf.as_mut_ptr()) };
        if rc < 0 {
            self.dev = None;
            return Err(());
        }
        Ok(())
    }
}

/// Write the frame just sent where other programs read it: whole, then moved
/// into place, so a reader never catches it half written. A courtesy: a
/// failure never stops the screen.
fn publish(path: &Path, rows: &[String]) {
    let text: Vec<String> = (0..4).map(|i| {
        let r: String = rows.get(i).map_or("", |s| s.as_str()).chars().take(20).collect();
        format!("{r:<20}")
    }).collect();
    let tmp = path.with_extension("new");
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if std::fs::write(&tmp, text.join("\n")).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

// ------------------------------------------------------------------ game-helper's note

/// The note's signature (when it was written, its size, its inode) and its fields.
fn read_event(path: &Path) -> (Option<(i128, u64, u64)>, Option<HashMap<String, String>>) {
    use std::os::unix::fs::MetadataExt;
    let Ok(m) = std::fs::metadata(path) else { return (None, None) };
    let Ok(bytes) = std::fs::read(path) else { return (None, None) };
    let text = String::from_utf8_lossy(&bytes);
    let mut fields = HashMap::new();
    for line in text.split_inclusive('\n') {
        let line = line.strip_suffix('\n').unwrap_or(line);
        if let Some((k, v)) = line.split_once('=') {
            fields.insert(k.to_string(), v.to_string());
        }
    }
    let sig = (m.mtime() as i128 * 1_000_000_000 + m.mtime_nsec() as i128, m.size(), m.ino());
    (Some(sig), Some(fields))
}

/// A game's name in plain letters: codes above 127 draw noise on the
/// keyboard, so accents lose their marks and symbols such as TM are dropped.
pub fn plain(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let kept: String = text.chars().filter(|c| !['\u{2122}', '\u{00ae}', '\u{00a9}'].contains(c)).collect();
    let ascii: String = kept.nfkd().filter(|c| c.is_ascii()).collect();
    ascii.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn int(v: Option<&String>, default: i64) -> i64 {
    v.and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(default)
}

/// Turns game-helper's notes into the launch and exit sequences.
struct Session {
    key: Option<(Option<String>, Option<String>)>,
    ev: Arc<Mutex<HashMap<String, String>>>,
    frames: Option<Playing>,
    /// which sequence is playing: "launch" or "exit"
    kind: Option<&'static str>,
}

fn alive(ev: &HashMap<String, String>) -> bool {
    Path::new(&format!("/proc/{}", int(ev.get("pid"), -1))).exists()
}

impl Session {
    fn new() -> Session {
        Session { key: None, ev: Arc::new(Mutex::new(HashMap::new())), frames: None, kind: None }
    }

    fn update(&mut self, ev: HashMap<String, String>, shown: Vec<String>, stats: Ask<Vec<String>>, seed: &mut dyn FnMut() -> u64) {
        *self.ev.lock().unwrap() = ev.clone();
        let key = (ev.get("session").cloned(), ev.get("phase").cloned());
        if self.key.as_ref() == Some(&key) {
            // progress on the sequence already playing
            return;
        }
        self.key = Some(key);
        let name = Some(plain(ev.get("name").map_or("", |s| s.as_str()))).filter(|n| !n.is_empty()).unwrap_or_else(|| "GAME".into());
        let steps: Vec<String> = ev.get("steps").map_or("", |s| s.as_str()).split('|').filter(|x| !x.is_empty()).map(String::from).collect();
        // (if game-helper has gone away, stop waiting on it)
        let shared = self.ev.clone();
        let done: Ask<i64> = Arc::new(move || {
            let ev = shared.lock().unwrap();
            if alive(&ev) { int(ev.get("done"), 0) } else { 1_000_000 }
        });
        match ev.get("phase").map(|s| s.as_str()) {
            Some("launch") => {
                self.frames = Some(Playing::launch(seed(), name, steps, done, stats, shown));
                self.kind = Some("launch");
            }
            Some("exit") => {
                let minutes = ((int(ev.get("ended"), 0) - int(ev.get("started"), 0)).div_euclid(60)).max(0);
                let shared = self.ev.clone();
                let finished: Ask<bool> = Arc::new(move || {
                    let ev = shared.lock().unwrap();
                    ev.get("finished").is_some_and(|f| f == "1") || !alive(&ev)
                });
                self.frames = Some(Playing::exit(seed(), name, steps, minutes, done, finished, stats, shown));
                self.kind = Some("exit");
            }
            _ => {}
        }
    }

    fn next(&mut self) -> Option<Frame> {
        let f = self.frames.as_mut()?.next();
        if f.is_none() {
            self.frames = None;
        }
        f
    }
}

// ------------------------------------------------------------------ the loop

/// What Ectodeck asks of the player.
pub enum Control {
    /// whether game-helper's launches and exits break in
    Announce(bool),
    /// hold the screen as it is, or carry on
    Paused(bool),
    /// stop, and hand the screen back to the keyboard's own screens (or, with
    /// false, leave it blank); the sender hears once it has
    Quit(bool, mpsc::Sender<()>),
}

/// What the player tells Ectodeck: what the screen shows, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct News {
    pub rows: Vec<String>,
    /// the sequence playing, "launch" or "exit", while one plays
    pub sequence: Option<&'static str>,
    pub state: State,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Playing,
    Paused,
    /// the z10-lcd service is drawing on the screen: Ectodeck waits for it to stop
    Other,
    /// the keyboard's screen isn't there
    Missing,
}

/// The service's loop, one turn at a time, with what it needs handed in.
pub struct Core {
    pub sink: Box<dyn Sink>,
    pub stats: Ask<Vec<String>>,
    pub event: PathBuf,
    pub frame_file: Option<PathBuf>,
    pub trace: Option<PathBuf>,
    /// the seed each new sequence's random choices start from
    pub seeds: Box<dyn FnMut() -> u64 + Send>,
    session: Session,
    shown: Option<Vec<String>>,
    last_sig: Option<(i128, u64, u64)>,
    frame_until: f64,
    stats_at: f64,
    pub announce: bool,
    pub paused: bool,
    /// what was pushed, for the page
    pub pushed: Option<Vec<String>>,
}

impl Core {
    pub fn new(sink: Box<dyn Sink>, stats: Ask<Vec<String>>, event: PathBuf, seeds: Box<dyn FnMut() -> u64 + Send>) -> Core {
        // a note already on disk at start-up is from before this ran
        let (last_sig, _) = read_event(&event);
        Core {
            sink,
            stats,
            event,
            frame_file: None,
            trace: None,
            seeds,
            session: Session::new(),
            shown: None,
            last_sig,
            frame_until: 0.0,
            stats_at: 0.0,
            announce: true,
            paused: false,
            pushed: None,
        }
    }

    fn push(&mut self, rows: Vec<String>, now: f64) {
        if !self.sink.find() {
            return;
        }
        if self.shown.as_ref() == Some(&rows) {
            return;
        }
        if self.sink.send(&rows, 1).is_ok() {
            if let Some(f) = &self.frame_file {
                publish(f, &rows);
            }
            if let Some(t) = &self.trace {
                use std::io::Write;
                if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(t) {
                    let _ = writeln!(file, "{now:.3} {}", rows.join("|"));
                }
            }
            self.pushed = Some(rows.clone());
            self.shown = Some(rows);
        } else {
            // unplugged or found anew; look again next time
            self.shown = None;
        }
    }

    /// Start afresh after standing aside: the next frame is sent whatever was last.
    pub fn forget_shown(&mut self) {
        self.shown = None;
        self.stats_at = 0.0;
    }

    /// One turn of the loop at `now` (seconds); false when the next turn
    /// should come at once, without the usual 10 ms sleep.
    pub fn tick(&mut self, now: f64) -> bool {
        let (sig, ev) = read_event(&self.event);
        if sig != self.last_sig {
            self.last_sig = sig;
            // (a note with nothing in it is no note)
            if let (Some(ev), true) = (ev.filter(|e| !e.is_empty()), self.announce) {
                let start = match &self.shown {
                    Some(s) => s.clone(),
                    None => (self.stats)(),
                };
                self.session.update(ev, start, self.stats.clone(), &mut self.seeds);
                self.frame_until = 0.0;
            }
        }
        if self.session.frames.is_some() {
            if now >= self.frame_until {
                match self.session.next() {
                    Some(f) => {
                        self.push(f.rows, now);
                        self.frame_until = now + f.ms as f64 / 1000.0;
                    }
                    None => {
                        self.stats_at = 0.0;
                        return false;
                    }
                }
            }
        } else if now >= self.stats_at && !self.paused {
            let rows = (self.stats)();
            self.push(rows, now);
            self.stats_at = now + INTERVAL;
        }
        true
    }

    /// Stop any sequence playing (launches no longer break in).
    pub fn drop_sequence(&mut self) {
        self.session.frames = None;
        self.stats_at = 0.0;
    }

    pub fn playing_sequence(&self) -> bool {
        self.session.frames.is_some()
    }

    /// The sequence playing, "launch" or "exit"; None while the Stats page shows.
    pub fn sequence(&self) -> Option<&'static str> {
        self.session.frames.as_ref().and(self.session.kind)
    }

    /// Hand the screen back: blank it, then, unless `own` is false, leave the
    /// screen to the keyboard's own screens, as EVGA's software does on closing.
    pub fn hand_back(&mut self, own: bool) {
        if !self.sink.find() {
            return;
        }
        let blank = vec![String::new(); 4];
        if self.sink.send(&blank, 1).is_ok() {
            if own {
                let _ = self.sink.send(&blank, 0);
            }
            if let Some(f) = &self.frame_file {
                publish(f, &blank);
            }
        }
    }
}

/// Whether the z10-lcd service (z10lcd.py) is running: then it draws on the
/// screen, and Ectodeck stands aside rather than fight it frame for frame.
fn other_running() -> bool {
    let me = std::process::id().to_string();
    let Ok(dir) = std::fs::read_dir("/proc") else { return false };
    dir.flatten().any(|e| {
        let n = e.file_name();
        let n = n.to_string_lossy();
        n.bytes().all(|b| b.is_ascii_digit()) && n != me && std::fs::read(e.path().join("cmdline")).is_ok_and(|c| c.split(|&b| b == 0).any(|a| a.ends_with(b"z10lcd.py")))
    })
}

/// The player, on a thread of its own.
pub struct Player {
    tell: mpsc::Sender<Control>,
}

impl Player {
    /// Start playing; each frame sent, and each change of state, goes to `news`.
    pub fn start(news: impl Fn(News) + Send + 'static, announce: bool, paused: bool) -> Player {
        let (tell, heard) = mpsc::channel::<Control>();
        std::thread::spawn(move || {
            let stats = Arc::new(Mutex::new(Stats::here()));
            // primed, as the service primed it: the first reading of the load is 0
            stats.lock().unwrap().cpu_load();
            let reader = stats.clone();
            let ask: Ask<Vec<String>> = Arc::new(move || reader.lock().unwrap().rows());
            let seeds = Box::new(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64));
            let mut core = Core::new(Box::new(Hidraw::default()), ask, runtime_dir().join("z10-lcd/session"), seeds);
            core.frame_file = Some(runtime_dir().join("z10-lcd/frame"));
            core.trace = std::env::var_os("Z10_LCD_TRACE").map(PathBuf::from);
            core.announce = announce;
            core.paused = paused;
            let start = Instant::now();
            let (mut other, mut other_at) = (other_running(), Instant::now());
            let mut told: Option<(State, Option<&'static str>)> = None;
            loop {
                // what Ectodeck asks
                while let Ok(c) = heard.try_recv() {
                    match c {
                        Control::Announce(on) => {
                            core.announce = on;
                            if !on && core.playing_sequence() {
                                core.drop_sequence();
                            }
                        }
                        Control::Paused(p) => {
                            core.paused = p;
                            if !p {
                                core.stats_at = 0.0;
                            }
                        }
                        Control::Quit(own, done) => {
                            if !other {
                                core.hand_back(own);
                            }
                            let _ = done.send(());
                            return;
                        }
                    }
                }
                if other_at.elapsed() > Duration::from_secs(2) {
                    other_at = Instant::now();
                    let was = other;
                    other = other_running();
                    if was && !other {
                        core.forget_shown();
                    }
                }
                let again = if other { true } else { core.tick(start.elapsed().as_secs_f64()) };
                let found = other || core.sink.find();
                let state = if other { State::Other } else if !found { State::Missing } else if core.paused && !core.playing_sequence() { State::Paused } else { State::Playing };
                let sequence = core.sequence();
                if let Some(rows) = core.pushed.take() {
                    news(News { rows, sequence, state });
                    told = Some((state, sequence));
                } else if told != Some((state, sequence)) {
                    news(News { rows: core.shown_rows(), sequence, state });
                    told = Some((state, sequence));
                }
                if again {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        });
        Player { tell }
    }

    pub fn tell(&self, c: Control) {
        let _ = self.tell.send(c);
    }

    /// Stop, handing the screen back; waits at most half a second for it.
    pub fn quit(&self, own: bool) {
        let (done, heard) = mpsc::channel();
        if self.tell.send(Control::Quit(own, done)).is_ok() {
            let _ = heard.recv_timeout(Duration::from_millis(500));
        }
    }
}

impl Core {
    fn shown_rows(&self) -> Vec<String> {
        self.shown.clone().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_as_the_service_built_them() {
        let f = frame(&["CPU 64° 5%".into(), "AB".into()], 1);
        assert_eq!(&f[0..4], &[0x0a, 0xea, 0x02, 0x01]);
        assert_eq!(f[8 + 6], 0xB0);
        assert_eq!(f[8..88].iter().fold(0u8, |a, b| a.wrapping_add(*b)).wrapping_add(f[7]), 0);
        assert_eq!(frame(&[], 0)[3], 0);
    }

    #[test]
    fn names_in_plain_letters() {
        assert_eq!(plain("Pokémon™  Legends:  Z-A"), "Pokemon Legends: Z-A");
        assert_eq!(plain("ＦＵＬＬ ﬁre ©"), "FULL fire");
    }
}
