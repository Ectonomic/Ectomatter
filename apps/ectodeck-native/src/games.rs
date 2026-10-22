//! The Library: the games and apps Ectodeck knows, and what each one switches.
//! Steam's come from Steam's own files each time (steam.rs). Ectodeck keeps
//! what is its own in one file in its data folder (library.json): the
//! favorites, the games and apps added by hand, and for each game or app the
//! profile each device switches to while it's in front ("Use with an app…"
//! ties a profile to an app the same way, in the same place), and Every
//! game's, which a game with no choice of its own follows. Also: the games
//! running now, watched once a second on a thread of its own.

use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::steam;

/// Every game's choices, which a game with none of its own follows.
pub const EVERY: &str = "game:*";

/// A Steam app's id in the Library: the name Steam gives its games' windows.
pub fn steam_id(n: u32) -> String {
    format!("steam_app_{n}")
}

pub fn steam_number(id: &str) -> Option<u32> {
    id.strip_prefix("steam_app_")?.parse().ok()
}

/// What a device does while a game or app is in front: switch to one of its
/// profiles (by the profile's own id: the keyboard's by its number, the
/// mouse's by the id Ectodeck gave it), or stay on the profile it's on.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    Profile(String),
    Stay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Game,
    App,
}

/// Where a game or app came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum From {
    Steam,
    /// added in the Library by hand
    Hand,
    /// tied to a profile with "Use with an app…"
    Tied,
}

/// A game or app as the Library shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub from: From,
    /// minutes played in all and when last played (seconds since 1970; 0 never), as Steam counts them
    pub minutes: u64,
    pub last: u64,
    pub art: steam::Art,
    /// a game added by hand: its program
    pub program: Option<PathBuf>,
}

/// What a game or app taken out of the Library had, for Undo to put back: its
/// place among those added by hand and its record there, its choices, and
/// whether it was a favorite.
#[derive(Clone, Debug, PartialEq)]
pub struct Removed {
    pub id: String,
    added: Option<(usize, Value)>,
    choices: Option<Value>,
    favorite: bool,
}

/// What Ectodeck keeps of the Library, and Steam's games and apps as last read.
pub struct Library {
    /// where it's kept: none for an offscreen check or a test, which keep nothing
    path: Option<PathBuf>,
    v: Value,
    pub steam: Option<steam::Steam>,
}

/// Where the Library is kept. An offscreen check can be given one to read
/// (ECTODECK_NATIVE_LIBRARY), which it reads and never writes.
fn kept_at() -> (Option<PathBuf>, Option<PathBuf>) {
    let offscreen = cfg!(test) || std::env::args().any(|a| a == "--snapshot");
    if offscreen {
        return (std::env::var_os("ECTODECK_NATIVE_LIBRARY").filter(|_| !cfg!(test)).map(PathBuf::from), None);
    }
    let p = crate::data_dir().join("library.json");
    (Some(p.clone()), Some(p))
}

