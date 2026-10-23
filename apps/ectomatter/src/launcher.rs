//! Games started through Ectomatter (docs/core-protocol.md, section 11, A
//! game's start). Ectomatter's launcher (apps/ectomatter-launch) is installed
//! as a Steam compatibility tool named Ectomatter; a game whose Compatibility
//! setting in Steam names it is handed to the launcher, which starts it from
//! the recipe Ectomatter writes for it here, from the game's choices and the
//! answer of the plugin that says how games start (the Game Configurator).
//! As a game starts, the launcher asks Ectomatter on a socket of its own to
//! make the game's changes first, and waits for them.
//!
//! Steam keeps each game's Compatibility setting and launch options in its
//! own files, which it reads as it starts and writes over as it quits, so
//! Ectomatter changes them only while Steam is closed: it asks Steam to
//! close, copies the files aside, changes them, and opens Steam again.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::vdf::{self, Node};

/// The launcher's name in Steam's Compatibility setting, and its folder
/// in Steam's compatibilitytools.d; and its name for a game's Linux version
/// (a second tool in the same folder, as Kawariki registers its own, for
/// Steam to start the Linux version through it, not the Windows one).
pub const TOOL: &str = "ectomatter";
pub const TOOL_LINUX: &str = "ectomatter-linux";

/// Whether a tool in Steam's Compatibility setting is Ectomatter's.
pub fn is_ours(t: &str) -> bool {
    t == TOOL || t == TOOL_LINUX
}

/// Whether a tool in Steam's Compatibility setting runs a game's Linux
/// version: Ectomatter's for it, or one of Steam's runtimes (a Steam Linux
/// Runtime, as `steamlinuxruntime_sniper`, or the legacy one), never Proton.
fn runs_linux(t: &str) -> bool {
    let l = t.to_ascii_lowercase();
    t == TOOL_LINUX || (l.contains("runtime") && !l.contains("proton"))
}

/// How long a game's start waits for its changes, as the launcher waits.
const HOLD: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------- what's installed

/// A Proton build installed: its id as Steam's Compatibility setting names
/// it, its name as people know it, its folder, the Steam Linux Runtime it
/// runs in, and who made it ("ge", "cachyos", "valve" or "other").
#[derive(Clone, Debug, PartialEq)]
pub struct Build {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub runtime: Option<PathBuf>,
    pub family: &'static str,
}

/// Valve's Proton builds by their Steam number: each one's id in Steam's
/// Compatibility setting, and its name.
const VALVE: [(&str, &str, &str); 8] = [
    ("1493710", "proton_experimental", "Proton Experimental"),
    ("2180100", "proton_hotfix", "Proton Hotfix"),
    ("3658110", "proton_10", "Proton 10.0"),
    ("2805730", "proton_9", "Proton 9.0"),
    ("2348590", "proton_8", "Proton 8.0"),
    ("1887720", "proton_7", "Proton 7.0"),
    ("1580130", "proton_63", "Proton 6.3"),
    ("1420170", "proton_513", "Proton 5.13"),
];

