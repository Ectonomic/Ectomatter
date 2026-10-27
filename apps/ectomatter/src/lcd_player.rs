//! Ectomatter playing the keyboard's screen, as the z10-lcd service did: the Stats
//! page once a second, and the launch and exit sequences breaking in when a
//! game starts or quits, as the Screen tab's Game launch says (Ectomatter sees
//! a game start and quit itself, games.rs; game-helper's note, while it runs,
//! names the steps it takes and how far it has got). The loop is z10lcd.py's main loop, carried over
//! step for step (see [Core::tick]), so the keyboard shows the same frames at
//! the same moments; what Ectomatter adds sits around it: a pause, whether
//! launches break in, standing aside while the Screen tab shows the keyboard's
//! own status screen, and handing the screen back to the keyboard's own screens
//! on quitting.
//!
//! Each frame goes to the keyboard through its core (`displays/show`), which
//! sends it between its other exchanges with the keyboard, so a frame never
//! meets the editor's own on the way.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::lcd_anim::{Ask, Frame, Playing};
use crate::lcd_stats::Stats;

/// seconds between readings of the Stats page
const INTERVAL: f64 = 1.0;

pub fn runtime_dir() -> PathBuf {
    #[cfg(unix)]
    let own = || PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() }));
    // (elsewhere the system's folder for passing files)
    #[cfg(not(unix))]
    let own = std::env::temp_dir;
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(own)
}

// ------------------------------------------------------------------ the keyboard's screen

