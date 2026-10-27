//! Putting back what Ectomatter changes for a game. As a game starts, the
//! plugins say what should change (their `game/changes`); Ectomatter writes
//! each change down with what it was, before making it, and makes it; as the
//! game quits, it undoes them, the last first. The record is a file
//! (put-back.json in Ectomatter's data folder), so a change made before
//! Ectomatter stopped short is put back the next time it starts, unless the
//! game is still running then, when it's put back as the game quits.
//!
//! The changes are made on a thread of their own, in order, so a slow one (a
//! command waited for, a monitor changing its mode) holds up nothing else of
//! Ectomatter's; how far it has got is shared, for the keyboard's screen.
//!
//! Two games running at once may change the same thing: the one started
//! later holds it, and the first to quit hands what it was to the other
//! rather than putting it back under it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{mpsc, Arc, Mutex};

use serde_json::{json, Value};

use crate::commands::{self, Ran, Script};

/// A game as its changes are made for it: its Library id, its name, and
/// the process the watch saw it by, with when that started.
#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub pid: u32,
    pub since: u64,
}

/// One change, as the record keeps it: which plugin asked, what it changes
/// (`{monitor, set, to}`, or `{command}`), the words for the keyboard's
/// screen and for saying it was put back, what it was before (for a
/// command, its undo), and whether it's been made.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub plugin: String,
    pub what: Value,
    pub label: String,
    pub words: String,
    pub before: Value,
    pub made: bool,
}

impl Change {
    /// What it changes, as two changes of the same thing share: a monitor's
    /// setting, or which monitor is the main one. A command shares with none.
    fn target(&self) -> Option<String> {
        let set = self.what["set"].as_str()?;
        if set == "main" {
            return Some("main".into());
        }
        Some(format!("{}/{set}", self.what["monitor"].as_str()?))
    }

    fn to_value(&self) -> Value {
        json!({ "plugin": self.plugin, "what": self.what, "label": self.label, "words": self.words, "before": self.before, "made": self.made })
    }