/// Steam's libraries: its own folder and each its libraryfolders.vdf names.
pub fn libraries(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.to_path_buf()];
    let text = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")).unwrap_or_default();
    if let Ok(Node::Keys(top)) = vdf::parse(&text) {
        for (_, lib) in top.iter().flat_map(|(_, v)| if let Node::Keys(k) = v { k.clone() } else { Vec::new() }) {
            if let Some(p) = lib.get("path").and_then(Node::words) {
                let p = PathBuf::from(p);
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// The folder an app is installed in, in whichever library holds it.
fn install_dir(libraries: &[PathBuf], app: &str) -> Option<PathBuf> {
    libraries.iter().find_map(|lib| {
        let acf = std::fs::read_to_string(lib.join(format!("steamapps/appmanifest_{app}.acf"))).ok()?;
        let dir = vdf::parse(&acf).ok()?.at(&["AppState", "installdir"])?.words()?.to_string();
        Some(lib.join("steamapps/common").join(dir))
    })
}

/// The runtime a Proton build asks for in its manifest, where it's installed.
fn runtime_for(proton: &Path, libraries: &[PathBuf]) -> Option<PathBuf> {
    let text = std::fs::read_to_string(proton.join("toolmanifest.vdf")).ok()?;
    let id = vdf::parse(&text).ok()?.at(&["manifest", "require_tool_appid"])?.words()?.to_string();
    install_dir(libraries, &id).filter(|p| p.join("_v2-entry-point").is_file())
}

/// Every Proton build installed: those in compatibilitytools.d, by the names
/// their own files give them, then Valve's, newest first. Ectomatter's own
/// launcher is none of them.
pub fn builds(root: &Path) -> Vec<Build> {
    let libs = libraries(root);
    let mut out = Vec::new();
    let tools = root.join("compatibilitytools.d");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&tools).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    dirs.sort();
    for d in dirs {
        if !d.join("proton").is_file() || d.file_name().is_some_and(|n| n == TOOL) {
            continue;
        }
        let text = std::fs::read_to_string(d.join("compatibilitytool.vdf")).unwrap_or_default();
        let (id, name) = vdf::parse(&text)
            .ok()
            .and_then(|n| match n.at(&["compatibilitytools", "compat_tools"]) {
                Some(Node::Keys(k)) => k.first().map(|(id, v)| (id.clone(), v.get("display_name").and_then(Node::words).unwrap_or(id).to_string())),
                _ => None,
            })
            .unwrap_or_else(|| {
                let n = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                (n.clone(), n)
            });
        let low = format!("{id} {name}").to_lowercase();
        let family = if low.contains("cachyos") { "cachyos" } else if low.contains("ge-proton") || low.contains("proton-ge") || low.starts_with("ge") { "ge" } else { "other" };
        let runtime = runtime_for(&d, &libs);
        out.push(Build { id, name, path: d, runtime, family });
    }
    for (app, id, name) in VALVE {
        if let Some(d) = install_dir(&libs, app).filter(|d| d.join("proton").is_file()) {
            let runtime = runtime_for(&d, &libs);
            out.push(Build { id: id.into(), name: name.into(), path: d, runtime, family: "valve" });
        }
    }
    out
}

// ---------------------------------------------------------------- Steam's own settings

fn config_file(root: &Path) -> PathBuf {
    root.join("config/config.vdf")
}

/// Each user's own settings in Steam (localconfig.vdf, where launch options are).
fn local_files(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(root.join("userdata")).map(|rd| rd.flatten().map(|e| e.path().join("config/localconfig.vdf")).filter(|p| p.is_file()).collect()).unwrap_or_default();
    out.sort();
    out
}

const MAPPING: [&str; 5] = ["InstallConfigStore", "Software", "Valve", "Steam", "CompatToolMapping"];
const APPS: [&str; 5] = ["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"];

/// Steam's Compatibility setting, by each app's number ("0" is Steam's
/// default for every game): the tool it names.
pub fn mapping(root: &Path) -> HashMap<String, String> {
    let text = std::fs::read_to_string(config_file(root)).unwrap_or_default();
    let Ok(n) = vdf::parse(&text) else { return HashMap::new() };
    match n.at(&MAPPING) {
        Some(Node::Keys(k)) => k.iter().filter_map(|(app, v)| Some((app.clone(), v.get("name")?.words()?.to_string()))).collect(),
        _ => HashMap::new(),
    }
}

/// Each app's launch options in Steam, by its number, from every user's settings.
pub fn launch_options(root: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for f in local_files(root) {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        let Ok(n) = vdf::parse(&text) else { continue };
        if let Some(Node::Keys(apps)) = n.at(&APPS) {
            for (app, v) in apps {
                if let Some(lo) = v.get("LaunchOptions").and_then(Node::words).filter(|l| !l.trim().is_empty()) {
                    out.insert(app.clone(), lo.to_string());
                }
            }
        }
    }
    out
}

/// Whether Steam is running: its client's own process, the 64-bit one
/// (steamrt64/steam) or the 32-bit one it replaced (ubuntu12_32/steam).
pub fn steam_running() -> bool {
    std::fs::read_dir("/proc").into_iter().flatten().flatten().any(|e| {
        e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) && std::fs::read_link(e.path().join("exe")).is_ok_and(|p| p.ends_with("steamrt64/steam") || p.ends_with("ubuntu12_32/steam"))
    })
}

/// Steam asked to close, and waited for, up to a minute.
pub fn close_steam() -> Result<(), String> {
    if !steam_running() {
        return Ok(());
    }
    std::process::Command::new("steam").arg("-shutdown").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().map_err(|e| format!("Ectomatter couldn’t ask Steam to close: {e}"))?;
    let t0 = Instant::now();
    while steam_running() {
        if t0.elapsed() > Duration::from_secs(60) {
            return Err("Steam didn’t close within a minute.".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    // (its helpers a moment after it)
    std::thread::sleep(Duration::from_secs(1));
    Ok(())
}

/// How the desktop's menu starts Steam: its entry's command (the person's
/// own entry, else the system's), its field codes (%U and the like) taken
/// out, as Grant's starts it with `-pipewire`; else plain `steam`.
fn steam_command() -> Vec<String> {
    let data = std::env::var("XDG_DATA_HOME").ok().filter(|d| !d.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share"));
    [data.join("applications/steam.desktop"), PathBuf::from("/usr/share/applications/steam.desktop")]
        .iter()
        .find_map(|f| exec_of(&std::fs::read_to_string(f).ok()?))
        .unwrap_or_else(|| vec!["steam".into()])
}

/// A desktop entry's own command: the Exec of its [Desktop Entry] (its
/// actions' come after), split as a shell splits it, field codes left out.
fn exec_of(text: &str) -> Option<Vec<String>> {
    let mut entry = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            entry = line == "[Desktop Entry]";
        } else if let Some(cmd) = line.strip_prefix("Exec=").filter(|_| entry) {
            let w: Vec<String> = words(cmd).into_iter().filter(|w| !(w.len() == 2 && w.starts_with('%'))).collect();
            return (!w.is_empty()).then_some(w);
        }
    }
    None
}

/// Whether any of Steam's own processes is left from one that closed: its
/// client, its web helper, or its steam.sh.
fn steam_left() -> bool {
    std::fs::read_dir("/proc").into_iter().flatten().flatten().any(|e| {
        if !e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        let exe = std::fs::read_link(e.path().join("exe")).unwrap_or_default();
        let cmd = std::fs::read(e.path().join("cmdline")).unwrap_or_default();
        exe.ends_with("steamrt64/steam") || exe.ends_with("ubuntu12_32/steam") || exe.file_name().is_some_and(|n| n == "steamwebhelper") || cmd.windows(9).any(|w| w == b"/steam.sh")
    })
}

/// Steam opened again, as it was before it was asked to close, once every
/// process of the one that closed is gone (a Steam started while the last
/// one is still going hands itself to it, and goes with it); asked twice if
/// it isn't up after half a minute. Whether it's up. (Its words go to
/// launch/steam.log in Ectomatter's data folder.)
pub fn open_steam() -> bool {
    let t0 = Instant::now();
    while steam_left() && t0.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(250));
    }
    let _ = std::fs::create_dir_all(crate::data_dir().join("launch"));
    for _ in 0..2 {
        let log = std::fs::File::create(crate::data_dir().join("launch/steam.log")).ok();
        let (out, err) = match log.as_ref().and_then(|f| Some((f.try_clone().ok()?, f.try_clone().ok()?))) {
            Some((o, e)) => (std::process::Stdio::from(o), std::process::Stdio::from(e)),
            None => (std::process::Stdio::null(), std::process::Stdio::null()),
        };
        // (without the marker of a terminal session Ectomatter was started from, which the
        // session's windows follow to its desktop: Steam's windows are Steam's own)
        let _ = std::process::Command::new("setsid").arg("-f").args(steam_command()).env_remove("CLAUDE_CODE_SESSION_ID").stdin(std::process::Stdio::null()).stdout(out).stderr(err).spawn();
        let t1 = Instant::now();
        while t1.elapsed() < Duration::from_secs(30) {
            if steam_running() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    false
}

/// A game's change in Steam's own settings: its Compatibility setting naming
/// one of Ectomatter's tools (TOOL, or TOOL_LINUX for its Linux version),
/// and its launch options: the line as it was read, and as it's to be (none:
/// taken out), changed only while it's still that line (one typed in Steam
/// before it closed is kept).
#[derive(Clone, Debug, PartialEq)]
pub struct Switch {
    pub app: String,
    pub through: Option<&'static str>,
    pub launch: Option<(String, Option<String>)>,
}

/// Steam's settings changed for each game, Steam closed: both files copied
/// aside first (into `backups`, a folder for this change, named for when),
/// each then written whole in place. Where the copies are.
pub fn change_steam(root: &Path, switches: &[Switch], backups: &Path) -> Result<PathBuf, String> {
    if steam_running() {
        return Err("Steam is running, and would write over the change as it quits.".into());
    }
    change_files(root, switches, backups)
}

/// The change itself, in the files of the Steam at `root`.
fn change_files(root: &Path, switches: &[Switch], backups: &Path) -> Result<PathBuf, String> {
    let aside = backups.join(stamp());
    std::fs::create_dir_all(&aside).map_err(|e| e.to_string())?;
    let config = config_file(root);
    if switches.iter().any(|s| s.through.is_some()) {
        let text = std::fs::read_to_string(&config).map_err(|e| format!("Steam’s settings couldn’t be read: {e}"))?;
        std::fs::write(aside.join("config.vdf"), &text).map_err(|e| e.to_string())?;
        let mut n = vdf::parse(&text)?;
        let map = n.group_at(&MAPPING);
        for s in switches {
            let Some(tool) = s.through else { continue };
            vdf::set(map, &s.app, Node::Keys(vec![("name".into(), Node::Words(tool.into())), ("config".into(), Node::Words(String::new())), ("priority".into(), Node::Words("250".into()))]));
        }
        replace(&config, &vdf::write(&n))?;
    }
    if switches.iter().any(|s| s.launch.is_some()) {
        for (k, f) in local_files(root).iter().enumerate() {
            let text = std::fs::read_to_string(f).map_err(|e| e.to_string())?;
            std::fs::write(aside.join(format!("localconfig-{k}.vdf")), &text).map_err(|e| e.to_string())?;
            let mut n = vdf::parse(&text)?;
            let mut changed = false;
            for s in switches {
                let Some((was, want)) = &s.launch else { continue };
                let now = n.at(&APPS).and_then(|a| a.get(&s.app)).and_then(|a| a.get("LaunchOptions")).and_then(Node::words).map(String::from);
                if now.as_deref() != Some(was.as_str()) {
                    continue;
                }
                let mut path: Vec<&str> = APPS.to_vec();
                path.push(&s.app);
                let app = n.group_at(&path);
                changed |= match want {
                    Some(l) => {
                        vdf::set(app, "LaunchOptions", Node::Words(l.clone()));
                        true
                    }
                    None => vdf::remove(app, "LaunchOptions"),
                };
            }
            if changed {
                replace(f, &vdf::write(&n))?;
            }
        }
    }
    Ok(aside)
}

/// A file written whole beside itself, then moved over it.
fn replace(p: &Path, text: &str) -> Result<(), String> {
    let tmp = p.with_extension("vdf.ectomatter");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, p).map_err(|e| e.to_string())
}

/// Now, as a folder's name sorts by it (20261023-142501).
fn stamp() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let m = crate::lcd_page::Moment::at(t as libc::time_t);
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", m.year, m.month + 1, m.day, m.hour, m.minute, t % 60)
}

// ---------------------------------------------------------------- the tool in Steam

const COMPAT: &str = "\"compatibilitytools\"\n{\n  \"compat_tools\"\n  {\n    \"ectomatter\" // Ectomatter: each game started as its page in Ectomatter's Library sets it up\n    {\n      \"install_path\" \".\"\n      \"display_name\" \"Ectomatter\"\n      \"from_oslist\"  \"windows\"\n      \"to_oslist\"    \"linux\"\n    }\n    \"ectomatter-linux\" // the same, for a game's Linux version\n    {\n      \"install_path\" \".\"\n      \"display_name\" \"Ectomatter (Linux version)\"\n      \"from_oslist\"  \"linux\"\n      \"to_oslist\"    \"linux\"\n    }\n  }\n}\n";
const MANIFEST: &str = "\"manifest\"\n{\n  \"version\" \"2\"\n  \"commandline\" \"/ectomatter-launch %verb%\"\n  \"use_tool_subprocess_reaper\" \"1\"\n}\n";

/// Ectomatter's launcher put in Steam's compatibilitytools.d, as a Proton
/// build is: its two files and the program, from beside Ectomatter's own.
/// Whether anything changed (Steam sees a new tool once it starts again).
pub fn install(root: &Path) -> Result<bool, String> {
    let program = std::env::current_exe().map_err(|e| e.to_string())?.with_file_name("ectomatter-launch");
    let bytes = std::fs::read(&program).map_err(|_| format!("Ectomatter’s launcher isn’t built beside it ({}).", program.display()))?;
    let dir = root.join("compatibilitytools.d").join(TOOL);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut changed = false;
    for (name, content) in [("compatibilitytool.vdf", COMPAT.as_bytes()), ("toolmanifest.vdf", MANIFEST.as_bytes()), ("ectomatter-launch", bytes.as_slice())] {
        let p = dir.join(name);
        if std::fs::read(&p).ok().as_deref() != Some(content) {
            let tmp = dir.join(format!(".{name}.{}.new", std::process::id()));
            std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
            if name == "ectomatter-launch" {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
            changed = true;
        }
    }
    Ok(changed)
}

// ---------------------------------------------------------------- recipes

/// A game's recipe written where the launcher reads it (launch/<app>.json;
/// "default" for Every game's), when it's changed.
pub fn write_recipe(name: &str, recipe: &Value) -> Result<(), String> {
    let dir = crate::data_dir().join("launch");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join(format!("{name}.json"));
    let text = serde_json::to_string_pretty(recipe).map_err(|e| e.to_string())?;
    if std::fs::read_to_string(&p).ok().as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    // (written whole beside it, then moved over it: a game starting never reads half a recipe)
    let tmp = dir.join(format!(".{name}.{}.json", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- the games' starts

/// A game starting through the launcher: the Library's id for it, its name,
/// Steam's reaper for it (the process the Library's watch will know it by),
/// and where to say its changes are made.
pub struct Start {
    pub id: String,
    pub name: String,
    pub pid: u32,
    pub done: std::sync::mpsc::Sender<()>,
}

/// The socket the launcher asks on.
fn socket() -> PathBuf {
    crate::lcd_player::runtime_dir().join("ectomatter/start.sock")
}

/// Listening for games starting, on a thread of its own: each told to the
/// window's loop, and the launcher answered once the game's changes are
/// made, or after half a minute.
pub fn listen(tell: smithay_client_toolkit::reexports::calloop::channel::Sender<Start>) {
    listen_at(&socket(), tell);
}

/// The same, on the socket at `path` (a test's own).
fn listen_at(path: &Path, tell: smithay_client_toolkit::reexports::calloop::channel::Sender<Start>) {
    let path = path.to_path_buf();
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::remove_file(&path);
    let listener = match std::os::unix::net::UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ectomatter: games started through Ectomatter won’t wait for their changes: {e}");
            return;
        }
    };
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let tell = tell.clone();
            std::thread::spawn(move || {
                let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
                let mut line = String::new();
                if BufReader::new(&conn).read_line(&mut line).is_err() {
                    return;
                }
                let Ok(v) = serde_json::from_str::<Value>(&line) else { return };
                let (tx, rx) = std::sync::mpsc::channel();
                let app = v["app"].as_str().unwrap_or("0");
                let start = Start { id: crate::games::steam_id(app.parse().unwrap_or(0)), name: v["name"].as_str().unwrap_or("").to_string(), pid: v["pid"].as_u64().unwrap_or(0) as u32, done: tx };
                if tell.send(start).is_ok() {
                    let _ = rx.recv_timeout(HOLD);
                }
                let mut conn = conn;
                let _ = conn.write_all(b"go\n");
            });
        }
    });
}