/// Report 10 as the service built it: `0a ea 02 <cmd> 00 00 00 <checksum>` and
/// eighty Windows-1252 characters, four rows of twenty, each as the keyboard's
/// table has it (the full block 0xDB, which lights the whole cell). `cmd` 1 draws
/// them; 0 hands the screen back to the keyboard's own screens. (Kept as the
/// service's own, which the Z10's core is checked against: the core builds
/// the frames sent now.)
#[cfg(test)]
pub fn frame(rows: &[String], cmd: u8) -> [u8; 88] {
    let mut text = Vec::with_capacity(80);
    for i in 0..4 {
        let r: Vec<char> = rows.get(i).map_or("", |s| s.as_str()).chars().take(20).collect();
        for k in 0..20 {
            text.push(evga_z10::lcd::cp1252(r.get(k).copied().unwrap_or(' ')));
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

/// The keyboard's screen, through the keyboard's core: each frame a call to it,
/// `displays/show` with the rows, and handing the screen back
/// `displays/hand-back`, each to the text display its core describes, by its
/// `control` (none goes before it describes one). The screen is there while
/// the core says the keyboard is (`there`, kept by the keyboard's thread from
/// the core's `device/added` and `device/removed`); a frame that doesn't go is
/// tried again a moment later.
pub struct CoreSink {
    core: Option<crate::core_host::Core>,
    there: Arc<AtomicBool>,
    failed: Option<Instant>,
}

/// After a frame doesn't go, how long before the next is tried.
const RETRY: Duration = Duration::from_secs(2);

impl CoreSink {
    pub fn new(core: Option<crate::core_host::Core>, there: Arc<AtomicBool>) -> CoreSink {
        CoreSink { core, there, failed: None }
    }
}

impl Sink for CoreSink {
    fn find(&mut self) -> bool {
        self.core.is_some() && self.there.load(Ordering::Relaxed) && self.failed.is_none_or(|t| t.elapsed() >= RETRY)
    }

    fn send(&mut self, rows: &[String], cmd: u8) -> Result<(), ()> {
        let Some(core) = &self.core else { return Err(()) };
        let Some(control) = crate::lcd_page::screen()["control"].as_str() else { return Err(()) };
        let r = if cmd == 0 { core.call("displays/hand-back", &serde_json::json!({ "control": control })) } else { core.call("displays/show", &serde_json::json!({ "control": control, "rows": rows })) };
        self.failed = r.is_err().then(Instant::now);
        r.map(|_| ()).map_err(|_| ())
    }
}

// ------------------------------------------------------------------ game-helper's note

/// The note's signature (when it was written, its size, its inode) and its fields.
fn read_event(path: &Path) -> (Option<(i128, u64, u64)>, Option<HashMap<String, String>>) {
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
    // (when it was written, its size, and on Unix its inode)
    let written = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos() as i128);
    #[cfg(unix)]
    let ino = std::os::unix::fs::MetadataExt::ino(&m);
    #[cfg(not(unix))]
    let ino = 0;
    let sig = (written, m.len(), ino);
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

/// What game-helper's note last said, and the sequence playing.
struct Session {
    key: Option<(Option<String>, Option<String>)>,
    ev: Arc<Mutex<HashMap<String, String>>>,
    frames: Option<Playing>,
    /// which sequence is playing: "launch" or "exit"
    kind: Option<&'static str>,
}

/// How long a screen stays as a game starts or quits: until the game's window
/// opens (starting), until everything is put back (quitting), or some seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Until {
    Window,
    Done,
    Secs(u32),
}

/// What the Screen tab's Game launch shows as a game starts, or as one quits:
/// whether anything, which layout (None: the screen as it ships, with its
/// animation), and until when.
#[derive(Clone, Debug, PartialEq)]
pub struct Showing {
    pub on: bool,
    pub layout: Option<crate::lcd_page::Page>,
    pub until: Until,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Launch {
    pub start: Showing,
    pub quit: Showing,
}

impl Default for Launch {
    /// As the note played them before there was a Game launch: both screens as they
    /// ship, the launch screen held no longer than its own animation.
    fn default() -> Launch {
        Launch { start: Showing { on: true, layout: None, until: Until::Secs(0) }, quit: Showing { on: true, layout: None, until: Until::Done } }
    }
}

/// A game as Ectomatter saw it: started, or quit (each with its readings), or
/// its window come to the front.
#[derive(Clone, Debug, PartialEq)]
pub enum GameSeen {
    Started(crate::lcd_page::Game),
    Quit(crate::lcd_page::Game),
    Window,
}

/// A game's name as the note and the Library are matched by: plain letters, any case.
fn folded(name: &str) -> String {
    plain(name).to_lowercase()
}

/// What the note says of a game's steps, if the note is of that game in that phase:
/// its steps, and how many are done (all of them once game-helper has gone).
fn note_steps(ev: &HashMap<String, String>, game: &str, phase: &str) -> Option<(Vec<String>, i64)> {
    if ev.get("phase").map(String::as_str) != Some(phase) || folded(ev.get("name").map_or("", |s| s.as_str())) != folded(game) {
        return None;
    }
    let steps: Vec<String> = ev.get("steps").map_or("", |s| s.as_str()).split('|').filter(|x| !x.is_empty()).map(String::from).collect();
    let done = if alive(ev) { int(ev.get("done"), 0) } else { 1_000_000 };
    Some((steps, done))
}

/// What Ectomatter itself is changing for a game, shared with the thread that
/// makes the changes (putback.rs): the game, its phase, each change's words
/// for the screen, and how far it has got.
pub type Own = Arc<Mutex<Option<crate::putback::Steps>>>;

/// A game's steps in a phase, Ectomatter's own first and then the note's
/// (game-helper's, while it still runs games): each step's words, how many
/// are done, and whether all of them are (a part with nothing to do is done).
fn merged(ev: &HashMap<String, String>, own: &Option<crate::putback::Steps>, game: &str, phase: &str) -> Option<(Vec<String>, i64, bool)> {
    let mine = own.as_ref().filter(|s| s.phase == phase && folded(&s.game) == folded(game));
    let theirs = note_steps(ev, game, phase);
    if mine.is_none() && theirs.is_none() {
        return None;
    }
    let mut steps: Vec<String> = mine.map(|s| s.labels.clone()).unwrap_or_default();
    let (mine_done, mine_finished) = mine.map_or((0, true), |s| (s.done as i64, s.finished));
    let (their_done, their_finished) = match &theirs {
        Some((st, d)) => (*d, *d >= st.len() as i64 || ev.get("finished").is_some_and(|f| f == "1") || !alive(ev)),
        None => (0, true),
    };
    let theirs_n = theirs.as_ref().map_or(0, |(st, _)| st.len()) as i64;
    steps.extend(theirs.map(|(st, _)| st).unwrap_or_default());
    let done = if mine_finished { mine.map_or(0, |s| s.labels.len() as i64) + their_done.min(theirs_n) } else { mine_done };
    Some((steps, done, mine_finished && their_finished))
}

fn alive(ev: &HashMap<String, String>) -> bool {
    crate::computer::here(format!("/proc/{}", int(ev.get("pid"), -1))).exists()
}

impl Session {
    fn new() -> Session {
        Session { key: None, ev: Arc::new(Mutex::new(HashMap::new())), frames: None, kind: None }
    }

    /// The note as it reads now: a new phase of a session is a game starting or
    /// quitting, said back as the phase, the game's name and, quitting, the minutes
    /// played; anything else is progress on the sequence playing.
    fn update(&mut self, ev: HashMap<String, String>) -> Option<(&'static str, String, i64)> {
        *self.ev.lock().unwrap() = ev.clone();
        let key = (ev.get("session").cloned(), ev.get("phase").cloned());
        if self.key.as_ref() == Some(&key) {
            return None;
        }
        self.key = Some(key);
        let name = Some(plain(ev.get("name").map_or("", |s| s.as_str()))).filter(|n| !n.is_empty()).unwrap_or_else(|| "GAME".into());
        match ev.get("phase").map(|s| s.as_str()) {
            Some("launch") => Some(("launch", name, 0)),
            Some("exit") => Some(("exit", name, ((int(ev.get("ended"), 0) - int(ev.get("started"), 0)).div_euclid(60)).max(0))),
            _ => None,
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

/// What Ectomatter asks of the player.
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
    /// a key's press: the layout so many on (or, below 0, back) comes in now, with its way
    /// in (see [Core::step])
    Step(i32),
    /// the playlist's first layout comes in now with its way in, as after another playlist
    /// is chosen by a key (see [Core::begin])
    Begin,
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
    /// what the Screen tab's Game launch shows as a game starts and quits
    GameSetup(Launch),
    /// a game Ectomatter saw start or quit, or its window come to the front
    Game(GameSeen),
}

/// What the player tells Ectomatter: what the screen shows, and why.
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
    /// the keyboard's screen isn't there
    Missing,
}

/// The service's loop, one turn at a time, with what it needs handed in.
pub struct Core {
    pub sink: Box<dyn Sink>,
    pub stats: Ask<Vec<String>>,
    pub event: PathBuf,
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
    /// the playlist: each page's seconds (infinite for a page that stays until
    /// the layouts are switched) and how it comes in, in order; the page
    /// showing, which `stats` reads; staying on it while it is edited
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
    /// how often the page showing changes between readings, in milliseconds (`stats` says
    /// so as it draws, see [crate::lcd_page::pace]): an effect every tenth of a second, words
    /// scrolling a cell's step at a time; 0 while it holds still, drawn once a second
    pub pace: Arc<AtomicU32>,
    /// what Ectomatter itself is changing for a game, as it starts or quits
    pub own: Own,
    /// what the Game launch shows as a game starts and quits
    pub launch: Launch,
    /// the game starting, playing or quitting (or the last one), for the pages' readings;
    /// whether its window has come to the front since it started
    pub game: Arc<Mutex<Option<crate::lcd_page::Game>>>,
    window: Arc<AtomicBool>,
    /// the loop's clock, as its last turn read it (seconds, as f64 bits), for what a
    /// screen shows until
    clock: Arc<std::sync::atomic::AtomicU64>,
    /// the sequences shown lately: their phase, their game, whether the note brought
    /// it on, when; so the note and Ectomatter's own sight of the same game starting (or
    /// quitting) bring on one screen, not two
    shown_for: Vec<(&'static str, String, bool, f64)>,
    /// a layout drawn with the computer's readings and the game's, a moment into it
    /// (none in the tests, where a layout shows as the screen ships it)
    pub draw: Option<Arc<dyn Fn(&crate::lcd_page::Page, f64) -> Vec<String> + Send + Sync>>,
}

impl Core {
    pub fn new(sink: Box<dyn Sink>, stats: Ask<Vec<String>>, event: PathBuf, seeds: Box<dyn FnMut() -> u64 + Send>) -> Core {
        // a note already on disk at start-up is from before this ran
        let (last_sig, _) = read_event(&event);
        Core {
            sink,
            stats,
            event,
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
            pace: Arc::new(AtomicU32::new(0)),
            launch: Launch::default(),
            own: Arc::new(Mutex::new(None)),
            game: Arc::new(Mutex::new(None)),
            window: Arc::new(AtomicBool::new(false)),
            clock: Arc::new(std::sync::atomic::AtomicU64::new(0f64.to_bits())),
            shown_for: Vec::new(),
            draw: None,
        }
    }

    /// A game Ectomatter saw start or quit, or its window come to the front.
    pub fn game_seen(&mut self, seen: GameSeen, now: f64) {
        match seen {
            GameSeen::Started(g) => {
                self.window.store(false, Ordering::Relaxed);
                let name = plain(&g.name);
                *self.game.lock().unwrap() = Some(g);
                self.announce("launch", name, 0, false, now);
            }
            GameSeen::Quit(g) => {
                let minutes = g.session(crate::games::now()) as i64;
                let name = plain(&g.name);
                *self.game.lock().unwrap() = Some(g);
                self.announce("exit", name, minutes, false, now);
            }
            GameSeen::Window => self.window.store(true, Ordering::Relaxed),
        }
    }

    /// A game starting ("launch") or quitting ("exit"), by the note or seen by
    /// Ectomatter: its screen comes in, unless the Game launch shows none, the
    /// screen is left to the keyboard, or the other of the two brought that game's
    /// screen in just now (a second start of the game, told the same way, is a start).
    fn announce(&mut self, phase: &'static str, name: String, minutes: i64, by_note: bool, now: f64) {
        if !self.announce || self.aside {
            return;
        }
        let show = if phase == "launch" { self.launch.start.clone() } else { self.launch.quit.clone() };
        if !show.on {
            return;
        }
        let key = folded(&name);
        self.shown_for.retain(|(_, _, _, t)| now - *t < 60.0);
        if self.shown_for.iter().any(|(p, n, note, _)| *p == phase && *n == key && *note != by_note) {
            return;
        }
        self.shown_for.push((phase, key, by_note, now));
        let start = match &self.shown {
            Some(s) => s.clone(),
            None => (self.stats)(),
        };
        // (the steps Ectomatter makes itself, then the note's)
        let (note, own, game) = (self.session.ev.clone(), self.own.clone(), name.clone());
        let steps: Ask<Vec<String>> = Arc::new(move || merged(&note.lock().unwrap(), &own.lock().unwrap(), &game, phase).map(|s| s.0).unwrap_or_default());
        let (note, own, game) = (self.session.ev.clone(), self.own.clone(), name.clone());
        let done: Ask<i64> = Arc::new(move || merged(&note.lock().unwrap(), &own.lock().unwrap(), &game, phase).map_or(1_000_000, |s| s.1));
        let (note, own, game, window, clock) = (self.session.ev.clone(), self.own.clone(), name.clone(), self.window.clone(), self.clock.clone());
        let until: Ask<bool> = Arc::new(move || match show.until {
            Until::Window => window.load(Ordering::Relaxed),
            Until::Secs(n) => f64::from_bits(clock.load(Ordering::Relaxed)) - now >= n as f64,
            // (everything put back: Ectomatter's own and the note's, or there's nothing to put back)
            Until::Done => merged(&note.lock().unwrap(), &own.lock().unwrap(), &game, "exit").is_none_or(|m| m.2),
        });
        let seed = (self.seeds)();
        let layout = show.layout.clone().filter(|p| p.as_shipped_game().is_none());
        // (on the screen the keyboard's core describes)
        let size = crate::lcd_page::size();
        self.session.frames = Some(match (layout, &self.draw) {
            (Some(page), Some(draw)) => {
                let draw = draw.clone();
                let from = Instant::now();
                Playing::layout(seed, size, Arc::new(move || draw(&page, from.elapsed().as_secs_f64())), until, phase == "exit", self.stats.clone(), start)
            }
            _ if phase == "launch" => Playing::launch(seed, size, name, steps, done, until, self.stats.clone(), start),
            _ => Playing::exit(seed, size, name, steps, minutes, done, until, self.stats.clone(), start),
        });
        self.session.kind = Some(phase);
        self.frame_until = 0.0;
        // (a game's screen breaks in on a page coming in, too)
        self.between.clear();
    }

    /// When the page showing is drawn next: once a second, or while it moves (an effect,
    /// words scrolling), as its next step comes.
    fn next_draw(&self, now: f64) -> f64 {
        now + match self.pace.load(Ordering::Relaxed) {
            0 => INTERVAL,
            ms => ms as f64 / 1000.0,
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
        self.clock.store(now.to_bits(), Ordering::Relaxed);
        // (standing aside, the keyboard's own screen shows: nothing is drawn)
        if self.aside {
            return true;
        }
        let (sig, ev) = read_event(&self.event);
        if sig != self.last_sig {
            self.last_sig = sig;
            // (a note with nothing in it is no note; one that changes has the next frame
            // come at once, as the service had it, whatever it changed)
            if let Some(ev) = ev.filter(|e| !e.is_empty()) {
                let news = self.session.update(ev);
                if self.announce {
                    if let Some((phase, name, minutes)) = news {
                        self.announce(phase, name, minutes, true, now);
                    }
                    self.frame_until = 0.0;
                    self.between.clear();
                }
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
        } else if (now >= self.stats_at && (!self.paused || self.page_since.is_none())) || self.redraw {
            // (paused, the page is drawn once as it comes on, then held)
            self.redraw = false;
            let rows = (self.stats)();
            self.push(rows, now);
            self.stats_at = self.next_draw(now);
            if self.page_since.is_none() {
                self.page_since = Some(now);
                // (a page that comes on while the playlist is paused has its whole time once it plays)
                if self.paused && self.held.is_none() {
                    self.held = Some(0.0);
                }
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
        self.come_in((self.page() + 1) % n, now);
    }

    /// Page `to` comes in now, as its turn would bring it: the frames of its way in from the
    /// page shown, then the page itself, with its whole time (paused, from when the
    /// playlist plays again).
    fn come_in(&mut self, to: usize, now: f64) {
        let old = self.shown.clone().unwrap_or_default();
        self.current.store(to, Ordering::Relaxed);
        let new = (self.stats)();
        let seed = (self.seeds)();
        let frames = crate::lcd_anim::page_turn(&old, &new, self.playlist[to].1, seed, crate::lcd_page::size());
        self.page_since = None;
        if self.paused {
            self.held = Some(0.0);
        }
        if frames.is_empty() {
            self.push(new, now);
            self.stats_at = self.next_draw(now);
            self.page_since = Some(now);
        } else {
            self.turn_from = now;
            self.turn_len = frames.iter().map(|f| f.ms as f64 / 1000.0).sum();
            self.between = frames.into();
            self.frame_until = now;
        }
    }

    /// A key's press: the page `d` on (or, below 0, back), going round at the ends, comes in
    /// now with its own way in, as if its turn had come early, and has its whole time; a
    /// paused playlist stays paused on it. Nothing while a sequence plays, the page is held
    /// for the editor, the screen is left to the keyboard, or the playlist has one page.
    pub fn step(&mut self, d: i32, now: f64) {
        let n = self.playlist.len();
        if n < 2 || self.hold || self.aside || self.playing_sequence() {
            return;
        }
        self.come_in((self.page() as i64 + d as i64).rem_euclid(n as i64) as usize, now);
    }

    /// The playlist's first page comes in now with its own way in, as after a key chooses
    /// another playlist (the same holds as for [Core::step], but one page is enough).
    pub fn begin(&mut self, now: f64) {
        if self.playlist.is_empty() || self.hold || self.aside || self.playing_sequence() {
            return;
        }
        self.come_in(0, now);
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

    /// The pages to play, and how long each shows and comes in. The page showing, timed
    /// again after it stayed, has its whole time from `now` (and doesn't turn the moment
    /// it is timed, however long it stayed).
    pub fn set_playlist(&mut self, list: Vec<(f64, crate::lcd_page::Transition)>, now: f64) {
        if self.page() >= list.len() {
            self.current.store(0, Ordering::Relaxed);
        }
        let i = self.page();
        if self.playlist.get(i).is_some_and(|p| p.0.is_infinite()) && list.get(i).is_some_and(|p| p.0.is_finite()) {
            if self.page_since.is_some() {
                self.page_since = Some(now);
            }
            if self.held.is_some() {
                self.held = Some(0.0);
            }
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
        let blank = vec![String::new(); crate::lcd_page::size().1];
        let mut done = false;
        if self.sink.send(&blank, 1).is_ok() {
            done = !own || self.sink.send(&blank, 0).is_ok();
        }
        done
    }
}

/// The player, on a thread of its own.
pub struct Player {
    tell: mpsc::Sender<Control>,
}

impl Player {
    /// Start playing; each frame sent, and each change of state, goes to `news`.
    pub fn start(news: impl Fn(News) + Send + 'static, announce: bool, launch: Launch, paused: bool, screen: Box<dyn Sink>, own: Own, readings: crate::hosted::Readings) -> Player {
        let (tell, heard) = mpsc::channel::<Control>();
        std::thread::spawn(move || {
            // (the computer's readings as the Sensor Feed tells them)
            let stats = Arc::new(Mutex::new(Stats::of(readings, "screen")));
            // primed, as the service primed it: the first reading of the load is 0
            stats.lock().unwrap().prime();
            // the pages, the one showing, and what they may show besides the computer's readings
            let shared = Arc::new(Mutex::new((vec![crate::lcd_page::Page::stats()], None::<(u8, String)>, None::<(f64, String)>)));
            let current = Arc::new(AtomicUsize::new(0));
            let pace = Arc::new(AtomicU32::new(0));
            let (reader, page, at, moves) = (stats.clone(), shared.clone(), current.clone(), pace.clone());
            // the computer read once a second, however often the page is drawn; and the page
            // showing, and when it came on, which its scrolling words are timed from
            let kept: Arc<Mutex<(Option<(Instant, crate::lcd_page::Readings)>, Option<(usize, String, Instant)>)>> = Arc::new(Mutex::new((None, None)));
            // the game starting, playing or quitting, and the note's steps for it
            let game: Arc<Mutex<Option<crate::lcd_page::Game>>> = Arc::new(Mutex::new(None));
            let note: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
            // everything a page may show, now: the computer read once a second, the rest as it is
            let read: Arc<dyn Fn() -> crate::lcd_page::Readings + Send + Sync> = {
                let (kept, page, game, note, own) = (kept.clone(), shared.clone(), game.clone(), note.clone(), own.clone());
                Arc::new(move || {
                    let mut k = kept.lock().unwrap();
                    if k.0.as_ref().is_none_or(|(t, _)| t.elapsed() >= Duration::from_millis(990)) {
                        // (only the plugins' readings the pages playing show are asked for)
                        let shows = crate::lcd_page::readings_shown(&page.lock().unwrap().0);
                        let mut stats = reader.lock().unwrap();
                        stats.showing(shows);
                        k.0 = Some((Instant::now(), stats.read()));
                    }
                    let mut r = k.0.as_ref().unwrap().1.clone();
                    let p = page.lock().unwrap();
                    r.now = Some(crate::lcd_page::Moment::now());
                    r.profile = p.1.as_ref().map(|(_, name)| name.clone());
                    r.profile_number = p.1.as_ref().map(|(n, _)| *n);
                    r.outside = p.2.clone();
                    r.game = game.lock().unwrap().clone().map(|mut g| {
                        // (what Ectomatter, then game-helper, is setting or putting back for this game)
                        let ev = note.lock().unwrap();
                        let o = own.lock().unwrap();
                        let phase = if g.ended > 0 { "exit" } else { "launch" };
                        if let Some((steps, done, _)) = merged(&ev, &o, &g.name, phase) {
                            let d = (done.max(0) as usize).min(steps.len());
                            g.changing = steps.get(d).cloned().unwrap_or_default();
                            g.done = (d, steps.len());
                        }
                        g
                    });
                    r
                })
            };
            let (reading, kept_page) = (read.clone(), kept.clone());
            let ask: Ask<Vec<String>> = Arc::new(move || {
                let r = reading();
                let mut k = kept_page.lock().unwrap();
                let p = page.lock().unwrap();
                let i = at.load(Ordering::Relaxed);
                let Some(page) = p.0.get(i).or(p.0.first()) else { return Vec::new() };
                if k.1.as_ref().is_none_or(|(j, id, _)| *j != i || *id != page.id) {
                    k.1 = Some((i, page.id.clone(), Instant::now()));
                }
                let t = k.1.as_ref().map_or(0.0, |(_, _, since)| since.elapsed().as_secs_f64());
                moves.store(crate::lcd_page::pace(page, &r).map_or(0, |s| (s * 1000.0).round() as u32), Ordering::Relaxed);
                crate::lcd_page::render_at(page, &r, t)
            });
            let seeds = Box::new(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64));
            let mut core = Core::new(screen, ask, runtime_dir().join("z10-lcd/session"), seeds);
            core.session.ev = note;
            core.game = game;
            core.launch = launch;
            core.own = own;
            let drawing = read.clone();
            core.draw = Some(Arc::new(move |p: &crate::lcd_page::Page, t: f64| crate::lcd_page::render_at(p, &drawing(), t)));
            core.trace = std::env::var_os("ECTOMATTER_LCD_TRACE").map(PathBuf::from);
            core.announce = announce;
            core.paused = paused;
            core.current = current;
            core.pace = pace;
            let start = Instant::now();
            let mut told: Option<(State, Option<&'static str>, usize, bool, u64)> = None;
            loop {
                // what Ectomatter asks
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
                            core.set_playlist(pages.iter().map(|p| (if p.stays { f64::INFINITY } else { p.secs as f64 }, p.comes_in)).collect(), start.elapsed().as_secs_f64());
                            shared.lock().unwrap().0 = pages;
                        }
                        Control::Show(i) => core.show(i),
                        Control::Step(d) => core.step(d, start.elapsed().as_secs_f64()),
                        Control::Begin => core.begin(start.elapsed().as_secs_f64()),
                        Control::Aside(a) => {
                            if a != core.aside {
                                eprintln!("ectomatter: the screen {}", if a { "left to the keyboard's own status screen" } else { "taken back for the playlist" });
                            }
                            core.stand_aside(a, start.elapsed().as_secs_f64());
                        }
                        Control::Hold(i) => core.hold_on(i),
                        Control::GameSetup(l) => core.launch = l,
                        Control::Game(seen) => core.game_seen(seen, start.elapsed().as_secs_f64()),
                        Control::Context(profile, outside) => {
                            let mut s = shared.lock().unwrap();
                            if s.1 != profile || s.2 != outside {
                                (s.1, s.2) = (profile, outside);
                                core.redraw = true;
                            }
                        }
                        Control::Quit(own, done) => {
                            let done_back = core.hand_back(own);
                            eprintln!("ectomatter: the screen {}", if !done_back { "could not be handed back" } else if own { "handed back to the keyboard's own screens" } else { "left blank" });
                            let _ = done.send(());
                            return;
                        }
                    }
                }
                let again = core.tick(start.elapsed().as_secs_f64());
                let found = core.sink.find();
                let state = if !found { State::Missing } else if core.paused && !core.playing_sequence() { State::Paused } else { State::Playing };
                let (sequence, page) = (core.sequence(), core.page());
                let (turning, elapsed, length) = core.standing(start.elapsed().as_secs_f64());
                // (said again as the page comes on after coming in, so the time it shows from is right)
                // (and again when the page's time is changed while it shows, or it is made to stay
                // or timed again, so the playhead keeps to it)
                let now_told = (state, sequence, page, turning, length.to_bits());
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

    /// Ectomatter's own steps come first on the screen, game-helper's after;
    /// done counts Ectomatter's until they're all made, then game-helper's too;
    /// and everything is put back only once both are.
    #[test]
    fn ectomatters_own_steps_come_first() {
        let mut ev = HashMap::new();
        for (k, v) in [("phase", "exit"), ("name", "ARC Raiders"), ("steps", "KEYBOARD|MOUSE"), ("done", "1"), ("pid", &std::process::id().to_string())] {
            ev.insert(k.to_string(), v.to_string());
        }
        let mut own = Some(crate::putback::Steps { game: "ARC Raiders".into(), phase: "exit", labels: vec!["BRIGHTNESS".into(), "PAUSE THE LOCAL AI".into()], done: 1, finished: false });
        let m = merged(&ev, &own, "ARC Raiders", "exit").unwrap();
        assert_eq!(m.0, vec!["BRIGHTNESS", "PAUSE THE LOCAL AI", "KEYBOARD", "MOUSE"]);
        assert_eq!((m.1, m.2), (1, false));
        own.as_mut().unwrap().done = 2;
        own.as_mut().unwrap().finished = true;
        let m = merged(&ev, &own, "ARC Raiders", "exit").unwrap();
        assert_eq!((m.1, m.2), (3, false), "game-helper still has one to put back");
        ev.insert("finished".into(), "1".into());
        assert!(merged(&ev, &own, "ARC Raiders", "exit").unwrap().2);
        // another game's steps are none of this one's
        assert!(merged(&HashMap::new(), &own, "Spelunky 2", "exit").is_none());
    }

    /// A screen that keeps what it's sent.
    struct Sent(Arc<Mutex<Vec<Vec<String>>>>);
    impl Sink for Sent {
        fn find(&mut self) -> bool {
            true
        }
        fn send(&mut self, rows: &[String], _cmd: u8) -> Result<(), ()> {
            self.0.lock().unwrap().push(rows.to_vec());
            Ok(())
        }
    }

    /// A player with no note, the Stats page as `stats`, run a while: what it sent.
    fn player(note: &Path) -> (Core, Arc<Mutex<Vec<Vec<String>>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let stats: Ask<Vec<String>> = Arc::new(|| vec!["CPU 40° 5%".into(), String::new(), String::new(), String::new()]);
        let core = Core::new(Box::new(Sent(sent.clone())), stats, note.to_path_buf(), Box::new(|| 7));
        (core, sent)
    }

    fn run_for(core: &mut Core, from: f64, secs: f64) -> f64 {
        let mut now = from;
        while now < from + secs {
            if core.tick(now) {
                now += 0.01;
            }
        }
        now
    }

    fn arc() -> crate::lcd_page::Game {
        crate::lcd_page::Game { name: "ARC Raiders".into(), since: crate::games::now(), ..Default::default() }
    }

    /// A game Ectomatter sees start brings in the launch screen with no note at all; held
    /// until the game's window opens, the screen waits at READY, then goes back to the page.
    #[test]
    fn a_game_seen_starting_brings_in_its_screen_held_till_its_window_opens() {
        let dir = std::env::temp_dir().join(format!("ectomatter-seen-{}", std::process::id()));
        let (mut core, sent) = player(&dir.join("session"));
        core.launch.start.until = Until::Window;
        let now = run_for(&mut core, 0.0, 1.0);
        core.game_seen(GameSeen::Started(arc()), now);
        let now = run_for(&mut core, now, 8.0);
        let rows = sent.lock().unwrap().clone();
        assert!(rows.iter().any(|r| r[0].starts_with("> LAUNCHING")) && rows.iter().any(|r| r[1].trim() == "ARC RAIDERS"), "the name comes in");
        assert!(rows.last().unwrap()[0].starts_with("> READY"), "held at READY while the window hasn't opened: {:?}", rows.last());
        assert!(core.playing_sequence());
        core.game_seen(GameSeen::Window, now);
        run_for(&mut core, now, 3.0);
        assert!(!core.playing_sequence(), "gone back to the playlist once the window opened");
        assert_eq!(sent.lock().unwrap().last().unwrap()[0], "CPU 40° 5%");
    }

    /// The note and Ectomatter's own sight of the same game bring in one screen, not two;
    /// a Game launch showing nothing brings in none.
    #[test]
    fn one_game_starting_brings_in_one_screen_or_none() {
        let dir = std::env::temp_dir().join(format!("ectomatter-once-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("session");
        let (mut core, _) = player(&note);
        let now = run_for(&mut core, 0.0, 0.5);
        core.game_seen(GameSeen::Started(arc()), now);
        let now = run_for(&mut core, now, 0.5);
        assert!(core.playing_sequence());
        core.drop_sequence();
        // game-helper's note of the same game, just after: no second screen
        std::fs::write(&note, format!("session=1-1\npid={}\nphase=launch\nname=ARC Raiders\nsteps=MONITORS\ndone=0\n", std::process::id())).unwrap();
        run_for(&mut core, now, 0.5);
        assert!(!core.playing_sequence(), "the note's launch is the same game's, already shown");
        // the game started again, as Ectomatter sees it: that's a start, and it's shown
        core.game_seen(GameSeen::Started(arc()), now + 1.0);
        assert!(core.playing_sequence(), "a second start is shown");
        let (mut off, _) = player(&dir.join("none"));
        off.launch.start.on = false;
        off.game_seen(GameSeen::Started(arc()), 0.0);
        assert!(!off.playing_sequence(), "Game launch shows nothing as a game starts");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A layout of one's own shows in place of the launch screen, drawn with the game's readings.
    #[test]
    fn a_layout_shows_in_place_of_the_launch_screen() {
        let dir = std::env::temp_dir().join(format!("ectomatter-layout-{}", std::process::id()));
        let (mut core, sent) = player(&dir.join("session"));
        let game = core.game.clone();
        core.draw = Some(Arc::new(move |p: &crate::lcd_page::Page, _t| {
            let r = crate::lcd_page::Readings { game: game.lock().unwrap().clone(), ..Default::default() };
            crate::lcd_page::render(p, &r)
        }));
        let mut page = crate::lcd_page::Page::built_in("game-launch").unwrap();
        page.pieces.retain(|p| p.row == 1);
        core.launch.start = Showing { on: true, layout: Some(page), until: Until::Secs(1) };
        core.game_seen(GameSeen::Started(arc()), 0.0);
        run_for(&mut core, 0.0, 4.0);
        let rows = sent.lock().unwrap().clone();
        assert!(rows.iter().any(|r| r[1].trim() == "ARC RAIDERS" && r[0].trim().is_empty()), "the layout, its game's name read");
        assert!(!rows.iter().any(|r| r[0].starts_with("> LAUNCHING")), "and not the launch screen");
        assert!(!core.playing_sequence());
    }

    #[test]
    fn frames_as_the_service_built_them() {
        let f = frame(&["CPU 64° 5%".into(), "AB".into()], 1);
        assert_eq!(&f[0..4], &[0x0a, 0xea, 0x02, 0x01]);
        assert_eq!(f[8 + 6], 0xB0);
        assert_eq!(f[8..88].iter().fold(0u8, |a, b| a.wrapping_add(*b)).wrapping_add(f[7]), 0);
        assert_eq!(frame(&[], 0)[3], 0);
        // the full block Fire and Spectrum draw with: the code the keyboard lights a whole cell for
        assert_eq!(frame(&["█▀".into()], 1)[8..10], [0xDB, b'?']);
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
        let note = std::env::temp_dir().join(format!("ectomatter-turn-{}-{:?}", std::process::id(), how));
        let _ = std::fs::remove_file(&note);
        let mut core = Core::new(Box::new(Kept(sent.clone(), clock.clone())), ask, note, Box::new(|| 7));
        core.current = current;
        core.set_playlist(vec![(3.0, crate::lcd_page::Transition::Wipe), (5.0, how)], 0.0);
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
    fn words_scrolling_are_drawn_a_quarter_second_at_a_time() {
        // a page whose rows change every tenth of a second (as scrolling words do, at whatever
        // moment it is drawn): drawn once a second while still, four times a second while moving
        let sent = Arc::new(Mutex::new(Vec::new()));
        let clock = Arc::new(Mutex::new(0.0));
        let c = clock.clone();
        let ask: Ask<Vec<String>> = Arc::new(move || vec![format!("{:.1}", *c.lock().unwrap())]);
        let note = std::env::temp_dir().join(format!("ectomatter-moving-{}", std::process::id()));
        let _ = std::fs::remove_file(&note);
        let mut core = Core::new(Box::new(Kept(sent.clone(), clock.clone())), ask, note, Box::new(|| 7));
        core.set_playlist(vec![(60.0, crate::lcd_page::Transition::Cut)], 0.0);
        run(&mut core, &clock, 0.0, 3.0);
        let still = sent.lock().unwrap().len();
        assert!((3..=4).contains(&still), "once a second while still: {still}");
        core.pace.store(250, Ordering::Relaxed);
        run(&mut core, &clock, 3.0, 5.0);
        let moving = sent.lock().unwrap().len() - still;
        assert!((7..=9).contains(&moving), "every quarter second while moving: {moving}");
        // an effect: every tenth of a second
        core.pace.store(100, Ordering::Relaxed);
        let before = sent.lock().unwrap().len();
        run(&mut core, &clock, 5.0, 6.0);
        let effect = sent.lock().unwrap().len() - before;
        assert!((9..=11).contains(&effect), "every tenth of a second with an effect: {effect}");
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

        // Wipe: 20 frames of 25 ms between (A as it was, then B a column more each), then B,
        // its five seconds from then
        let (mut core, _sent, clock) = two_pages(Transition::Wipe);
        // (when each wipe began and B's page came on, as the player says where it stands)
        let (mut t, mut began, mut on, mut again) = (0.0, None, None, None);
        while t < 9.0 {
            *clock.lock().unwrap() = t;
            core.tick(t);
            match (core.standing(t).0, began, on) {
                (true, None, _) => began = Some(t),
                (false, Some(_), None) => on = Some(t),
                (true, Some(_), Some(_)) if again.is_none() => again = Some(t),
                _ => {}
            }
            t += 0.01;
        }
        let (began, on, again) = (began.expect("B's wipe"), on.expect("B on"), again.expect("A's wipe"));
        assert!((began - 3.0).abs() < 0.05, "B's wipe at {began}");
        // (each 25 ms frame waits for the loop's next 10 ms turn, as the service's did: about 30 ms)
        assert!(on - began > 19.0 * 0.025 && on - began < 20.0 * 0.031, "the wipe took {}", on - began);
        assert!((again - (on + 5.0)).abs() < 0.08, "A comes in again at {again}, B on at {on}");
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
    fn a_key_steps_the_playlist_with_each_layout_s_own_way_in() {
        use crate::lcd_page::Transition;
        // A for 3 s, B for 5 s coming in with a wipe
        let (mut core, sent, clock) = two_pages(Transition::Wipe);
        run(&mut core, &clock, 0.0, 1.0);
        // a step on, a second into A: B's wipe at once, then B with its whole five seconds
        core.step(1, 1.0);
        assert!(core.standing(1.0).0, "B's wipe at once");
        run(&mut core, &clock, 1.0, 2.0);
        let b = first_at(&sent, "PAGE B").expect("B");
        assert!(b < 1.7, "B at {b}");
        run(&mut core, &clock, 2.0, b + 4.9);
        assert!(!sent.lock().unwrap().iter().any(|(t, r)| *t > b && r[0] == "PAGE A"), "A came back before B's five seconds");
        // a step on from the last goes round to the first, and back from the first to the last
        core.step(1, b + 4.9);
        run(&mut core, &clock, b + 4.9, b + 5.0);
        assert_eq!(core.page(), 0);
        core.step(-1, b + 5.0);
        assert_eq!(core.page(), 1);
        // paused, a step shows the layout and stays paused on it, its time still to come
        run(&mut core, &clock, b + 5.0, b + 6.0);
        core.pause(true, b + 6.0);
        core.step(1, b + 6.0);
        run(&mut core, &clock, b + 6.0, b + 20.0);
        assert_eq!(sent.lock().unwrap().last().unwrap().1[0], "PAGE A", "A drawn while paused");
        assert_eq!(core.page(), 0);
        assert!(core.standing(b + 20.0).1 < 0.01, "A's time waits for Play");
        core.pause(false, b + 20.0);
        run(&mut core, &clock, b + 20.0, b + 22.9);
        assert_eq!(core.page(), 0, "A has its whole three seconds once it plays");
        // held for the editor, or with one page, a step does nothing
        core.hold_on(Some(0));
        core.step(1, b + 23.0);
        assert_eq!(core.page(), 0);
        core.hold_on(None);
        core.set_playlist(vec![(3.0, Transition::Cut)], b + 23.0);
        core.step(1, b + 23.0);
        assert_eq!(core.page(), 0);
    }

    #[test]
    fn a_page_that_stays_shows_until_the_layouts_are_switched() {
        use crate::lcd_page::Transition;
        let (mut core, sent, clock) = two_pages(Transition::Cut);
        // A stays: a minute on, B never comes in, and the playlist says A's time has no end
        core.set_playlist(vec![(f64::INFINITY, Transition::Cut), (5.0, Transition::Cut)], 0.0);
        run(&mut core, &clock, 0.0, 60.0);
        assert!(first_at(&sent, "PAGE B").is_none(), "B came in");
        let (turning, elapsed, length) = core.standing(60.0);
        assert!(!turning && length.is_infinite() && elapsed > 59.0, "{turning} {elapsed} {length}");
        // switched to B, B has its five seconds, and A stays again when it comes back
        core.show(1);
        run(&mut core, &clock, 60.0, 80.0);
        let b = first_at(&sent, "PAGE B").expect("B on switching");
        assert!(b < 60.05, "B at {b}");
        let back = sent.lock().unwrap().iter().find(|(t, r)| *t > b && r[0] == "PAGE A").map(|(t, _)| *t).expect("A again");
        assert!((back - (b + 5.0)).abs() < 0.05, "A again at {back}");
        // A timed again after staying a while: its three seconds from then, not at once
        core.set_playlist(vec![(3.0, Transition::Cut), (5.0, Transition::Cut)], 80.0);
        run(&mut core, &clock, 80.0, 82.9);
        assert!(!sent.lock().unwrap().iter().any(|(t, r)| *t > 80.0 && r[0] == "PAGE B"), "B came in at once");
        run(&mut core, &clock, 82.9, 83.5);
        assert!(sent.lock().unwrap().iter().any(|(t, r)| *t > 82.9 && r[0] == "PAGE B"), "B after A's three seconds");
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
        assert!(turning && elapsed > 0.1 && elapsed < 0.25 && (length - 20.0 * 0.025).abs() < 1e-9, "{turning} {elapsed} {length}");
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