    fn of(v: &Value) -> Option<Change> {
        Some(Change {
            plugin: v["plugin"].as_str()?.to_string(),
            what: v["what"].clone(),
            label: v["label"].as_str().unwrap_or("").to_string(),
            words: v["words"].as_str().unwrap_or("").to_string(),
            before: v["before"].clone(),
            made: v["made"].as_bool().unwrap_or(false),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    pub game: Game,
    pub changes: Vec<Change>,
}

/// The record of what's changed for each game running.
pub struct Journal {
    path: Option<PathBuf>,
    pub sessions: Vec<Session>,
}

impl Journal {
    /// The record as it was left, from where it's kept (none, for a check
    /// or a test, which keeps nothing).
    pub fn load(path: Option<PathBuf>) -> Journal {
        let v: Value = path.as_ref().and_then(|p| crate::files::read_json(p)).unwrap_or(Value::Null);
        let sessions = v["sessions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| {
                let g = &s["game"];
                Some(Session {
                    game: Game { id: g["id"].as_str()?.into(), name: g["name"].as_str().unwrap_or("").into(), pid: g["pid"].as_u64().unwrap_or(0) as u32, since: g["since"].as_u64().unwrap_or(0) },
                    changes: s["changes"].as_array().into_iter().flatten().filter_map(Change::of).collect(),
                })
            })
            .collect();
        Journal { path, sessions }
    }

    fn save(&self) {
        let Some(p) = &self.path else { return };
        if self.sessions.is_empty() {
            let _ = std::fs::remove_file(p);
            return;
        }
        let v = json!({ "sessions": self.sessions.iter().map(|s| json!({
            "game": { "id": s.game.id, "name": s.game.name, "pid": s.game.pid, "since": s.game.since },
            "changes": s.changes.iter().map(Change::to_value).collect::<Vec<_>>(),
        })).collect::<Vec<_>>() });
        if let Err(e) = crate::files::write_json(p, &v) {
            eprintln!("ectomatter: what's to be put back wasn’t saved: {e}");
        }
    }
}

/// Where the record is kept: in Ectomatter's data folder, never for a check.
pub fn kept_at() -> Option<PathBuf> {
    let offscreen = cfg!(test) || std::env::args().any(|a| a == "--snapshot");
    (!offscreen).then(|| crate::data_dir().join("put-back.json"))
}

/// The monitors, as the changes reach them: a setting of one read as it is
/// now and set (brightness 0 to 10000, mode, hdr, vrr, enabled; "main", read
/// as the main monitor's id and set by making that one the main).
pub trait Screens: Send {
    fn now(&mut self, monitor: &str, set: &str) -> Result<Value, String>;
    fn set(&mut self, monitor: &str, set: &str, to: &Value) -> Result<(), String>;
}

/// How far the changes for a game have got, for the keyboard's screen: the
/// game, starting ("launch") or quitting ("exit"), each change's words for
/// the screen, how many are made, and whether all are.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Steps {
    pub game: String,
    pub phase: &'static str,
    pub labels: Vec<String>,
    pub done: usize,
    pub finished: bool,
}

/// What the thread has to say: a command run (by its id, for the game or
/// for Run now), what was put back for a game (after Ectomatter stopped short,
/// or not), and a change that couldn't be made.
#[derive(Clone, Debug, PartialEq)]
pub enum Told {
    Ran { script: u64, game: String, quit: bool, ran: Ran, at: u64 },
    PutBack { name: String, words: Vec<String>, after_stop: bool },
    Failed { words: String, why: String },
    /// a game's changes made (or tried), as it starts, by the plan's number:
    /// a game started through Ectomatter waits for this
    Started { id: String, seq: u64 },
}

/// A game's changes to make: the game, and each change with, for a
/// command, the command itself (the plugin named it by its id alone); and
/// the plan's own number, which its Started says.
pub struct Plan {
    pub game: Game,
    pub changes: Vec<(String, Value, String, String, Option<Script>)>,
    pub seq: u64,
}

enum Job {
    Start(Plan),
    Quit(String),
    /// at Ectomatter's start, once the games running are known
    Settle(Vec<crate::games::Running>),
    /// Run now: a command's start, then its undo
    Try(Script),
    /// a plugin turned off: its changes put back now
    Drop(String),
}

/// The thread that makes and undoes the changes.
pub struct Worker {
    tx: mpsc::Sender<Job>,
    pub steps: Arc<Mutex<Option<Steps>>>,
    pub told: mpsc::Receiver<Told>,
}

impl Worker {
    pub fn start(journal: Journal, screens: Option<Box<dyn Screens>>) -> Worker {
        let (tx, jobs) = mpsc::channel::<Job>();
        let (say, told) = mpsc::channel();
        let steps: Arc<Mutex<Option<Steps>>> = Arc::new(Mutex::new(None));
        let shared = steps.clone();
        // (a check run offscreen makes no change and runs no command: its jobs are let go)
        let offscreen = !cfg!(test) && std::env::args().any(|a| a == "--snapshot");
        let _ = std::thread::Builder::new().name("put back".into()).spawn(move || {
            let left: Vec<(String, u32)> = journal.sessions.iter().map(|s| (s.game.id.clone(), s.game.pid)).collect();
            let mut h = Hands { journal, screens, kept: HashMap::new(), steps: shared, say, left };
            for job in jobs {
                if offscreen {
                    continue;
                }
                match job {
                    Job::Start(p) => h.start(p),
                    Job::Quit(id) => h.quit(&id, false),
                    Job::Settle(running) => h.settle(&running),
                    Job::Try(s) => h.try_now(&s),
                    Job::Drop(plugin) => h.drop_plugin(&plugin),
                }
            }
        });
        Worker { tx, steps, told }
    }

    pub fn start_game(&self, p: Plan) {
        let _ = self.tx.send(Job::Start(p));
    }

    pub fn quit_game(&self, id: &str) {
        let _ = self.tx.send(Job::Quit(id.to_string()));
    }

    pub fn settle(&self, running: Vec<crate::games::Running>) {
        let _ = self.tx.send(Job::Settle(running));
    }

    pub fn try_now(&self, s: Script) {
        let _ = self.tx.send(Job::Try(s));
    }

    pub fn drop_plugin(&self, plugin: &str) {
        let _ = self.tx.send(Job::Drop(plugin.to_string()));
    }
}

/// What the thread holds: the record, the monitors, the commands kept running
/// (by game and change), how far it's got, and where it tells.
struct Hands {
    journal: Journal,
    screens: Option<Box<dyn Screens>>,
    kept: HashMap<(String, usize), Child>,
    steps: Arc<Mutex<Option<Steps>>>,
    say: mpsc::Sender<Told>,
    /// the games the record held as this Ectomatter started, each by its id and process
    left: Vec<(String, u32)>,
}

/// A command's undo, as the record keeps it with its process while it's kept running.
fn undo_of(s: &Script) -> Value {
    json!({ "script": s.id, "quit": s.quit, "keep": s.keep, "limit": s.limit, "name": s.name })
}

impl Hands {
    fn show(&self, s: Option<Steps>) {
        *self.steps.lock().unwrap_or_else(|p| p.into_inner()) = s;
    }