/// Steam's reaper for a game, above a process of the game's start (the
/// launcher's parent): the process the Library's watch knows the game by.
pub fn reaper_above(pid: u32) -> Option<u32> {
    let mut at = pid;
    for _ in 0..16 {
        if std::fs::read_to_string(format!("/proc/{at}/comm")).is_ok_and(|c| c.trim_end() == "reaper") && std::fs::read(format!("/proc/{at}/cmdline")).ok().and_then(|c| crate::steam::app_of_reaper(&c)).is_some() {
            return Some(at);
        }
        at = crate::steam::parent_in(&std::fs::read_to_string(format!("/proc/{at}/stat")).ok()?)?;
        if at <= 1 {
            return None;
        }
    }
    None
}

// ---------------------------------------------------------------- what's known, read on a thread

/// What Ectomatter knows of this computer for games' starts: Steam's folder,
/// the Proton builds installed, Steam's own default build (its Compatibility
/// setting for every other game), each game's Compatibility setting, the main
/// monitor (its size, refresh rate in hertz, and whether HDR is on), the
/// cores with the extra cache, and each prefix asked about (by game: its
/// size, whether it's made, how many backups it has).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Known {
    pub root: PathBuf,
    pub builds: Vec<Build>,
    pub default: String,
    pub mapping: HashMap<String, String>,
    pub main: Value,
    pub cores: Option<String>,
    pub prefixes: HashMap<u32, (u64, bool, usize)>,
}

