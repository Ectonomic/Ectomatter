//! Ectodeck playing the Z10's screen, as the z10-lcd service did: the Stats
//! page once a second, and game-helper's launch and exit sequences breaking
//! in when its note changes. The loop is z10lcd.py's main loop, carried over
//! step for step (see [Core::tick]), so the keyboard shows the same frames at
//! the same moments; what Ectodeck adds sits around it: a pause, whether
//! launches break in, standing aside while the old service still runs or while
//! the Screen tab shows the keyboard's own status screen, and handing the screen
//! back to the keyboard's own screens on quitting.
//!
//! Each frame sent is also written to `$XDG_RUNTIME_DIR/z10-lcd/frame`, as the
//! service wrote it: the keyboard will not read its screen back, so that file
//! is how anything else knows what it shows.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
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
    /// hold the screen as it is, or carry on
    Paused(bool),
    /// stop, and hand the screen back to the keyboard's own screens (or, with
    /// false, leave it blank); the sender hears once it has
    Quit(bool, mpsc::Sender<()>),
    /// the pages to play in turn (an edit is seen as it is made)
    Pages(Vec<crate::lcd_page::Page>),
    /// show this page now, as it is, and carry on from it
    Show(usize),
    /// stay on this page, which is being edited; None carries on
    Hold(Option<usize>),
    /// end the launch or exit sequence playing, back to the playlist
    EndSequence,
    /// leave the screen to the keyboard's own status screen, for the Screen tab's
    /// picture of it, or (false) take it back (see [Core::stand_aside])
    Aside(bool),
    /// what a page may show besides the computer's readings: the profile the
    /// keyboard is using (its number and name), and the weather outside (°C, the
    /// sky in words)
    Context(Option<(u8, String)>, Option<(f64, String)>),
}