    fn step(&self, done: usize, finished: bool) {
        if let Some(s) = self.steps.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
            s.done = done;
            s.finished = finished;
        }
    }

    fn env(game: &Game) -> Vec<(&'static str, String)> {
        vec![("ECTOMATTER_GAME", game.name.clone()), ("ECTOMATTER_GAME_ID", game.id.clone())]
    }

    fn start(&mut self, plan: Plan) {
        // (the same game twice is one session: a second start puts the first back first)
        if self.journal.sessions.iter().any(|s| s.game.id == plan.game.id) {
            self.quit(&plan.game.id, false);
        }
        let game = plan.game.clone();
        let plan_seq = plan.seq;
        self.show(Some(Steps { game: game.name.clone(), phase: "launch", labels: plan.changes.iter().map(|c| c.2.clone()).collect(), done: 0, finished: plan.changes.is_empty() }));
        self.journal.sessions.push(Session { game: game.clone(), changes: Vec::new() });
        let at = self.journal.sessions.len() - 1;
        for (k, (plugin, what, label, words, script)) in plan.changes.into_iter().enumerate() {
            // what it was, written down before it's changed
            let before = match &script {
                Some(s) => Ok(undo_of(s)),
                None => self.read(&what),
            };
            let before = match before {
                Ok(b) => b,
                Err(why) => {
                    let _ = self.say.send(Told::Failed { words: words.clone(), why });
                    self.step(k + 1, false);
                    continue;
                }
            };
            self.journal.sessions[at].changes.push(Change { plugin, what: what.clone(), label, words: words.clone(), before, made: false });
            self.journal.save();
            let i = self.journal.sessions[at].changes.len() - 1;
            let made = match &script {
                Some(s) => {
                    let (ran, child) = commands::run(&s.start, s.limit, s.wait, s.keep, &Hands::env(&game));
                    if let Some(c) = child {
                        if s.keep {
                            self.journal.sessions[at].changes[i].before["pid"] = json!(c.id());
                            self.kept.insert((game.id.clone(), i), c);
                        }
                    }
                    let ok = ran.ok();
                    let _ = self.say.send(Told::Ran { script: s.id, game: game.name.clone(), quit: false, ran, at: crate::games::now() });
                    ok
                }
                None => match self.make(&what) {
                    Ok(()) => true,
                    Err(why) => {
                        let _ = self.say.send(Told::Failed { words: words.clone(), why });
                        false
                    }
                },
            };
            self.journal.sessions[at].changes[i].made = made;
            self.journal.save();
            self.step(k + 1, false);
        }
        let n = self.steps.lock().unwrap_or_else(|p| p.into_inner()).as_ref().map_or(0, |s| s.labels.len());
        self.step(n, true);
        let _ = self.say.send(Told::Started { id: game.id.clone(), seq: plan_seq });
    }

    fn read(&mut self, what: &Value) -> Result<Value, String> {
        let (Some(m), Some(set)) = (what["monitor"].as_str(), what["set"].as_str()) else { return Err("Ectomatter doesn’t know how to make that change.".into()) };
        let s = self.screens.as_mut().ok_or("Ectomatter can’t reach this computer’s monitors.")?;
        s.now(m, set)
    }

    fn make(&mut self, what: &Value) -> Result<(), String> {
        let (Some(m), Some(set)) = (what["monitor"].as_str(), what["set"].as_str()) else { return Err("Ectomatter doesn’t know how to make that change.".into()) };
        let s = self.screens.as_mut().ok_or("Ectomatter can’t reach this computer’s monitors.")?;
        s.set(m, set, &what["to"])
    }