impl Known {
    /// Whether a game starts through Ectomatter, by Steam's Compatibility setting.
    pub fn through(&self, app: u32) -> bool {
        self.mapping.get(&app.to_string()).is_some_and(|t| is_ours(t))
    }

    /// Whether a game runs its Linux version: it has one Steam starts
    /// (`linux`, steam.rs), and its Compatibility setting names no Proton
    /// build (none, a Steam Linux Runtime, or Ectomatter's for Linux versions).
    pub fn native(&self, app: u32, linux: bool) -> bool {
        linux && self.mapping.get(&app.to_string()).is_none_or(|t| runs_linux(t))
    }

    /// The Proton build Steam starts a game with, by its Compatibility
    /// setting: its own (as named, installed or not), else Steam's default.
    pub fn steam_build(&self, app: u32) -> String {
        self.mapping.get(&app.to_string()).filter(|t| !is_ours(t) && !runs_linux(t)).cloned().unwrap_or_else(|| self.default.clone())
    }

    /// The Steam Linux Runtime a game's Linux version starts in: the one its
    /// Compatibility setting names, else Steam Play's (`valve`), where it's
    /// installed in a library.
    pub fn runtime_of(&self, app: u32, valve: Option<&str>) -> Option<PathBuf> {
        let named = self.mapping.get(&app.to_string()).filter(|t| t.to_ascii_lowercase().starts_with("steamlinuxruntime")).map(String::as_str).or(valve)?;
        // (by its folder, whatever case the setting spells it in: steamlinuxruntime_sniper
        // is SteamLinuxRuntime_sniper's)
        libraries(&self.root)
            .into_iter()
            .flat_map(|l| std::fs::read_dir(l.join("steamapps/common")).into_iter().flatten().flatten().map(|e| e.path()))
            .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(named)) && p.join("_v2-entry-point").is_file())
    }

    /// What the Game Configurator is told of this computer (its `facts`,
    /// before a game's own are added).
    pub fn facts(&self) -> Value {
        json!({
            "default": self.default,
            "home": std::env::var("HOME").unwrap_or_default(),
            "protons": self.builds.iter().map(|b| json!({ "id": b.id, "name": b.name, "path": b.path, "runtime": b.runtime, "family": b.family })).collect::<Vec<_>>(),
            "main": self.main,
            "cores": self.cores,
        })
    }
}

/// The main monitor as a game's start needs it: its size, its refresh rate
/// in hertz, and whether HDR is on (from ECTOMATTER_MONITORS's file
/// offscreen, as the plugins' monitors are).
pub fn main_monitor() -> Value {
    let list: Vec<crate::monitors::Monitor> = match std::env::var_os("ECTOMATTER_MONITORS") {
        Some(f) => {
            let v: Value = std::fs::read_to_string(&f).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
            let main = v.as_array().into_iter().flatten().find(|m| m["main"] == true).cloned().unwrap_or(Value::Null);
            let mode = crate::monitors::Mode::of(&main["mode"]);
            return match mode {
                Some(m) => json!({ "width": m.width, "height": m.height, "refresh": (m.refresh + 500) / 1000, "hdr": main["hdr"] == true }),
                None => json!({}),
            };
        }
        None => crate::monitors::list().unwrap_or_default(),
    };
    match crate::monitors::main_of(&list).and_then(|m| m.mode.map(|mode| (mode, m.hdr))) {
        Some((m, hdr)) => json!({ "width": m.width, "height": m.height, "refresh": (m.refresh + 500) / 1000, "hdr": hdr == Some(true) }),
        None => json!({}),
    }
}

/// What's known, read now, of the Steam at `root`, with the prefixes asked
/// about (each game's Steam number and its library).
pub fn know(root: &Path, prefixes: &[(u32, PathBuf)]) -> Known {
    let mapping = mapping(root);
    let mut k = Known { builds: builds(root), default: mapping.get("0").cloned().unwrap_or_default(), mapping, main: main_monitor(), cores: cache_cores(), root: root.to_path_buf(), prefixes: HashMap::new() };
    for (app, lib) in prefixes {
        let p = prefix_of(lib, *app);
        let made = p.join("pfx").is_dir();
        k.prefixes.insert(*app, (if made { size_of(&p) } else { 0 }, made, backups(lib, *app).len()));
    }
    k
}

/// The thread that reads what's known, as it's asked (a prefix's size can
/// take a moment to add up).
pub struct Reader {
    ask: std::sync::mpsc::Sender<(PathBuf, Vec<(u32, PathBuf)>)>,
    pub known: std::sync::mpsc::Receiver<Known>,
}

impl Reader {
    pub fn start() -> Reader {
        let (ask, asked) = std::sync::mpsc::channel::<(PathBuf, Vec<(u32, PathBuf)>)>();
        let (tx, known) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new().name("launch-facts".into()).spawn(move || {
            while let Ok(mut a) = asked.recv() {
                // (asks piled up are one read, the latest's)
                while let Ok(more) = asked.try_recv() {
                    a = more;
                }
                if tx.send(know(&a.0, &a.1)).is_err() {
                    break;
                }
            }
        });
        Reader { ask, known }
    }