/// What the player tells Ectodeck: what the screen shows, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct News {
    pub rows: Vec<String>,
    /// the sequence playing, "launch" or "exit", while one plays
    pub sequence: Option<&'static str>,
    pub state: State,
    /// which page of the playlist shows, or is coming in
    pub page: usize,
    /// where the playlist stands in that page's turn: whether the page is
    /// still coming in, the seconds that part has run, and how long it runs
    /// (coming in, its frames; showing, the page's own time)
    pub turning: bool,
    pub elapsed: f64,
    pub length: f64,
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
    /// the page changed: show it at the next turn, paused or not
    pub redraw: bool,
    /// the playlist: each page's seconds and how it comes in, in order; the
    /// page showing, which `stats` reads; staying on it while it is edited
    pub playlist: Vec<(f64, crate::lcd_page::Transition)>,
    pub current: Arc<AtomicUsize>,
    pub hold: bool,
    /// when the page showing came on; None until its first frame
    page_since: Option<f64>,
    /// the frames carrying the screen to the next page, still to show; when
    /// they began, and how long they take in all
    between: VecDeque<Frame>,
    turn_from: f64,
    turn_len: f64,
    /// paused: how long the page had shown, which it carries on from
    held: Option<f64>,
    /// standing aside: the keyboard shows its own status screen, and nothing is drawn
    pub aside: bool,
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
            redraw: false,
            playlist: Vec::new(),
            current: Arc::new(AtomicUsize::new(0)),
            hold: false,
            page_since: None,
            between: VecDeque::new(),
            turn_from: 0.0,
            turn_len: 0.0,
            held: None,
            aside: false,
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
        // (standing aside, the keyboard's own screen shows: nothing is drawn)
        if self.aside {
            return true;
        }
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
                // (a game's launch breaks in on a page coming in, too)
                self.between.clear();
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
                        // the page it ends on gets its whole time
                        self.page_since = None;
                        return false;
                    }
                }
            }
        } else if !self.between.is_empty() {
            if now >= self.frame_until {
                let f = self.between.pop_front().expect("a frame between pages");
                self.push(f.rows, now);
                self.frame_until = now + f.ms as f64 / 1000.0;
                if self.between.is_empty() {
                    // the page itself, once the last frame has had its time
                    self.stats_at = self.frame_until;
                }
            }
        } else if self.turn_due(now) {
            self.turn(now);
        } else if (now >= self.stats_at && !self.paused) || self.redraw {
            self.redraw = false;
            let rows = (self.stats)();
            self.push(rows, now);
            self.stats_at = now + INTERVAL;
            if self.page_since.is_none() {
                self.page_since = Some(now);
            }
        }
        true
    }

    /// The page showing.
    pub fn page(&self) -> usize {
        self.current.load(Ordering::Relaxed)
    }

    /// Whether the page showing has had its time and another should come in.
    fn turn_due(&self, now: f64) -> bool {
        let n = self.playlist.len();
        !self.paused && !self.hold && n > 1 && self.page_since.is_some_and(|t| now >= t + self.playlist[self.page() % n].0)
    }

    /// The next page comes in: the frames between the page shown and it,
    /// then the page itself.
    fn turn(&mut self, now: f64) {
        let n = self.playlist.len();
        let next = (self.page() + 1) % n;
        let old = self.shown.clone().unwrap_or_default();
        self.current.store(next, Ordering::Relaxed);
        let new = (self.stats)();
        let seed = (self.seeds)();
        let frames = crate::lcd_anim::page_turn(&old, &new, self.playlist[next].1, seed);
        self.page_since = None;
        if frames.is_empty() {
            self.push(new, now);
            self.stats_at = now + INTERVAL;
            self.page_since = Some(now);
        } else {
            self.turn_from = now;
            self.turn_len = frames.iter().map(|f| f.ms as f64 / 1000.0).sum();
            self.between = frames.into();
            self.frame_until = now;
        }
    }

    /// Where the playlist stands at `now`: whether a page is coming in, how
    /// long that part has run, and how long it runs (see [News]).
    pub fn standing(&self, now: f64) -> (bool, f64, f64) {
        let n = self.playlist.len();
        if self.session.frames.is_some() || n == 0 {
            return (false, 0.0, 0.0);
        }
        if !self.between.is_empty() {
            return (true, (now - self.turn_from).max(0.0), self.turn_len);
        }
        let length = self.playlist[self.page() % n].0;
        let elapsed = match (self.held, self.page_since) {
            (Some(h), _) => h,
            (None, Some(t)) => now - t,
            (None, None) => 0.0,
        };
        (false, elapsed.clamp(0.0, length), length)
    }

    /// Hold the screen as it is, or (false) carry on from where it stood:
    /// the page keeps the time it had shown.
    pub fn pause(&mut self, paused: bool, now: f64) {
        if paused == self.paused {
            return;
        }
        self.paused = paused;
        // (standing aside, the page's time is already held)
        if self.aside {
            return;
        }
        if paused {
            self.held = self.page_since.map(|t| now - t);
        } else {
            self.stats_at = 0.0;
            if let Some(h) = self.held.take() {
                self.page_since = Some(now - h);
            }
        }
    }

    /// The pages to play, and how long each shows and comes in.
    pub fn set_playlist(&mut self, list: Vec<(f64, crate::lcd_page::Transition)>) {
        if self.page() >= list.len() {
            self.current.store(0, Ordering::Relaxed);
        }
        self.playlist = list;
        self.redraw = true;
    }

    /// Show page `i` now and carry on from it.
    pub fn show(&mut self, i: usize) {
        self.current.store(i.min(self.playlist.len().saturating_sub(1)), Ordering::Relaxed);
        self.between.clear();
        self.page_since = None;
        self.redraw = true;
    }

    /// Stay on page `i` while it is edited, or (None) carry on.
    pub fn hold_on(&mut self, i: Option<usize>) {
        match i {
            Some(i) => {
                self.hold = true;
                self.show(i);
            }
            None => {
                self.hold = false;
                self.page_since = None;
            }
        }
    }

    /// Leave the screen to the keyboard's own status screen: the screen blanked, then
    /// handed back, so no row the status screen leaves empty keeps a page's words. Or
    /// (false) take it back, the page drawn again at once. The page keeps the time it had
    /// shown, as through a pause; a sequence playing ends, and a game's note that comes
    /// meanwhile is past by then, so no sequence plays late.
    pub fn stand_aside(&mut self, aside: bool, now: f64) {
        if aside == self.aside {
            return;
        }
        self.aside = aside;
        if aside {
            if !self.paused {
                self.held = self.page_since.map(|t| now - t);
            }
            self.drop_sequence();
            self.between.clear();
            self.hand_back(true);
        } else {
            if !self.paused {
                if let Some(h) = self.held.take() {
                    self.page_since = Some(now - h);
                }
            }
            self.last_sig = read_event(&self.event).0;
            self.forget_shown();
            self.redraw = true;
        }
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
    pub fn hand_back(&mut self, own: bool) -> bool {
        if !self.sink.find() {
            return false;
        }
        let blank = vec![String::new(); 4];
        let mut done = false;
        if self.sink.send(&blank, 1).is_ok() {
            done = !own || self.sink.send(&blank, 0).is_ok();
            if let Some(f) = &self.frame_file {
                publish(f, &blank);
            }
        }
        done
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
            // the pages, the one showing, and what they may show besides the computer's readings
            let shared = Arc::new(Mutex::new((vec![crate::lcd_page::Page::stats()], None::<(u8, String)>, None::<(f64, String)>)));
            let current = Arc::new(AtomicUsize::new(0));
            let (reader, page, at) = (stats.clone(), shared.clone(), current.clone());
            let ask: Ask<Vec<String>> = Arc::new(move || {
                let mut r = reader.lock().unwrap().read();
                let p = page.lock().unwrap();
                r.now = Some(crate::lcd_page::Moment::now());
                r.profile = p.1.as_ref().map(|(_, name)| name.clone());
                r.profile_number = p.1.as_ref().map(|(n, _)| *n);
                r.outside = p.2.clone();
                let i = at.load(Ordering::Relaxed);
                p.0.get(i).or(p.0.first()).map_or_else(Vec::new, |page| crate::lcd_page::render(page, &r))
            });
            let seeds = Box::new(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64));
            let mut core = Core::new(Box::new(Hidraw::default()), ask, runtime_dir().join("z10-lcd/session"), seeds);
            core.frame_file = Some(runtime_dir().join("z10-lcd/frame"));
            core.trace = std::env::var_os("Z10_LCD_TRACE").map(PathBuf::from);
            core.announce = announce;
            core.paused = paused;
            core.current = current;
            let start = Instant::now();
            let (mut other, mut other_at) = (other_running(), Instant::now());
            let mut told: Option<(State, Option<&'static str>, usize, bool)> = None;
            loop {
                // what Ectodeck asks
                while let Ok(c) = heard.try_recv() {
                    match c {
                        Control::Paused(p) => core.pause(p, start.elapsed().as_secs_f64()),
                        Control::EndSequence => {
                            if core.playing_sequence() {
                                core.drop_sequence();
                                core.restart_page();
                            }
                        }
                        Control::Pages(pages) => {
                            core.set_playlist(pages.iter().map(|p| (p.secs as f64, p.comes_in)).collect());
                            shared.lock().unwrap().0 = pages;
                        }
                        Control::Show(i) => core.show(i),
                        Control::Aside(a) => {
                            if a != core.aside {
                                eprintln!("ectodeck-native: the screen {}", if a { "left to the keyboard's own status screen" } else { "taken back for the playlist" });
                            }
                            core.stand_aside(a, start.elapsed().as_secs_f64());
                        }
                        Control::Hold(i) => core.hold_on(i),
                        Control::Context(profile, outside) => {
                            let mut s = shared.lock().unwrap();
                            if s.1 != profile || s.2 != outside {
                                (s.1, s.2) = (profile, outside);
                                core.redraw = true;
                            }
                        }
                        Control::Quit(own, done) => {
                            if !other {
                                let done = core.hand_back(own);
                                eprintln!("ectodeck-native: the screen {}", if !done { "could not be handed back" } else if own { "handed back to the keyboard's own screens" } else { "left blank" });
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
                let (sequence, page) = (core.sequence(), core.page());
                let (turning, elapsed, length) = core.standing(start.elapsed().as_secs_f64());
                // (said again as the page comes on after coming in, so the time it shows from is right)
                let now_told = (state, sequence, page, turning);
                if let Some(rows) = core.pushed.take() {
                    news(News { rows, sequence, state, page, turning, elapsed, length });
                    told = Some(now_told);
                } else if told != Some(now_told) {
                    news(News { rows: core.shown_rows(), sequence, state, page, turning, elapsed, length });
                    told = Some(now_told);
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

    /// The page showing starts its time again (after a pause).
    fn restart_page(&mut self) {
        self.page_since = None;
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

    /// A screen that keeps what it is sent, and when.
    struct Kept(Arc<Mutex<Vec<(f64, Vec<String>)>>>, Arc<Mutex<f64>>);

    impl Sink for Kept {
        fn find(&mut self) -> bool {
            true
        }
        fn send(&mut self, rows: &[String], _cmd: u8) -> Result<(), ()> {
            let t = *self.1.lock().unwrap();
            self.0.lock().unwrap().push((t, rows.to_vec()));
            Ok(())
        }
    }

    /// A player of two pages, A for 3 s and B for 5 s, B coming in with `how`;
    /// the pages say which they are and the second the clock is in.
    fn two_pages(how: crate::lcd_page::Transition) -> (Core, Arc<Mutex<Vec<(f64, Vec<String>)>>>, Arc<Mutex<f64>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let clock = Arc::new(Mutex::new(0.0));
        let current = Arc::new(AtomicUsize::new(0));
        let (at, c) = (current.clone(), clock.clone());
        let ask: Ask<Vec<String>> = Arc::new(move || {
            let page = ["A", "B"][at.load(Ordering::Relaxed)];
            vec![format!("PAGE {page}"), format!("{:.0}", *c.lock().unwrap())]
        });
        let note = std::env::temp_dir().join(format!("ectodeck-turn-{}-{:?}", std::process::id(), how));
        let _ = std::fs::remove_file(&note);
        let mut core = Core::new(Box::new(Kept(sent.clone(), clock.clone())), ask, note, Box::new(|| 7));
        core.current = current;
        core.set_playlist(vec![(3.0, crate::lcd_page::Transition::Wipe), (5.0, how)]);
        (core, sent, clock)
    }

    /// Run `core` from `from` to `to` seconds, 10 ms a turn.
    fn run(core: &mut Core, clock: &Arc<Mutex<f64>>, from: f64, to: f64) {
        let mut t = from;
        while t < to {
            *clock.lock().unwrap() = t;
            core.tick(t);
            t += 0.01;
        }
    }

    fn first_at(sent: &Arc<Mutex<Vec<(f64, Vec<String>)>>>, row0: &str) -> Option<f64> {
        sent.lock().unwrap().iter().find(|(_, r)| r[0] == row0).map(|(t, _)| *t)
    }

    #[test]
    fn pages_play_in_turn_each_for_its_time() {
        use crate::lcd_page::Transition;
        // Cut: B shows the moment A's three seconds are up; A again after B's five
        let (mut core, sent, clock) = two_pages(Transition::Cut);
        run(&mut core, &clock, 0.0, 9.0);
        let b = first_at(&sent, "PAGE B").expect("B shows");
        assert!((b - 3.0).abs() < 0.02, "B at {b}");
        // (A comes in with a wipe: its first frame as B's five seconds end)
        let back = sent.lock().unwrap().iter().find(|(t, r)| *t > b && r[0] != "PAGE B").map(|(t, _)| *t).expect("A again");
        assert!((back - 8.0).abs() < 0.05, "A again at {back}");
        // every second the page shows is read again
        assert!(sent.lock().unwrap().iter().any(|(t, r)| r[0] == "PAGE B" && *t > b + 0.9));

        // Wipe: 21 frames of 25 ms between, then B, its five seconds from then
        let (mut core, sent, clock) = two_pages(Transition::Wipe);
        run(&mut core, &clock, 0.0, 9.0);
        let wipe: Vec<f64> = sent.lock().unwrap().iter().filter(|(t, r)| *t < 5.0 && r[0].contains('#')).map(|(t, _)| *t).collect();
        assert_eq!(wipe.len(), 20, "the frames with a # in them (the first is A as it was)");
        assert!((wipe[0] - 3.0).abs() < 0.05);
        let b = first_at(&sent, "PAGE B").unwrap();
        // (each 25 ms frame waits for the loop's next 10 ms turn, as the service's did: about 30 ms)
        assert!(b > 3.0 + 21.0 * 0.025 && b < 3.0 + 21.0 * 0.031, "B at {b}");
        let back = sent.lock().unwrap().iter().filter(|(t, r)| *t > b && r[0].contains('#')).map(|(t, _)| *t).next().unwrap();
        assert!((back - (b + 5.0)).abs() < 0.05, "A comes in again at {back}");
    }

    #[test]
    fn paused_or_edited_the_page_stays() {
        use crate::lcd_page::Transition;
        let (mut core, sent, clock) = two_pages(Transition::Cut);
        // A shows for a second, then the screen holds for nine
        run(&mut core, &clock, 0.0, 1.0);
        core.pause(true, 1.0);
        run(&mut core, &clock, 1.0, 10.0);
        assert!(first_at(&sent, "PAGE B").is_none(), "paused, no page turns");
        assert_eq!(core.standing(10.0).0, false);
        assert!((core.standing(10.0).1 - 1.0).abs() < 0.02, "the time A had shown, held: {:?}", core.standing(10.0));
        // A carries on with the two seconds it had left
        core.pause(false, 10.0);
        run(&mut core, &clock, 10.0, 11.9);
        assert!(first_at(&sent, "PAGE B").is_none());
        run(&mut core, &clock, 11.9, 13.5);
        let b = first_at(&sent, "PAGE B").expect("B after A's three seconds in all");
        assert!((b - 12.0).abs() < 0.05, "B at {b}");
        // held on A while A is edited: shown at once, and it stays
        core.hold_on(Some(0));
        run(&mut core, &clock, 13.5, 30.0);
        let last_b = sent.lock().unwrap().iter().rev().find(|(_, r)| r[0] == "PAGE B").map(|(t, _)| *t).unwrap();
        assert!(last_b < 13.6, "B last at {last_b}");
        // done editing: A has its whole time from its next reading (within a second), then B
        core.hold_on(None);
        run(&mut core, &clock, 30.0, 32.9);
        assert!(!sent.lock().unwrap().iter().any(|(t, r)| r[0] == "PAGE B" && *t > 30.0));
        run(&mut core, &clock, 32.9, 34.1);
        assert!(sent.lock().unwrap().iter().any(|(t, r)| r[0] == "PAGE B" && *t > 32.9));
        // shown on asking
        core.show(0);
        run(&mut core, &clock, 34.1, 34.2);
        assert_eq!(sent.lock().unwrap().last().unwrap().1[0], "PAGE A");
    }

    #[test]
    fn standing_aside_the_keyboard_has_its_screen_and_the_page_waits() {
        use crate::lcd_page::Transition;
        let (mut core, sent, clock) = two_pages(Transition::Cut);
        // A shows for a second, then the keyboard's own screen for nine
        run(&mut core, &clock, 0.0, 1.0);
        let before = sent.lock().unwrap().len();
        core.stand_aside(true, 1.0);
        run(&mut core, &clock, 1.0, 10.0);
        {
            // the screen blanked, then handed back, and nothing drawn after
            let s = sent.lock().unwrap();
            assert_eq!(s.len(), before + 2, "{:?}", &s[before..]);
            assert!(s[before..].iter().all(|(_, r)| r.iter().all(|row| row.is_empty())));
        }
        // back: A at once, with the two seconds it had left, then B
        core.stand_aside(false, 10.0);
        run(&mut core, &clock, 10.0, 10.05);
        assert_eq!(sent.lock().unwrap().last().unwrap().1[0], "PAGE A");
        run(&mut core, &clock, 10.05, 11.9);
        assert!(first_at(&sent, "PAGE B").is_none());
        run(&mut core, &clock, 11.9, 12.5);
        let b = first_at(&sent, "PAGE B").expect("B after A's three seconds in all");
        assert!((b - 12.0).abs() < 0.05, "B at {b}");
    }

    #[test]
    fn the_playlist_says_where_it_stands() {
        use crate::lcd_page::Transition;
        let (mut core, _sent, clock) = two_pages(Transition::Wipe);
        run(&mut core, &clock, 0.0, 2.0);
        let (turning, elapsed, length) = core.standing(2.0);
        assert!(!turning && (elapsed - 2.0).abs() < 0.03 && length == 3.0, "{turning} {elapsed} {length}");
        // B coming in: the wipe's frames, from when they began
        run(&mut core, &clock, 2.0, 3.2);
        let (turning, elapsed, length) = core.standing(3.2);
        assert!(turning && elapsed > 0.1 && elapsed < 0.25 && (length - 21.0 * 0.025).abs() < 1e-9, "{turning} {elapsed} {length}");
        // B showing, for its five seconds
        run(&mut core, &clock, 3.2, 4.5);
        let (turning, _, length) = core.standing(4.5);
        assert!(!turning && length == 5.0);
    }

    #[test]
    fn names_in_plain_letters() {
        assert_eq!(plain("Pokémon™  Legends:  Z-A"), "Pokemon Legends: Z-A");
        assert_eq!(plain("ＦＵＬＬ ﬁre ©"), "FULL fire");
    }
}