    /// One change put back: a monitor's setting as it was, or a command's
    /// undo run (its process stopped first, if it was kept running).
    fn undo(&mut self, game: &Game, i: usize, c: &Change) -> Result<(), String> {
        if c.what.get("command").is_some() {
            if let Some(mut child) = self.kept.remove(&(game.id.clone(), i)) {
                commands::stop(&mut child);
            } else if let Some(pid) = c.before["pid"].as_u64() {
                // (kept running by an Ectomatter before this one: stopped if it's still the same process)
                stop_left(pid as u32, game.since);
            }
            let quit = c.before["quit"].as_str().unwrap_or("");
            if !quit.trim().is_empty() {
                let limit = c.before["limit"].as_u64().unwrap_or(10);
                let (ran, _) = commands::run(quit, limit, true, false, &Hands::env(game));
                let ok = ran.ok();
                let _ = self.say.send(Told::Ran { script: c.before["script"].as_u64().unwrap_or(0), game: game.name.clone(), quit: true, ran: ran.clone(), at: crate::games::now() });
                if !ok {
                    return Err(ran.words());
                }
            }
            return Ok(());
        }
        let (Some(m), Some(set)) = (c.what["monitor"].as_str(), c.what["set"].as_str()) else { return Ok(()) };
        let s = self.screens.as_mut().ok_or("Ectomatter can’t reach this computer’s monitors.")?;
        if set == "main" {
            // (the main monitor goes back to the one it was)
            return match c.before.as_str() {
                Some(was) if was != m => s.set(was, "main", &json!(true)),
                _ => Ok(()),
            };
        }
        s.set(m, set, &c.before)
    }

    /// A game's changes put back, the last first; a change another game
    /// still running made since is left to that game, with what it was.
    fn quit(&mut self, id: &str, after_stop: bool) {
        let Some(at) = self.journal.sessions.iter().position(|s| s.game.id == id) else { return };
        let session = self.journal.sessions[at].clone();
        let made: Vec<(usize, Change)> = session.changes.iter().cloned().enumerate().filter(|(_, c)| c.made || c.what.get("command").is_none()).collect();
        self.show(Some(Steps { game: session.game.name.clone(), phase: "exit", labels: made.iter().rev().map(|(_, c)| c.label.clone()).collect(), done: 0, finished: made.is_empty() }));
        let mut words = Vec::new();
        for (n, (i, c)) in made.iter().rev().enumerate() {
            // a later game holding the same thing takes what it was
            let later = c.target().and_then(|t| {
                self.journal.sessions.iter().enumerate().skip(at + 1).find_map(|(j, s)| s.changes.iter().position(|x| x.made && x.target().as_deref() == Some(&t)).map(|k| (j, k)))
            });
            if let Some((j, k)) = later {
                self.journal.sessions[j].changes[k].before = c.before.clone();
            } else {
                match self.undo(&session.game, *i, c) {
                    Ok(()) => words.push(c.words.clone()),
                    Err(why) => {
                        let _ = self.say.send(Told::Failed { words: c.words.clone(), why });
                    }
                }
            }
            // (each one put back is struck from the record at once)
            self.journal.sessions[at].changes.retain(|x| x != c);
            self.journal.save();
            self.step(n + 1, false);
        }
        self.journal.sessions.remove(at);
        self.journal.save();
        self.step(made.len(), true);
        let _ = self.say.send(Told::PutBack { name: session.game.name.clone(), words, after_stop });
    }

    /// At Ectomatter's start: a game recorded as changed that's still running
    /// keeps its changes until it quits; one that isn't has them put back now.
    fn settle(&mut self, running: &[crate::games::Running]) {
        // (only what an Ectomatter before this one left: a game this one has started since is its own)
        let left = std::mem::take(&mut self.left);
        let gone: Vec<String> = self.journal.sessions.iter().filter(|s| left.contains(&(s.game.id.clone(), s.game.pid)) && !running.iter().any(|r| r.id == s.game.id && r.pid == s.game.pid)).map(|s| s.game.id.clone()).collect();
        for id in gone {
            self.quit(&id, true);
        }
        self.show(None);
    }

    /// Run now: a command's start, then its undo, so nothing is left changed.
    fn try_now(&mut self, s: &Script) {
        let game = Game { id: String::new(), name: "Run now".into(), pid: 0, since: 0 };
        let (ran, child) = commands::run(&s.start, s.limit, true, s.keep, &Hands::env(&game));
        let ok = ran.ok();
        let _ = self.say.send(Told::Ran { script: s.id, game: String::new(), quit: false, ran, at: crate::games::now() });
        if let Some(mut c) = child {
            commands::stop(&mut c);
        }
        if ok && !s.quit.trim().is_empty() {
            let (ran, _) = commands::run(&s.quit, s.limit, true, false, &Hands::env(&game));
            let _ = self.say.send(Told::Ran { script: s.id, game: String::new(), quit: true, ran, at: crate::games::now() });
        }
    }