    pub fn ask(&self, root: PathBuf, prefixes: Vec<(u32, PathBuf)>) {
        let _ = self.ask.send((root, prefixes));
    }
}

// ---------------------------------------------------------------- the prefix

/// A game's prefix: Steam's compatdata folder for it, in its library.
pub fn prefix_of(library: &Path, app: u32) -> PathBuf {
    library.join("steamapps/compatdata").join(app.to_string())
}

/// How much a folder holds, its links not followed.
pub fn size_of(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => size_of(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

/// Where a game's prefix is backed up: beside Steam's compatdata, on the
/// same drive, so a copy is quick (on btrfs, a copy that shares the files).
fn backups_dir(library: &Path, app: u32) -> PathBuf {
    library.join("steamapps/ectomatter-backups").join(app.to_string())
}

/// A game's backups so far, the newest last.
pub fn backups(library: &Path, app: u32) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(backups_dir(library, app)).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
    out.sort();
    out
}

/// A game's prefix copied aside, whole: where the copy is.
pub fn back_up(library: &Path, app: u32) -> Result<PathBuf, String> {
    let from = prefix_of(library, app);
    if !from.is_dir() {
        return Err("The game has no prefix yet.".into());
    }
    // (the folder itself, where Steam's compatdata holds only a link to it on another drive)
    let from = std::fs::canonicalize(&from).unwrap_or(from);
    let dir = backups_dir(library, app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // (a folder of its own: two backups in the same second never share one)
    let at = stamp();
    let to = (1..).map(|k| if k == 1 { dir.join(&at) } else { dir.join(format!("{at}-{k}")) }).find(|p| !p.exists()).expect("a free name");
    let out = std::process::Command::new("cp").args(["-a", "--reflink=auto"]).arg(&from).arg(&to).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&to);
        return Err(format!("The copy stopped: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(to)
}

/// A game's prefix backed up, then taken away, for Proton to make afresh
/// as the game next starts: where the backup is.
pub fn reset(library: &Path, app: u32) -> Result<PathBuf, String> {
    let kept = back_up(library, app)?;
    std::fs::remove_dir_all(prefix_of(library, app)).map_err(|e| format!("Backed up to {}, but the prefix couldn’t be removed: {e}", kept.display()))?;
    Ok(kept)
}

/// A program run inside a game's prefix with the game's Proton build: Wine's
/// configuration, Winetricks, or a program the person chose.
pub fn in_prefix(root: &Path, library: &Path, app: u32, proton: &Path, what: &[String]) -> std::process::Command {
    let compat = prefix_of(library, app);
    let mut c;
    if what.first().is_some_and(|w| w == "winetricks") {
        // (Winetricks with the build's own Wine, in the prefix's Windows files)
        c = std::process::Command::new("winetricks");
        c.args(&what[1..]);
        c.env("WINEPREFIX", compat.join("pfx"));
        for (var, bin) in [("WINE", "wine"), ("WINESERVER", "wineserver")] {
            let p = proton.join("files/bin").join(bin);
            if p.is_file() {
                c.env(var, p);
            }
        }
    } else {
        c = std::process::Command::new(proton.join("proton"));
        c.arg("run").args(what);
        c.env("STEAM_COMPAT_DATA_PATH", &compat);
        c.env("STEAM_COMPAT_CLIENT_INSTALL_PATH", root);
        c.env("SteamAppId", app.to_string());
        c.env("SteamGameId", app.to_string());
    }
    c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    c
}

/// Whether a program is on the PATH.
pub fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}

/// A path as people read it, the home folder as ~.
pub fn shown(p: &Path) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let s = p.display().to_string();
    if !home.is_empty() && s.starts_with(&home) { format!("~{}", &s[home.len()..]) } else { s }
}

// ---------------------------------------------------------------- the cores with the extra cache

/// The CPUs sharing the largest cache (the V-Cache cores on an X3D part, which
/// GameMode keeps a game on), as Linux lists them ("0-7,16-23").
pub fn cache_cores() -> Option<String> {
    let mut best: Option<(u64, String)> = None;
    for e in std::fs::read_dir("/sys/devices/system/cpu").ok()?.flatten() {
        let d = e.path().join("cache/index3");
        let (Ok(size), Ok(list)) = (std::fs::read_to_string(d.join("size")), std::fs::read_to_string(d.join("shared_cpu_list"))) else { continue };
        let Ok(kb) = size.trim().trim_end_matches('K').parse::<u64>() else { continue };
        if best.as_ref().is_none_or(|(b, _)| kb > *b) {
            best = Some((kb, list.trim().to_string()));
        }
    }
    best.map(|(_, l)| l)
}

// ---------------------------------------------------------------- game-helper's games, moved over

/// What a game's launch line through game-helper becomes in Ectomatter: its
/// Game Configurator values, the devices' profiles it switched (by the
/// Library's device keys), and what's left of the line for Steam (none,
/// once all of it is Ectomatter's).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Moved {
    pub values: Vec<(String, Value)>,
    pub profiles: Vec<(String, String)>,
    pub left: Option<String>,
}

/// Games set to start through Ectomatter, carried over first: for each Steam
/// number in `apps` (with whether Steam starts its Linux version), the
/// Proton build Steam started it with, as named, where Every game's is
/// another; and its launch line through game-helper, as the Game
/// Configurator's values (`plugin`'s) and its devices' profiles, where the
/// game has none of its own, the line then taken out. An app (`switch`
/// false, as SteamVR) only has its line taken out. The changes to make in
/// Steam's own settings.
pub fn carry_over(lib: &mut crate::games::Library, k: &Known, apps: &[(u32, bool, bool)], lines: &HashMap<String, String>, mouse: Option<&str>, plugin: &str) -> Vec<Switch> {
    let every_build = lib.plugin_values(crate::games::EVERY, plugin)["version"].as_str().map_or_else(|| k.default.clone(), String::from);
    let mut out = Vec::new();
    for &(app, native, switch) in apps {
        let id = crate::games::steam_id(app);
        let old = k.steam_build(app);
        if switch && !native && old != every_build && !old.is_empty() && lib.plugin_values(&id, plugin).get("version").is_none() {
            lib.set_plugin_value(&id, plugin, "version", Some(json!(old)));
        }
        let line = lines.get(&app.to_string());
        let launch = match line.and_then(|l| from_game_helper(l, "2", mouse)) {
            Some(m) => {
                for (key, v) in m.values {
                    if lib.plugin_values(&id, plugin).get(&key).is_none() {
                        lib.set_plugin_value(&id, plugin, &key, Some(v));
                    }
                }
                for (device, p) in m.profiles {
                    if lib.choice(&id, &device).is_none() {
                        lib.set_choice(&id, &device, Some(crate::games::Choice::Profile(p)));
                    }
                }
                line.map(|l| (l.clone(), m.left))
            }
            None => None,
        };
        let through = switch.then_some(if native { TOOL_LINUX } else { TOOL });
        if through.is_some() || launch.is_some() {
            out.push(Switch { app: app.to_string(), through, launch });
        }
    }
    out
}