impl Library {
    pub fn load() -> Library {
        let (read, write) = kept_at();
        let v = read.and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice::<Value>(&b).ok()).filter(|v| v.is_object()).unwrap_or_else(|| json!({}));
        Library { path: write, v, steam: None }
    }

    /// A Library kept nowhere, as given (for the tests).
    #[cfg(test)]
    pub fn of(v: Value, steam: Option<steam::Steam>) -> Library {
        Library { path: None, v, steam }
    }

    fn save(&self) {
        let Some(p) = &self.path else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, serde_json::to_vec_pretty(&self.v).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, p);
        }
    }

    fn obj(&mut self, key: &str) -> &mut Map<String, Value> {
        if !self.v[key].is_object() {
            self.v[key] = json!({});
        }
        self.v[key].as_object_mut().expect("an object")
    }

    // ------------------------------------------------------------ what each switches

    /// What a game or app has chosen of its own for a device, if anything.
    pub fn choice(&self, id: &str, device: &str) -> Option<Choice> {
        match self.v["choices"][id][device].as_str()? {
            "stay" => Some(Choice::Stay),
            p => Some(Choice::Profile(p.to_string())),
        }
    }

    /// A game's or app's own choice for a device, or none (a game then follows
    /// Every game's, an app stays as it is).
    pub fn set_choice(&mut self, id: &str, device: &str, c: Option<Choice>) {
        let choices = self.obj("choices");
        let mine = choices.entry(id.to_string()).or_insert_with(|| json!({}));
        if !mine.is_object() {
            *mine = json!({});
        }
        let m = mine.as_object_mut().expect("an object");
        match c {
            Some(Choice::Stay) => {
                m.insert(device.into(), json!("stay"));
            }
            Some(Choice::Profile(p)) => {
                m.insert(device.into(), json!(p));
            }
            None => {
                m.remove(device);
            }
        }
        if m.is_empty() {
            choices.remove(id);
        }
        self.save();
    }

    /// The games and apps a device's profile is chosen for, Every game among them.
    pub fn tied(&self, device: &str, profile: &str) -> Vec<String> {
        self.v["choices"].as_object().map_or(Vec::new(), |m| m.iter().filter(|(_, c)| c[device].as_str() == Some(profile)).map(|(id, _)| id.clone()).collect())
    }

    /// What a device does while this game or app is in front: its own choice,
    /// or for a game, Every game's.
    pub fn wanted(&self, id: &str, device: &str) -> Option<Choice> {
        self.choice(id, device).or_else(|| self.is_game(id).then(|| self.choice(EVERY, device)).flatten())
    }

    /// Ties made before the Library kept them (each device's profile and the
    /// apps it was tied to), taken in once; an app that chose already keeps
    /// its choice. True if any were taken.
    pub fn take_ties(&mut self, device: &str, ties: &[(String, Vec<String>)]) -> bool {
        let mut took = false;
        for (profile, apps) in ties {
            for app in apps {
                if self.choice(app, device).is_none() {
                    self.set_choice(app, device, Some(Choice::Profile(profile.clone())));
                    took = true;
                }
            }
        }
        took
    }

    // ------------------------------------------------------------ favorites

    pub fn is_favorite(&self, id: &str) -> bool {
        self.v["favorites"].as_array().is_some_and(|a| a.iter().any(|x| x.as_str() == Some(id)))
    }

    pub fn set_favorite(&mut self, id: &str, on: bool) {
        let mut list: Vec<String> = self.v["favorites"].as_array().map_or(Vec::new(), |a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect());
        list.retain(|x| x != id);
        if on {
            list.push(id.to_string());
        }
        self.v["favorites"] = json!(list);
        self.save();
    }

    /// The first time Steam is read: the games and apps starred in Steam
    /// become the first favorites. After that, only stars in the Library change them.
    pub fn seed_favorites(&mut self) {
        if self.v["favorites_from_steam"].as_bool() == Some(true) {
            return;
        }
        let Some(s) = &self.steam else { return };
        let starred: Vec<String> = s.favorites.iter().filter(|n| s.apps.iter().any(|a| a.id == **n)).map(|n| steam_id(*n)).collect();
        let mut list: Vec<String> = self.v["favorites"].as_array().map_or(Vec::new(), |a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect());
        for id in starred {
            if !list.contains(&id) {
                list.push(id);
            }
        }
        self.v["favorites"] = json!(list);
        self.v["favorites_from_steam"] = json!(true);
        self.save();
    }

    // ------------------------------------------------------------ the games and apps

    /// A game added by hand, by its program; its id, which names it in the Library.
    pub fn add_game(&mut self, program: &Path, name: &str) -> String {
        let p = program.to_string_lossy().to_string();
        // (named by its program, so the same program added twice is one game)
        let hash = p.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
        let id = format!("game:{hash:016x}");
        let mut added = self.added_raw();
        added.retain(|a| a["id"].as_str() != Some(&id));
        added.push(json!({ "id": id, "kind": "game", "name": name, "program": p }));
        self.v["added"] = json!(added);
        self.save();
        id
    }

    /// An app added by hand, by its id (as front_app.rs names an app) and name.
    pub fn add_app(&mut self, id: &str, name: &str) {
        let mut added = self.added_raw();
        if added.iter().any(|a| a["id"].as_str() == Some(id)) {
            return;
        }
        added.push(json!({ "id": id, "kind": "app", "name": name }));
        self.v["added"] = json!(added);
        self.save();
    }

    /// A game or app taken out of the Library (one added by hand, or an app
    /// that's there for its ties), with its choices: what it had, for Undo.
    pub fn remove(&mut self, id: &str) -> Removed {
        let mut added = self.added_raw();
        let at = added.iter().position(|a| a["id"].as_str() == Some(id));
        let was = at.map(|i| (i, added.remove(i)));
        self.v["added"] = json!(added);
        let choices = self.obj("choices").remove(id);
        let favorite = self.is_favorite(id);
        self.set_favorite(id, false);
        Removed { id: id.to_string(), added: was, choices, favorite }
    }

    /// Undo: a game or app taken out put back as it was.
    pub fn restore(&mut self, r: Removed) {
        if let Some((i, a)) = r.added {
            let mut added = self.added_raw();
            added.insert(i.min(added.len()), a);
            self.v["added"] = json!(added);
        }
        if let Some(c) = r.choices {
            self.obj("choices").insert(r.id.clone(), c);
        }
        if r.favorite {
            self.set_favorite(&r.id, true);
        }
        self.save();
    }

    fn added_raw(&self) -> Vec<Value> {
        self.v["added"].as_array().cloned().unwrap_or_default()
    }

    /// Every game and app, Steam's first, then those added by hand, then the
    /// apps a profile is tied to that are neither. `name_of` names a tied app.
    pub fn entries(&self, name_of: &dyn Fn(&str) -> String) -> Vec<Entry> {
        let mut out: Vec<Entry> = Vec::new();
        if let Some(s) = &self.steam {
            for a in &s.apps {
                let kind = if a.kind == steam::Kind::Game { Kind::Game } else { Kind::App };
                out.push(Entry { id: steam_id(a.id), name: a.name.clone(), kind, from: From::Steam, minutes: a.minutes, last: a.last, art: a.art.clone(), program: None });
            }
        }
        for a in self.added_raw() {
            let Some(id) = a["id"].as_str() else { continue };
            if out.iter().any(|e| e.id == id) {
                continue;
            }
            let kind = if a["kind"] == "app" { Kind::App } else { Kind::Game };
            let program = a["program"].as_str().map(PathBuf::from);
            out.push(Entry { id: id.into(), name: a["name"].as_str().unwrap_or(id).into(), kind, from: From::Hand, minutes: 0, last: 0, art: steam::Art::default(), program });
        }
        if let Some(m) = self.v["choices"].as_object() {
            for (id, c) in m {
                if id == EVERY || out.iter().any(|e| &e.id == id) || c.as_object().is_none_or(|c| c.is_empty()) {
                    continue;
                }
                // (a Steam game since uninstalled keeps its choices, out of sight)
                if steam_number(id).is_some() || id.starts_with("game:") {
                    continue;
                }
                out.push(Entry { id: id.clone(), name: name_of(id), kind: Kind::App, from: From::Tied, minutes: 0, last: 0, art: steam::Art::default(), program: None });
            }
        }
        out
    }

    /// Whether an id is a game's: a Steam game's, or one added by hand.
    pub fn is_game(&self, id: &str) -> bool {
        if id == EVERY {
            return false;
        }
        if let Some(n) = steam_number(id) {
            return self.steam.as_ref().is_none_or(|s| s.apps.iter().find(|a| a.id == n).is_none_or(|a| a.kind == steam::Kind::Game));
        }
        self.added_raw().iter().any(|a| a["id"].as_str() == Some(id) && a["kind"] != "app")
    }

    /// The games added by hand, each by its id and program: what the watch looks for.
    pub fn programs(&self) -> Vec<(String, PathBuf)> {
        self.added_raw().iter().filter(|a| a["kind"] != "app").filter_map(|a| Some((a["id"].as_str()?.to_string(), PathBuf::from(a["program"].as_str()?)))).collect()
    }
}