    /// A plugin turned off: what it changed for the games running put back now.
    fn drop_plugin(&mut self, plugin: &str) {
        for at in 0..self.journal.sessions.len() {
            let game = self.journal.sessions[at].game.clone();
            let mine: Vec<(usize, Change)> = self.journal.sessions[at].changes.iter().cloned().enumerate().filter(|(_, c)| c.plugin == plugin).collect();
            for (i, c) in mine.iter().rev() {
                if c.made || c.what.get("command").is_none() {
                    if let Err(why) = self.undo(&game, *i, c) {
                        let _ = self.say.send(Told::Failed { words: c.words.clone(), why });
                    }
                }
                self.journal.sessions[at].changes.retain(|x| x != c);
                self.journal.save();
            }
        }
    }
}

/// A command an Ectomatter before this one kept running, stopped, if it's
/// still the process it started (one started no earlier than its game).
#[cfg(unix)]
fn stop_left(pid: u32, since: u64) {
    let still = std::fs::read_to_string(crate::computer::here(format!("/proc/{pid}/stat"))).ok().is_some_and(|_| crate::games::started_at(pid).is_none_or(|t| t + 1 >= since));
    if still {
        // SAFETY: signalling the process group that command led
        unsafe {
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
    }
}

#[cfg(not(unix))]
fn stop_left(_pid: u32, _since: u64) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Monitors kept in a table: each setting by monitor and name.
    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<HashMap<String, Value>>>);

    impl Screens for Fake {
        fn now(&mut self, m: &str, set: &str) -> Result<Value, String> {
            let t = self.0.lock().unwrap();
            if set == "main" {
                return Ok(t.get("main").cloned().unwrap_or(Value::Null));
            }
            t.get(&format!("{m}/{set}")).cloned().ok_or_else(|| format!("no {set} on {m}"))
        }
        fn set(&mut self, m: &str, set: &str, to: &Value) -> Result<(), String> {
            let mut t = self.0.lock().unwrap();
            if set == "main" {
                t.insert("main".into(), json!(m));
            } else {
                t.insert(format!("{m}/{set}"), to.clone());
            }
            Ok(())
        }
    }

    fn game(id: &str, pid: u32) -> Game {
        Game { id: id.into(), name: id.to_uppercase(), pid, since: 1 }
    }

    fn bright(m: &str, to: u64) -> (String, Value, String, String, Option<Script>) {
        ("monitor-switcher".into(), json!({ "monitor": m, "set": "brightness", "to": to }), "BRIGHTNESS".into(), format!("the {m}’s brightness"), None)
    }

    /// What the thread says, up to `n` things, its starts made aside.
    fn wait_for(w: &Worker, n: usize) -> Vec<Told> {
        let mut out = Vec::new();
        while out.len() < n {
            match w.told.recv_timeout(std::time::Duration::from_secs(10)) {
                Ok(Told::Started { .. }) => {}
                Ok(t) => out.push(t),
                Err(_) => break,
            }
        }
        out
    }

    /// A game's start is said once its changes are made, for a game started
    /// through Ectomatter's launcher, which waits on it.
    #[test]
    fn a_start_is_said_once_its_changes_are_made() {
        let fake = Fake::default();
        fake.0.lock().unwrap().insert("side/brightness".into(), json!(3500));
        let w = Worker::start(Journal::load(None), Some(Box::new(fake.clone())));
        w.start_game(Plan { game: game("arc", 10), changes: vec![bright("side", 0)], seq: 7 });
        let t = w.told.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        assert_eq!(t, Told::Started { id: "arc".into(), seq: 7 });
        assert_eq!(fake.0.lock().unwrap()["side/brightness"], json!(0), "made before it's said");
    }

    /// Changes made as a game starts are put back as it quits, the last first.
    #[test]
    fn what_a_game_changes_goes_back_as_it_quits() {
        let fake = Fake::default();
        fake.0.lock().unwrap().insert("side/brightness".into(), json!(3500));
        fake.0.lock().unwrap().insert("main".into(), json!("big"));
        let w = Worker::start(Journal::load(None), Some(Box::new(fake.clone())));
        let main = ("monitor-switcher".into(), json!({ "monitor": "side", "set": "main", "to": true }), "MAIN MONITOR".into(), "the side’s place".into(), None);
        w.start_game(Plan { game: game("arc", 10), changes: vec![main, bright("side", 0)], seq: 0 });
        w.quit_game("arc");
        let told = wait_for(&w, 1);
        assert_eq!(told, vec![Told::PutBack { name: "ARC".into(), words: vec!["the side’s brightness".into(), "the side’s place".into()], after_stop: false }]);
        let t = fake.0.lock().unwrap();
        assert_eq!(t["side/brightness"], json!(3500));
        assert_eq!(t["main"], json!("big"));
    }

    /// A game that quits under another holding the same setting hands it what
    /// it was, so the second puts back the first's start.
    #[test]
    fn two_games_hand_a_setting_on() {
        let fake = Fake::default();
        fake.0.lock().unwrap().insert("side/brightness".into(), json!(3500));
        let w = Worker::start(Journal::load(None), Some(Box::new(fake.clone())));
        w.start_game(Plan { game: game("a", 1), changes: vec![bright("side", 0)], seq: 0 });
        w.start_game(Plan { game: game("b", 2), changes: vec![bright("side", 2500)], seq: 0 });
        w.quit_game("a");
        let _ = wait_for(&w, 1);
        assert_eq!(fake.0.lock().unwrap()["side/brightness"], json!(2500), "b still holds it");
        w.quit_game("b");
        let _ = wait_for(&w, 1);
        assert_eq!(fake.0.lock().unwrap()["side/brightness"], json!(3500), "and puts back what it was before a");
    }

    /// The record outlives Ectomatter: a game recorded and no longer running
    /// has its changes put back at the next start, and that's said; one still
    /// running keeps them.
    #[test]
    fn a_record_left_behind_is_put_back() {
        let path = std::env::temp_dir().join(format!("ectomatter-put-back-{}.json", std::process::id()));
        let fake = Fake::default();
        fake.0.lock().unwrap().insert("side/brightness".into(), json!(3500));
        {
            let mut j = Journal::load(Some(path.clone()));
            j.sessions.push(Session { game: game("gone", 50), changes: vec![Change { plugin: "monitor-switcher".into(), what: json!({ "monitor": "side", "set": "brightness", "to": 0 }), label: "BRIGHTNESS".into(), words: "the side’s brightness".into(), before: json!(3500), made: true }] });
            j.sessions.push(Session { game: game("still", 60), changes: vec![] });
            j.save();
            fake.0.lock().unwrap().insert("side/brightness".into(), json!(0));
        }
        let w = Worker::start(Journal::load(Some(path.clone())), Some(Box::new(fake.clone())));
        w.settle(vec![crate::games::Running { id: "still".into(), pid: 60, since: 1 }]);
        let told = wait_for(&w, 1);
        assert_eq!(told, vec![Told::PutBack { name: "GONE".into(), words: vec!["the side’s brightness".into()], after_stop: true }]);
        assert_eq!(fake.0.lock().unwrap()["side/brightness"], json!(3500));
        // (the game still running is still recorded)
        std::thread::sleep(std::time::Duration::from_millis(50));
        let left = Journal::load(Some(path.clone()));
        assert_eq!(left.sessions.iter().map(|s| s.game.id.as_str()).collect::<Vec<_>>(), vec!["still"]);
        let _ = std::fs::remove_file(&path);
    }

    /// A command runs as its game starts and its undo as the game quits.
    #[cfg(unix)]
    #[test]
    fn a_command_and_its_undo() {
        let f = std::env::temp_dir().join(format!("ectomatter-script-{}", std::process::id()));
        let s = Script { id: 3, name: "Mark".into(), start: format!("echo started > '{}'", f.display()), quit: format!("rm '{}'", f.display()), wait: true, keep: false, limit: 5 };
        let w = Worker::start(Journal::load(None), None);
        w.start_game(Plan { game: game("arc", 1), changes: vec![(crate::front::CUSTOM_SCRIPTS.into(), json!({ "command": 3 }), "MARK".into(), "Mark".into(), Some(s))], seq: 0 });
        let told = wait_for(&w, 1);
        assert!(matches!(&told[0], Told::Ran { script: 3, quit: false, ran: Ran::Done(_), .. }), "{told:?}");
        assert!(f.exists());
        w.quit_game("arc");
        let told = wait_for(&w, 2);
        assert!(matches!(&told[0], Told::Ran { script: 3, quit: true, ran: Ran::Done(_), .. }), "{told:?}");
        assert!(!f.exists());
    }
}