/// The mouse's profile game-helper's -m switched to (its "game" profile):
/// the one named Gaming, or Game.
pub fn game_profile(store: &crate::profiles::Store) -> Option<String> {
    store.list.iter().find(|p| p.name.eq_ignore_ascii_case("gaming") || p.name.eq_ignore_ascii_case("game")).map(|p| p.id.clone())
}

/// Every game whose launch line in Steam runs game-helper, set to start
/// through Ectomatter with its flags carried over (carry_over), Steam closed
/// for it and opened again if it was open, as `ectomatter --move-game-helper`
/// asks: what was done, in a sentence. Refused while a Steam game runs, and
/// while another Ectomatter runs, which would write its own Library over this one's.
pub fn move_game_helper() -> Result<String, String> {
    let me = std::process::id();
    let other = std::fs::read_dir("/proc").into_iter().flatten().flatten().any(|e| {
        e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()).is_some_and(|pid| pid != me) && std::fs::read_link(e.path().join("exe")).is_ok_and(|p| p.file_name().is_some_and(|n| n == "ectomatter"))
    });
    if other {
        return Err("Quit Ectomatter first: its window would write its own Library over this change.".into());
    }
    if !crate::steam::running().is_empty() {
        return Err("Quit the game first: Steam has to close for this change.".into());
    }
    let steam = crate::steam::read().ok_or("Steam isn’t on this computer.")?;
    let root = steam.root.clone();
    let lines = launch_options(&root);
    let k = know(&root, &[]);
    let mut lib = crate::games::Library::load();
    let mouse = crate::profiles::path_for("sensei310").and_then(crate::profiles::Store::read_only).as_ref().and_then(game_profile);
    let mut apps: Vec<(u32, bool, bool)> = Vec::new();
    for (app, line) in &lines {
        let (Ok(n), Some(_)) = (app.parse::<u32>(), from_game_helper(line, "2", None)) else { continue };
        let known = steam.apps.iter().find(|a| a.id == n);
        // (an app, as SteamVR is, keeps how Steam starts it; a game not installed is a Windows one)
        let switch = known.is_none_or(|a| a.kind == crate::steam::Kind::Game);
        apps.push((n, known.is_some_and(|a| k.native(n, a.linux)), switch));
    }
    if apps.is_empty() {
        return Ok("No launch line in Steam runs game-helper.".into());
    }
    apps.sort();
    let switches = carry_over(&mut lib, &k, &apps, &lines, mouse.as_deref(), "game-configurator");
    install(&root)?;
    let was_open = steam_running();
    close_steam()?;
    let aside = change_steam(&root, &switches, &crate::data_dir().join("steam-backups"));
    let reopened = !was_open || open_steam();
    let aside = aside?;
    let games = switches.iter().filter(|s| s.through.is_some()).count();
    let mut said = format!("{games} games start through Ectomatter now, and game-helper is out of {} launch lines. Steam’s settings were copied to {} first.", switches.iter().filter(|s| s.launch.is_some()).count(), shown(&aside));
    if !reopened {
        said.push_str(" Steam didn’t open again: open it from your apps.");
    }
    Ok(said)
}

/// Words split as a shell splits them, each quote taken out.
fn words(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut quote, mut any) = (Vec::new(), String::new(), None::<char>, false);
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                any = true;
            }
            (None, c) if c.is_whitespace() => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            (None, c) => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// An environment variable as a launch line sets it (NAME=value), if the
/// word is one.
fn assignment(w: &str) -> Option<(String, String)> {
    let (k, v) = w.split_once('=')?;
    (!k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !k.starts_with(|c: char| c.is_ascii_digit())).then(|| (k.to_string(), v.to_string()))
}

/// A word as a shell would need it written.
fn shell_word(w: &str) -> String {
    if !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,+%@".contains(c)) { w.to_string() } else { format!("\"{}\"", w.replace('"', "\\\"")) }
}