/// A link or file opened as the desktop opens it: a steam:// link goes to
/// Steam, as a desktop shortcut's does.
pub fn open(target: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let c = {
        let mut c = std::process::Command::new("open");
        c.arg(target);
        c
    };
    #[cfg(windows)]
    let c = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", target]);
        c
    };
    #[cfg(not(any(target_os = "macos", windows)))]
    let c = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(target);
        c
    };
    spawn(c)
}

/// A game added by hand started: its program run as it is where it can be,
/// else opened as the desktop opens a file of its kind.
pub fn start(program: &Path) -> Result<(), String> {
    if !program.exists() {
        return Err(format!("{} isn’t there anymore.", program.display()));
    }
    if runnable(program) {
        let mut c = std::process::Command::new(program);
        if let Some(dir) = program.parent() {
            c.current_dir(dir);
        }
        return spawn(c);
    }
    open(&program.to_string_lossy())
}

#[cfg(unix)]
fn runnable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn runnable(p: &Path) -> bool {
    p.extension().is_some_and(|e| ["exe", "bat", "cmd"].iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// A program set going on its own: nothing of Ectodeck's waits on it, and
/// once it ends it's let go.
fn spawn(mut c: std::process::Command) -> Result<(), String> {
    use std::process::Stdio;
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = c.spawn().map_err(|e| e.to_string())?;
    let _ = std::thread::Builder::new().name("started".into()).spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// A name for sorting: a leading "The" counts for nothing.
pub fn sort_key(name: &str) -> String {
    let n = name.trim();
    let n = match n.get(..4) {
        Some(the) if n.len() > 4 && the.eq_ignore_ascii_case("the ") => &n[4..],
        _ => n,
    };
    n.to_lowercase()
}

// ---------------------------------------------------------------- the games running

/// A game running now: its id in the Library, the process it runs under (a
/// Steam game's reaper, or the program added by hand), and when that started
/// (seconds since 1970).
#[derive(Clone, Debug, PartialEq)]
pub struct Running {
    pub id: String,
    pub pid: u32,
    pub since: u64,
}

/// The games running now, looked for once a second on a thread of its own.
/// Each time the list changes it comes back through `seen`; `programs` says
/// which programs are games added by hand.
pub struct Watch {
    pub seen: mpsc::Receiver<Vec<Running>>,
    pub programs: Arc<Mutex<Vec<(String, PathBuf)>>>,
}

impl Watch {
    pub fn start() -> Watch {
        let (tx, seen) = mpsc::channel();
        let programs: Arc<Mutex<Vec<(String, PathBuf)>>> = Arc::new(Mutex::new(Vec::new()));
        let mine = programs.clone();
        std::thread::Builder::new()
            .name("game watch".into())
            .spawn(move || {
                let mut last: Option<Vec<Running>> = None;
                loop {
                    let programs = mine.lock().map(|p| p.clone()).unwrap_or_default();
                    let mut now = running_now(&programs);
                    // (a game keeps the time it was first seen at)
                    if let Some(before) = &last {
                        for r in &mut now {
                            if let Some(b) = before.iter().find(|b| b.id == r.id && b.pid == r.pid) {
                                r.since = b.since;
                            }
                        }
                    }
                    if last.as_ref() != Some(&now) {
                        if tx.send(now.clone()).is_err() {
                            return;
                        }
                        last = Some(now);
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            })
            .ok();
        Watch { seen, programs }
    }
}

/// The games running now: Steam's, by their reapers, and those added by
/// hand, by their programs.
fn running_now(programs: &[(String, PathBuf)]) -> Vec<Running> {
    let mut out: Vec<Running> = steam::running().into_iter().map(|(n, pid)| Running { id: steam_id(n), pid, since: started(pid).unwrap_or_else(now) }).collect();
    for (id, pid) in hand_running(programs) {
        if !out.iter().any(|r| r.id == id) {
            out.push(Running { id, pid, since: started(pid).unwrap_or_else(now) });
        }
    }
    out
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// When a process started, in seconds since 1970: the computer's start plus
/// the process's own start, as its stat line counts it in clock ticks.
#[cfg(target_os = "linux")]
fn started(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let ticks: u64 = stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()?;
    let boot: u64 = std::fs::read_to_string("/proc/stat").ok()?.lines().find_map(|l| l.strip_prefix("btime "))?.trim().parse().ok()?;
    // SAFETY: sysconf only reads a constant of the system's
    let per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    Some(boot + ticks / u64::try_from(per_second).ok().filter(|t| *t > 0)?)
}

#[cfg(not(target_os = "linux"))]
fn started(_pid: u32) -> Option<u64> {
    None
}

/// The games added by hand running now, each by its id and the first process
/// found running its program: as the program itself, or as what a program
/// was given to run (a Windows game run by Wine names its .exe so).
#[cfg(target_os = "linux")]
fn hand_running(programs: &[(String, PathBuf)]) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    if programs.is_empty() {
        return out;
    }
    let mut pids: Vec<u32> = std::fs::read_dir("/proc").into_iter().flatten().flatten().filter_map(|e| e.file_name().to_str()?.parse().ok()).collect();
    pids.sort_unstable();
    for pid in pids {
        let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
        let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        for (id, program) in programs {
            if out.iter().any(|(i, _)| i == id) {
                continue;
            }
            if exe.as_deref() == Some(program.as_path()) || runs(&cmd, program) {
                out.push((id.clone(), pid));
            }
        }
    }
    out
}

#[cfg(not(target_os = "linux"))]
fn hand_running(_programs: &[(String, PathBuf)]) -> Vec<(String, u32)> {
    Vec::new()
}

/// Whether a command line runs a program: names it as one of its words, as
/// itself or as Wine writes a path (Z:\home\...).
pub fn runs(cmdline: &[u8], program: &Path) -> bool {
    let want = program.to_string_lossy();
    cmdline.split(|b| *b == 0).filter(|a| !a.is_empty()).any(|a| {
        let a = String::from_utf8_lossy(a);
        a == want || a.strip_prefix("Z:").or_else(|| a.strip_prefix("z:")).is_some_and(|rest| rest.replace('\\', "/") == want)
    })
}

/// The running game a process belongs to: the game whose process it is or
/// comes from (a game's windows belong to processes under it).
pub fn game_of(pid: u32, running: &[Running]) -> Option<String> {
    let mut p = pid;
    for _ in 0..64 {
        if let Some(r) = running.iter().find(|r| r.pid == p) {
            return Some(r.id.clone());
        }
        p = parent(p)?;
        if p <= 1 {
            return None;
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn parent(pid: u32) -> Option<u32> {
    steam::parent_in(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

#[cfg(not(target_os = "linux"))]
fn parent(_pid: u32) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steam_of(apps: &[(u32, &str, steam::Kind)], favorites: &[u32]) -> steam::Steam {
        steam::Steam {
            root: PathBuf::new(),
            apps: apps.iter().map(|(id, name, kind)| steam::App { id: *id, name: name.to_string(), kind: *kind, minutes: 0, last: 0, library: PathBuf::new(), art: steam::Art::default() }).collect(),
            favorites: favorites.to_vec(),
        }
    }

    #[test]
    fn a_game_follows_every_game_until_it_chooses() {
        let mut l = Library::of(json!({}), Some(steam_of(&[(1808500, "ARC Raiders", steam::Kind::Game), (250820, "SteamVR", steam::Kind::App)], &[])));
        l.set_choice(EVERY, "z10", Some(Choice::Profile("2".into())));
        assert_eq!(l.wanted("steam_app_1808500", "z10"), Some(Choice::Profile("2".into())));
        // an app doesn't follow Every game
        assert_eq!(l.wanted("steam_app_250820", "z10"), None);
        assert_eq!(l.wanted("org.kde.konsole", "z10"), None);
        // a game's own Don't switch wins over Every game's profile
        l.set_choice("steam_app_1808500", "z10", Some(Choice::Stay));
        assert_eq!(l.wanted("steam_app_1808500", "z10"), Some(Choice::Stay));
        // and going back to following
        l.set_choice("steam_app_1808500", "z10", None);
        assert_eq!(l.wanted("steam_app_1808500", "z10"), Some(Choice::Profile("2".into())));
        assert!(l.v["choices"].get("steam_app_1808500").is_none(), "an empty record is taken out");
    }

    #[test]
    fn a_profile_knows_what_its_chosen_for() {
        let mut l = Library::of(json!({}), None);
        l.set_choice("steam", "z10", Some(Choice::Profile("2".into())));
        l.set_choice(EVERY, "z10", Some(Choice::Profile("2".into())));
        l.set_choice("org.kde.konsole", "z10", Some(Choice::Profile("3".into())));
        l.set_choice("steam", "sensei310", Some(Choice::Profile("p1".into())));
        let mut t = l.tied("z10", "2");
        t.sort();
        assert_eq!(t, vec!["game:*".to_string(), "steam".to_string()]);
        assert_eq!(l.tied("sensei310", "p1"), vec!["steam".to_string()]);
    }

    #[test]
    fn old_ties_are_taken_in_once_and_never_over_a_choice() {
        let mut l = Library::of(json!({}), None);
        l.set_choice("krita", "z10", Some(Choice::Profile("4".into())));
        let took = l.take_ties("z10", &[("2".into(), vec!["steam".into(), "krita".into()])]);
        assert!(took);
        assert_eq!(l.choice("steam", "z10"), Some(Choice::Profile("2".into())));
        assert_eq!(l.choice("krita", "z10"), Some(Choice::Profile("4".into())));
    }

    #[test]
    fn steams_stars_seed_the_favorites_once() {
        let s = steam_of(&[(1, "A", steam::Kind::Game), (2, "B", steam::Kind::Game)], &[2, 99]);
        let mut l = Library::of(json!({}), Some(s));
        l.seed_favorites();
        assert!(l.is_favorite("steam_app_2"));
        // (a favorite that isn't installed isn't one here)
        assert!(!l.is_favorite("steam_app_99"));
        l.set_favorite("steam_app_2", false);
        l.seed_favorites();
        assert!(!l.is_favorite("steam_app_2"), "only the Library's stars change them after the first time");
    }

    #[test]
    fn the_library_lists_steams_then_the_added_then_the_tied() {
        let mut l = Library::of(json!({}), Some(steam_of(&[(1808500, "ARC Raiders", steam::Kind::Game), (250820, "SteamVR", steam::Kind::App)], &[])));
        let g = l.add_game(Path::new("/games/xenia_canary.exe"), "Xenia");
        l.add_app("org.kde.krita", "Krita");
        l.set_choice("org.kde.konsole", "z10", Some(Choice::Profile("3".into())));
        l.set_choice(EVERY, "z10", Some(Choice::Profile("2".into())));
        let e = l.entries(&|id| format!("named {id}"));
        let ids: Vec<&str> = e.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["steam_app_1808500", "steam_app_250820", g.as_str(), "org.kde.krita", "org.kde.konsole"]);
        assert_eq!(e[2].kind, Kind::Game);
        assert_eq!(e[4].name, "named org.kde.konsole");
        assert!(l.is_game(&g) && !l.is_game("org.kde.krita") && !l.is_game(EVERY));
        // the same program added twice is one game
        assert_eq!(l.add_game(Path::new("/games/xenia_canary.exe"), "Xenia Canary"), g);
        assert_eq!(l.programs().len(), 1);
    }

    #[test]
    fn a_game_taken_out_comes_back_with_undo() {
        let mut l = Library::of(json!({}), None);
        let a = l.add_game(Path::new("/games/a"), "A");
        let b = l.add_game(Path::new("/games/b"), "B");
        l.set_choice(&a, "z10", Some(Choice::Profile("2".into())));
        l.set_favorite(&a, true);
        let gone = l.remove(&a);
        assert!(l.entries(&|i| i.into()).iter().all(|e| e.id != a) && !l.is_favorite(&a) && l.choice(&a, "z10").is_none());
        l.restore(gone);
        let ids: Vec<String> = l.entries(&|i| i.into()).into_iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![a.clone(), b]);
        assert!(l.is_favorite(&a) && l.choice(&a, "z10") == Some(Choice::Profile("2".into())));
    }

    #[test]
    fn a_leading_the_counts_for_nothing_in_a_sort() {
        let mut names = vec!["The Elder Scrolls IV: Oblivion Remastered", "Baldur's Gate 3", "theme park", "THE FINALS"];
        names.sort_by_key(|n| sort_key(n));
        assert_eq!(names, vec!["Baldur's Gate 3", "The Elder Scrolls IV: Oblivion Remastered", "THE FINALS", "theme park"]);
    }

    #[test]
    fn a_program_is_seen_however_its_run() {
        let p = Path::new("/home/u/Desktop/Xbox 360/xenia_canary.exe");
        assert!(runs(b"/home/u/Desktop/Xbox 360/xenia_canary.exe\0--fullscreen\0", p));
        assert!(runs(b"wine64\0Z:\\home\\u\\Desktop\\Xbox 360\\xenia_canary.exe\0", p));
        assert!(!runs(b"/usr/bin/kate\0/home/u/Desktop/Xbox 360/notes.txt\0", p));
    }
}