/// A launch line through game-helper, carried over: -r as Proton's HDR, -g
/// as gamescope's, -c as the pad straight to the game, -x as GE's Xbox copy,
/// --orgb as OpenRGB's profile for the game, -k and -m as the keyboard's and
/// the mouse's (the profiles `keyboard` and `mouse` name, where given); the
/// line's own variables and the game's arguments as the Game Configurator's,
/// the upgrades and DLL overrides among them. -b and -v are left out
/// (Ectomatter's Monitors does them). None for a line without game-helper.
pub fn from_game_helper(line: &str, keyboard: &str, mouse: Option<&str>) -> Option<Moved> {
    let w = words(line);
    let at = w.iter().position(|x| x == "game-helper" || x.ends_with("/game-helper"))?;
    let mut m = Moved::default();
    let mut env: Vec<(String, String)> = w[..at].iter().filter_map(|x| assignment(x)).collect();
    let mut i = at + 1;
    let mut hdr: Option<&str> = None;
    // the flags, up to "--"
    while i < w.len() && w[i] != "--" {
        let has_value = |j: usize| w.get(j).is_some_and(|v| v != "--" && !v.starts_with('-'));
        match w[i].as_str() {
            "-r" | "--hdr" => hdr = hdr.or(Some("proton")),
            "-g" | "--gamescope" => hdr = Some("gamescope"),
            "-c" | "--native-controller" => m.values.push(("pad".into(), json!("native"))),
            "-x" | "--sony-xinput-backup" => m.values.push(("xbox".into(), json!(true))),
            "-k" | "--keyboard-profile" => {
                let p = if has_value(i + 1) { i += 1; w[i].clone() } else { keyboard.to_string() };
                m.profiles.push(("z10".into(), p));
            }
            "-m" | "--mouse-profile" => {
                if has_value(i + 1) {
                    i += 1;
                }
                if let Some(p) = mouse {
                    m.profiles.push(("sensei310".into(), p.to_string()));
                }
            }
            "-orgb" | "--orgb" => {
                let mut name = Vec::new();
                while has_value(i + 1) {
                    i += 1;
                    name.push(w[i].clone());
                }
                if !name.is_empty() {
                    m.profiles.push(("openrgb".into(), name.join(" ")));
                }
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(h) = hdr {
        m.values.push(("hdr".into(), json!(h)));
    }
    // after "--": variables set in front of the game's command, the command, the game's arguments
    let rest = w.get(i + 1..).unwrap_or(&[]);
    let cmd = rest.iter().position(|x| x == "%command%");
    let (before, after) = match cmd {
        Some(c) => (&rest[..c], &rest[c + 1..]),
        None => (rest, &[][..]),
    };
    env.extend(before.iter().filter_map(|x| assignment(x)));
    let mut kept = serde_json::Map::new();
    for (k, v) in env {
        match (k.as_str(), v.as_str()) {
            ("PROTON_DLSS_UPGRADE", "1") => m.values.push(("dlss".into(), json!(true))),
            ("PROTON_FSR4_UPGRADE", "1") => m.values.push(("fsr4".into(), json!(true))),
            ("PROTON_XESS_UPGRADE", "1") => m.values.push(("xess".into(), json!(true))),
            ("WINEDLLOVERRIDES", d) => m.values.push(("dll".into(), json!(d))),
            _ => {
                kept.insert(k, json!(v));
            }
        }
    }
    if !kept.is_empty() {
        m.values.push(("env".into(), Value::Object(kept)));
    }
    if !after.is_empty() {
        m.values.push(("args".into(), json!(after.iter().map(|a| shell_word(a)).collect::<Vec<_>>().join(" "))));
    }
    Some(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved(line: &str) -> Moved {
        from_game_helper(line, "2", Some("p65e5e7d567f370")).expect("a game-helper line")
    }

    fn value<'a>(m: &'a Moved, k: &str) -> Option<&'a Value> {
        m.values.iter().find(|(key, _)| key == k).map(|(_, v)| v)
    }

    /// Grant's launch lines, as Steam held them, each carried over whole.
    #[test]
    fn game_helpers_lines_are_carried_over() {
        let arc = moved("game-helper --orgb ARC Raiders -b --gamescope -m -k -- %command%");
        assert_eq!(value(&arc, "hdr"), Some(&json!("gamescope")));
        let p = |d: &str, v: &str| (d.to_string(), v.to_string());
        assert_eq!(arc.profiles, [p("openrgb", "ARC Raiders"), p("sensei310", "p65e5e7d567f370"), p("z10", "2")]);
        assert_eq!(arc.left, None);
        let bg3 = moved("WINEDLLOVERRIDES=DWrite.dll=n,b game-helper --hdr -b -- %command% --skip-launcher ");
        assert_eq!((value(&bg3, "hdr"), value(&bg3, "dll"), value(&bg3, "args")), (Some(&json!("proton")), Some(&json!("DWrite.dll=n,b")), Some(&json!("--skip-launcher"))));
        let hfw = moved("PROTON_DLSS_UPGRADE=1 game-helper -r -b -c -- %command% -nolauncher");
        assert_eq!((value(&hfw, "dlss"), value(&hfw, "pad"), value(&hfw, "args")), (Some(&json!(true)), Some(&json!("native")), Some(&json!("-nolauncher"))));
        let wukong = moved("PROTON_DLSS_UPGRADE=\"1\" game-helper -b  -r -- %command%");
        assert_eq!(value(&wukong, "dlss"), Some(&json!(true)));
        let outlaws = moved("game-helper -b -r -- PROTON_ENABLE_NVAPI=1 %command%     ");
        assert_eq!(value(&outlaws, "env"), Some(&json!({ "PROTON_ENABLE_NVAPI": "1" })));
        let gotg = moved("game-helper -r -b -c -- %command% -nosplash -nolauncher");
        assert_eq!(value(&gotg, "args"), Some(&json!("-nosplash -nolauncher")));
        let k3 = moved("game-helper -k 3 -- %command%");
        assert_eq!(k3.profiles, [("z10".to_string(), "3".to_string())]);
        let vr = moved("game-helper -v -- %command%");
        assert!(vr.values.is_empty() && vr.profiles.is_empty(), "SteamVR's -v is Monitors' now");
        assert!(from_game_helper("PROTON_SONY_AUTO_XINPUT=0 %command%", "2", None).is_none(), "a line without game-helper stays as it is");
    }

    /// game-helper's games carried over: ARC Raiders keeps its Hotfix build
    /// and gets gamescope's HDR and its profiles (its keyboard's choice kept,
    /// being its own already); SteamVR, an app, only loses its line.
    #[test]
    fn game_helpers_games_are_carried_over() {
        let mut lib = crate::games::Library::of(json!({ "choices": { "steam_app_1808500": { "z10": "1" } } }), None);
        let k = Known { default: "Proton-GE Latest".into(), mapping: HashMap::from([("1808500".to_string(), "proton_hotfix".to_string())]), ..Known::default() };
        let lines = HashMap::from([("1808500".to_string(), "game-helper --orgb ARC Raiders -b --gamescope -m -k -- %command%".to_string()), ("250820".to_string(), "game-helper -v -- %command%".to_string())]);
        let s = carry_over(&mut lib, &k, &[(1808500, false, true), (250820, false, false)], &lines, Some("p65e5e7d567f370"), "game-configurator");
        assert_eq!(s, [
            Switch { app: "1808500".into(), through: Some(TOOL), launch: Some(("game-helper --orgb ARC Raiders -b --gamescope -m -k -- %command%".into(), None)) },
            Switch { app: "250820".into(), through: None, launch: Some(("game-helper -v -- %command%".into(), None)) },
        ]);
        assert_eq!(lib.plugin_values("steam_app_1808500", "game-configurator"), json!({ "version": "proton_hotfix", "hdr": "gamescope" }));
        assert_eq!(lib.choice("steam_app_1808500", "openrgb"), Some(crate::games::Choice::Profile("ARC Raiders".into())));
        assert_eq!(lib.choice("steam_app_1808500", "sensei310"), Some(crate::games::Choice::Profile("p65e5e7d567f370".into())));
        assert_eq!(lib.choice("steam_app_1808500", "z10"), Some(crate::games::Choice::Profile("1".into())), "its own choice kept");
    }

    /// Steam opened again as the desktop's menu opens it, its own options kept.
    #[test]
    fn steam_starts_as_its_menu_starts_it() {
        let entry = "[Desktop Entry]\nName=Steam\nExec=/usr/bin/steam -pipewire %U\n\n[Desktop Action Store]\nExec=/usr/bin/steam steam://store\n";
        assert_eq!(exec_of(entry), Some(vec!["/usr/bin/steam".to_string(), "-pipewire".into()]));
        assert_eq!(exec_of("[Desktop Action Store]\nExec=/usr/bin/steam steam://store\n"), None, "an action's isn't the entry's own");
    }

    /// A game starting, as the launcher tells it: told to the window's loop by
    /// its Library id, and the launcher answered "go" only once its changes
    /// are said made.
    #[test]
    fn a_start_waits_for_its_changes() {
        use smithay_client_toolkit::reexports::calloop::{channel, EventLoop};
        let dir = std::env::temp_dir().join(format!("ectomatter-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("start.sock");
        let (tx, rx) = channel::channel::<Start>();
        listen_at(&path, tx);
        let p = path.clone();
        let launcher = std::thread::spawn(move || {
            let mut s = std::os::unix::net::UnixStream::connect(&p).unwrap();
            writeln!(s, "{}", json!({ "app": "1808500", "name": "ARC Raiders", "pid": 4242 })).unwrap();
            let t0 = Instant::now();
            let mut line = String::new();
            BufReader::new(s).read_line(&mut line).unwrap();
            (line, t0.elapsed())
        });
        let mut ev: EventLoop<Option<Start>> = EventLoop::try_new().unwrap();
        ev.handle().insert_source(rx, |e, _, got: &mut Option<Start>| {
            if let channel::Event::Msg(s) = e {
                *got = Some(s);
            }
        }).unwrap();
        let mut got = None;
        let t0 = Instant::now();
        while got.is_none() && t0.elapsed() < Duration::from_secs(10) {
            ev.dispatch(Some(Duration::from_millis(100)), &mut got).unwrap();
        }
        let start = got.expect("the start told");
        assert_eq!((start.id.as_str(), start.name.as_str(), start.pid), ("steam_app_1808500", "ARC Raiders", 4242));
        std::thread::sleep(Duration::from_millis(300));
        start.done.send(()).unwrap();
        let (line, waited) = launcher.join().unwrap();
        assert_eq!(line, "go\n");
        assert!(waited >= Duration::from_millis(300), "it waited for the changes: {waited:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Steam laid out with GE-Proton in compatibilitytools.d and Valve's
    /// Hotfix in a second library, each with its runtime: both found, by the
    /// ids Steam's Compatibility setting gives them; Ectomatter's own isn't one.
    #[test]
    fn the_builds_installed_are_found() {
        let s = std::env::temp_dir().join(format!("ectomatter-builds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&s);
        let (root, lib) = (s.join("Steam"), s.join("Games"));
        let put = |p: PathBuf, t: &str| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, t).unwrap();
        };
        put(root.join("steamapps/libraryfolders.vdf"), &format!("\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}", root.display(), lib.display()));
        put(root.join("compatibilitytools.d/Proton-GE Latest/proton"), "");
        put(root.join("compatibilitytools.d/Proton-GE Latest/compatibilitytool.vdf"), "\"compatibilitytools\" { \"compat_tools\" { \"Proton-GE Latest\" // its name\n { \"install_path\" \".\" \"display_name\" \"Proton-GE Latest\" } } }");
        put(root.join("compatibilitytools.d/Proton-GE Latest/toolmanifest.vdf"), "\"manifest\" { \"require_tool_appid\" \"4183110\" }");
        put(root.join("compatibilitytools.d/ectomatter/proton"), "");
        put(lib.join("steamapps/appmanifest_2180100.acf"), "\"AppState\" { \"installdir\" \"Proton Hotfix\" }");
        put(lib.join("steamapps/common/Proton Hotfix/proton"), "");
        put(lib.join("steamapps/common/Proton Hotfix/toolmanifest.vdf"), "\"manifest\" { \"require_tool_appid\" \"4183110\" }");
        put(root.join("steamapps/appmanifest_4183110.acf"), "\"AppState\" { \"installdir\" \"SteamLinuxRuntime_4\" }");
        put(root.join("steamapps/common/SteamLinuxRuntime_4/_v2-entry-point"), "");
        let b = builds(&root);
        let ids: Vec<(&str, &str)> = b.iter().map(|b| (b.id.as_str(), b.family)).collect();
        assert_eq!(ids, [("Proton-GE Latest", "ge"), ("proton_hotfix", "valve")]);
        assert!(b.iter().all(|b| b.runtime.as_deref() == Some(root.join("steamapps/common/SteamLinuxRuntime_4").as_path())));
        let _ = std::fs::remove_dir_all(&s);
    }

    /// Steam's settings changed (as they are with Steam closed; the test's
    /// Steam is only files): the games set to start through Ectomatter, their game-helper lines taken out, the other
    /// games as they were, and both files copied aside first.
    #[test]
    fn steams_settings_change_for_the_games_moved() {
        let s = std::env::temp_dir().join(format!("ectomatter-steam-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&s);
        let root = s.join("Steam");
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::create_dir_all(root.join("userdata/10122646/config")).unwrap();
        std::fs::write(root.join("config/config.vdf"), "\"InstallConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"CompatToolMapping\"\n\t\t\t\t{\n\t\t\t\t\t\"1808500\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"name\"\t\t\"proton_hotfix\"\n\t\t\t\t\t\t\"config\"\t\t\"\"\n\t\t\t\t\t\t\"priority\"\t\t\"250\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n").unwrap();
        std::fs::write(root.join("userdata/10122646/config/localconfig.vdf"), "\"UserLocalConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"apps\"\n\t\t\t\t{\n\t\t\t\t\t\"1808500\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"Playtime\"\t\t\"21842\"\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"game-helper --orgb ARC Raiders -b --gamescope -m -k -- %command%\"\n\t\t\t\t\t}\n\t\t\t\t\t\"418530\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"PROTON_SONY_AUTO_XINPUT=0 %command%\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n").unwrap();
        // (the copies in the test's own folder, so Grant's data is never touched)
        let gh = "game-helper --orgb ARC Raiders -b --gamescope -m -k -- %command%".to_string();
        let typed = Switch { app: "418530".into(), through: None, launch: Some(("game-helper -- %command%".into(), None)) };
        let aside = change_files(&root, &[Switch { app: "1808500".into(), through: Some(TOOL), launch: Some((gh, None)) }, typed], &s.join("backups")).unwrap();
        assert!(aside.starts_with(s.join("backups")));
        assert_eq!(mapping(&root).get("1808500").map(String::as_str), Some(TOOL));
        let lo = launch_options(&root);
        assert!(lo.get("1808500").is_none(), "game-helper's line taken out");
        assert_eq!(lo.get("418530").map(String::as_str), Some("PROTON_SONY_AUTO_XINPUT=0 %command%"), "a line no longer the one read is kept");
        assert!(std::fs::read_to_string(aside.join("config.vdf")).unwrap().contains("proton_hotfix"), "copied aside first");
        assert!(std::fs::read_to_string(root.join("userdata/10122646/config/localconfig.vdf")).unwrap().contains("\"Playtime\"\t\t\"21842\""));
        let _ = std::fs::remove_dir_all(&s);
    }
}
