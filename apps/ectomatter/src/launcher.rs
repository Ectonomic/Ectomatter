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
/// runs in, and whether Steam installs it itself, as one of its apps
/// (Valve's builds), rather than it sitting in compatibilitytools.d.
#[derive(Clone, Debug, PartialEq)]
pub struct Build {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub runtime: Option<PathBuf>,
    pub from_steam: bool,
}

/// The Proton builds Steam installs as its apps, by their Steam numbers:
/// each one's id in Steam's Compatibility setting, and its name, newest
/// first. One Steam has since added is found all the same (steam_builds).
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

/// The id Steam's Compatibility setting gives a build of its own named as
/// its folder is ("Proton 11.0" is proton_11, "Proton 6.3" proton_63), for
/// one newer than VALVE.
fn steam_id_of(folder: &str) -> String {
    let rest = folder.to_lowercase();
    let rest = rest.trim_start_matches("proton").trim().trim_start_matches('-').trim();
    let rest = rest.strip_suffix(".0").unwrap_or(rest);
    format!("proton_{}", rest.replace(['.', ' '], "").replace('-', "_"))
}

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
        let runtime = runtime_for(&d, &libs);
        out.push(Build { id, name, path: d, runtime, from_steam: false });
    }
    out.extend(steam_builds(&libs));
    out
}

/// The Proton builds Steam installs as its apps: VALVE's, newest first, after
/// any Steam has added since (an app whose folder holds a `proton` and a
/// `toolmanifest.vdf`).
fn steam_builds(libs: &[PathBuf]) -> Vec<Build> {
    let mut known = Vec::new();
    let mut newer = Vec::new();
    for lib in libs {
        let apps = std::fs::read_dir(lib.join("steamapps")).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>()).unwrap_or_default();
        for f in apps {
            let Some(app) = f.strip_prefix("appmanifest_").and_then(|a| a.strip_suffix(".acf")) else { continue };
            let Some(d) = install_dir(std::slice::from_ref(lib), app).filter(|d| d.join("proton").is_file() && d.join("toolmanifest.vdf").is_file()) else { continue };
            let runtime = runtime_for(&d, libs);
            match VALVE.iter().position(|(a, _, _)| *a == app) {
                Some(k) => known.push((k, Build { id: VALVE[k].1.into(), name: VALVE[k].2.into(), path: d, runtime, from_steam: true })),
                None => {
                    let folder = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    newer.push(Build { id: steam_id_of(&folder), name: folder, path: d, runtime, from_steam: true });
                }
            }
        }
    }
    known.sort_by_key(|(k, _)| *k);
    newer.sort_by(|a, b| b.name.cmp(&a.name));
    newer.into_iter().chain(known.into_iter().map(|(_, b)| b)).collect()
}

// ---------------------------------------------------------------- Steam's own settings

fn config_file(root: &Path) -> PathBuf {
    root.join("config/config.vdf")
}

const MAPPING: [&str; 5] = ["InstallConfigStore", "Software", "Valve", "Steam", "CompatToolMapping"];

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

/// Whether Steam is running: its client's own process, the 64-bit one
/// (steamrt64/steam) or the 32-bit one it replaced (ubuntu12_32/steam).
pub fn steam_running() -> bool {
    std::fs::read_dir(crate::computer::here("/proc")).into_iter().flatten().flatten().any(|e| {
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
    std::fs::read_dir(crate::computer::here("/proc")).into_iter().flatten().flatten().any(|e| {
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
/// one of Ectomatter's tools (TOOL, or TOOL_LINUX for its Linux version).
#[derive(Clone, Debug, PartialEq)]
pub struct Switch {
    pub app: String,
    pub through: &'static str,
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
    if !switches.is_empty() {
        let text = std::fs::read_to_string(&config).map_err(|e| format!("Steam’s settings couldn’t be read: {e}"))?;
        std::fs::write(aside.join("config.vdf"), &text).map_err(|e| e.to_string())?;
        let mut n = vdf::parse(&text)?;
        let map = n.group_at(&MAPPING);
        for s in switches {
            vdf::set(map, &s.app, Node::Keys(vec![("name".into(), Node::Words(s.through.into())), ("config".into(), Node::Words(String::new())), ("priority".into(), Node::Words("250".into()))]));
        }
        replace(&config, &vdf::write(&n))?;
    }
    Ok(aside)
}

/// A file written whole beside itself, then moved over it.
fn replace(p: &Path, text: &str) -> Result<(), String> {
    crate::files::write_whole(p, text.as_bytes()).map_err(|e| e.to_string())
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
            #[cfg(unix)]
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

/// How a value in a game's start may be written, as the plugin's manifest
/// names it (docs/core-protocol.md, section 11, A game's start).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Written {
    /// `"number"`: a plain number or switch (`1`, `120`, `1.5`)
    Number,
    /// `"word"`: one word (`fsr`, `top-left`)
    Word,
    /// `"list"`: items and their settings (`dwrite=n,b;d3d9=n`, `8:0,1,2,3`,
    /// `0xbeef/0xdead`), none naming a command, a file, a folder or a log
    List,
}

impl Written {
    const ALL: [Written; 3] = [Written::Number, Written::Word, Written::List];

    fn of(v: &Value) -> Option<Written> {
        Written::ALL.into_iter().find(|w| v.as_str() == Some(w.name()))
    }

    pub fn name(self) -> &'static str {
        match self {
            Written::Number => "number",
            Written::Word => "word",
            Written::List => "list",
        }
    }

    /// The ways of writing this one allows, from the narrowest up (a list
    /// takes any word, a word any number).
    pub fn within(self) -> &'static [Written] {
        let at = Written::ALL.iter().position(|w| *w == self).unwrap_or(0);
        &Written::ALL[..=at]
    }
}

/// A program a plugin may wrap a game in: its name, the options it may be
/// given (each with how its value is written; none, an option that takes
/// none) and what comes last, before the game (gamescope's `--`).
#[derive(Clone, Debug, PartialEq)]
pub struct Wrapper {
    pub program: String,
    pub options: Vec<(String, Option<Written>)>,
    pub last: Option<String>,
}

/// What a plugin's manifest says its games' starts may hold (its
/// `{"launch": {"wrap", "env"}}` use): the programs a game may be wrapped
/// in, and the variables it may set, each with how its value is written.
/// Neither a program that runs what it's given (`runs_code`) nor a variable
/// that loads code (`loads_code`) is taken, whatever the manifest says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Starts {
    pub wrappers: Vec<Wrapper>,
    pub variables: Vec<(String, Written)>,
    /// the programs its buttons may run in a game's prefix (`tools`)
    pub tools: Vec<String>,
}

impl Starts {
    pub fn of(v: &Value) -> Starts {
        let plain = |s: &str, extra: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c));
        let wrappers = v["wrap"].as_object().into_iter().flatten().filter(|(p, _)| plain(p, "-_.") && !p.starts_with(['-', '.']) && !runs_code(p)).map(|(p, w)| Wrapper {
            program: p.clone(),
            // (an option is a dash and a plain name; one written another way than these, none)
            options: w["options"].as_object().into_iter().flatten().filter(|(o, x)| o.starts_with('-') && plain(o, "-_") && (x.is_null() || Written::of(x).is_some())).map(|(o, x)| (o.clone(), Written::of(x))).collect(),
            last: w["last"].as_str().filter(|l| l.starts_with('-') && plain(l, "-")).map(String::from),
        }).collect();
        let variables = v["env"].as_object().into_iter().flatten().filter(|(k, _)| plain(k, "_") && !loads_code(k)).filter_map(|(k, x)| Some((k.clone(), Written::of(x)?))).collect();
        let mut tools: Vec<String> = v["tools"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|p| plain(p, "-_.") && !p.starts_with(['-', '.']) && !runs_code(p)).map(String::from).collect();
        // (each in one order, whatever order a JSON object's keys come in)
        let (mut wrappers, mut variables): (Vec<Wrapper>, Vec<(String, Written)>) = (wrappers, variables);
        wrappers.sort_by(|a, b| a.program.cmp(&b.program));
        variables.sort_by(|a, b| a.0.cmp(&b.0));
        tools.sort();
        Starts { wrappers, variables, tools }
    }
}

/// Whether a program runs what it's given rather than wrapping a game: a
/// shell, an interpreter, a way to run as someone else, or a program that
/// starts another from its arguments. A plugin can't wrap a game in one.
fn runs_code(p: &str) -> bool {
    const RUNS: [&str; 52] = [
        "sh", "bash", "dash", "zsh", "fish", "ksh", "mksh", "csh", "tcsh", "nu", "elvish", "xonsh", "busybox", "toybox",
        "env", "sudo", "doas", "pkexec", "su", "runuser", "setpriv", "unshare", "nsenter", "chroot", "bwrap",
        "node", "nodejs", "deno", "bun", "tclsh", "wish", "awk", "gawk", "mawk", "nawk", "sed", "xargs", "find",
        "nohup", "setsid", "timeout", "stdbuf", "strace", "ltrace", "gdb", "valgrind",
        "flatpak", "flatpak-spawn", "systemd-run", "xdg-open", "ssh", "socat",
    ];
    let p = p.to_ascii_lowercase();
    RUNS.contains(&p.as_str()) || ["wine", "wine64", "wine-preloader", "wine64-preloader", "cmd", "cmd.exe"].contains(&p.as_str()) || ["python", "perl", "ruby", "lua", "php", "pwsh", "powershell", "osascript"].iter().any(|i| p.starts_with(i))
}

/// Whether a wrapper is one the plugin may wrap a game in, given only the
/// options its manifest names: each a known one, each value after an option
/// that takes one written as it says, and its last one last, so nothing
/// before the game is a program of its own.
fn known_wrapper(w: &[&str], wrappers: &[Wrapper]) -> bool {
    let Some(known) = wrappers.iter().find(|k| w.first() == Some(&k.program.as_str())) else { return false };
    let mut rest = &w[1..];
    if let Some(last) = &known.last {
        let Some((l, before)) = rest.split_last() else { return false };
        if l != last {
            return false;
        }
        rest = before;
    }
    let mut args = rest.iter();
    while let Some(a) = args.next() {
        match known.options.iter().find(|(o, _)| o == a) {
            Some((_, Some(how))) => {
                if !args.next().is_some_and(|v| plain_as(*how, v)) {
                    return false;
                }
            }
            Some((_, None)) => {}
            None => return false,
        }
    }
    true
}

/// Whether a value is written as `how` allows: plain signs only, no longer
/// than 512 characters; a list's items each a plain name, none starting a
/// path, and none whose setting names a command, a file, a folder or a log
/// (MangoHud's `exec=` and `output_folder=`, any program's `config_file=`).
fn plain_as(how: Written, v: &str) -> bool {
    let only = |extra: &str| !v.is_empty() && v.len() <= 512 && v.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c));
    match how {
        Written::Number => only("."),
        Written::Word => only("_.-"),
        Written::List => {
            const NAMES: [&str; 13] = ["exec", "cmd", "command", "run", "file", "files", "folder", "dir", "path", "script", "autostart", "log", "output"];
            only("_.=,;:/+#*-")
                && v.split([',', ';']).all(|item| {
                    let key = item.split('=').next().unwrap_or("");
                    !item.starts_with(['/', '.']) && !item.contains("..") && !key.to_ascii_lowercase().split(['_', '-', ':']).any(|part| NAMES.contains(&part) || part == "config")
                })
        }
    }
}

/// What a button in a plugin's section does, as its `does` says, of the
/// actions Ectomatter makes on a game's page (docs/core-protocol.md,
/// section 11, A game's start).
#[derive(Clone, Debug, PartialEq)]
pub enum Does {
    StartThrough,
    StartEveryThrough,
    OpenPrefix,
    BackUpPrefix,
    ResetPrefix,
    /// a Windows program inside the prefix: one of the tools, or one the
    /// person picks (none)
    RunInPrefix(Option<String>),
    /// a program of this computer's, given the prefix and the build's Wine
    RunWithPrefix(String),
}

impl Does {
    /// What a button does for the plugin whose manifest says `uses`: none for
    /// a disabled one, one that names nothing Ectomatter makes, a plugin that
    /// doesn't use `launch`, or a program its `tools` don't name.
    pub fn of(action: &Value, uses: &crate::core_host::Uses) -> Option<Does> {
        if action["disabled"] == true || !uses.launch {
            return None;
        }
        let does = &action["does"];
        let tool = |key: &str| does[key].as_str().filter(|p| uses.starts.tools.iter().any(|t| t == p)).map(String::from);
        match does.as_str() {
            Some("start-through") => Some(Does::StartThrough),
            Some("start-every-through") => Some(Does::StartEveryThrough),
            Some("open-prefix") => Some(Does::OpenPrefix),
            Some("back-up-prefix") => Some(Does::BackUpPrefix),
            Some("reset-prefix") => Some(Does::ResetPrefix),
            Some(_) => None,
            None if does["run-in-prefix"] == "pick" => Some(Does::RunInPrefix(None)),
            None => tool("run-in-prefix").map(|p| Does::RunInPrefix(Some(p))).or_else(|| tool("run-with-prefix").map(Does::RunWithPrefix)),
        }
    }
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

/// Whether a variable loads code or points a program at files, which no
/// plugin and no person may set through Ectomatter: the dynamic loader's,
/// Vulkan's loader's, SDL's own library, Proton's debugger, Wine's loader
/// and paths, the shell's and Python's start files, any search path, any
/// file of settings.
fn loads_code(k: &str) -> bool {
    let named = ["SDL_DYNAMIC_API", "PROTON_REMOTE_DEBUG_CMD", "WINELOADER", "WINESERVER", "WINEPREFIX", "WINEDLLPATH", "HOME", "SHELL", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "PYTHONSTARTUP", "BASH_ENV", "ENV"].contains(&k);
    named || k.starts_with("LD_") || k.starts_with("VK_") || k.contains("PRELOAD") || k.ends_with("PATH") || k.ends_with("_FILENAMES") || k.ends_with("_FILES") || k.contains("CONFIGFILE") || k.contains("CONFIG_FILE")
}

/// Whether a variable the person typed may be set: any but those that load
/// code (`loads_code`), its value plain (no path, no shell's signs).
fn own_may(k: &str, v: &str) -> bool {
    !loads_code(k) && v.chars().all(|c| c.is_ascii_alphanumeric() || "_.=,;:+#-".contains(c))
}

/// A plugin's recipe for how a game starts, as Ectomatter writes it for the
/// launcher: only what the plugin's manifest says it may hold (`starts`),
/// and nothing that runs code of its own (docs/core-protocol.md, section 11,
/// A game's start). Each wrapper is one the manifest names, with the options
/// it names (its last one last, so nothing after is a program of its own);
/// the Proton build and the runtime are ones Steam has (`facts`, `protons`,
/// or the game's own runtime); a variable is one the manifest names, its
/// value written as it says, or one the person typed in the plugin's
/// settings (`own`). What's left out, in words, for the log.
pub fn checked_recipe(recipe: &Value, facts: &Value, starts: &Starts, own: &[String]) -> (Value, Vec<String>) {
    // (anything but a recipe is none)
    if !recipe.is_object() {
        return (Value::Null, vec!["the recipe, which isn’t one".into()]);
    }
    let mut r = recipe.clone();
    let mut out = Vec::new();
    let wraps: Vec<Value> = recipe["wrap"].as_array().into_iter().flatten().filter(|w| {
        let w: Vec<&str> = w.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let fine = known_wrapper(&w, &starts.wrappers);
        if !fine {
            out.push(format!("the wrapper {}", w.join(" ")));
        }
        fine
    }).cloned().collect();
    r["wrap"] = Value::Array(wraps);
    let paths = |k: &str| -> Vec<String> { facts["protons"].as_array().into_iter().flatten().filter_map(|b| b[k].as_str().map(String::from)).collect() };
    if let Some(p) = recipe["proton"].as_str().filter(|p| !paths("path").iter().any(|q| q == p)) {
        out.push(format!("the Proton build {p}"));
        r.as_object_mut().map(|o| o.remove("proton"));
    }
    if let Some(i) = r.get_mut("instead").and_then(Value::as_array_mut) {
        let known = paths("path");
        i.retain(|p| {
            let fine = p.as_str().is_some_and(|p| known.iter().any(|q| q == p));
            if !fine {
                out.push(format!("the Proton build {p}"));
            }
            fine
        });
    }
    let runtimes: Vec<String> = paths("runtime").into_iter().chain(facts["game"]["runtime"].as_str().map(String::from)).collect();
    if let Some(p) = recipe["runtime"].as_str().filter(|p| !runtimes.iter().any(|q| q == p)) {
        out.push(format!("the runtime {p}"));
        r.as_object_mut().map(|o| o.remove("runtime"));
    }
    // (a variable the manifest names, its value as it says; or one the person typed, that loads nothing)
    let may = |k: &str, v: &Value| {
        let Some(v) = v.as_str() else { return false };
        match starts.variables.iter().find(|(n, _)| n == k) {
            Some((_, how)) => plain_as(*how, v),
            None => own.iter().any(|o| o == k) && own_may(k, v),
        }
    };
    for key in ["env", "env_default", "env_append"] {
        if let Some(o) = r.get_mut(key).and_then(Value::as_object_mut) {
            o.retain(|k, v| {
                let fine = may(k, v);
                if !fine {
                    out.push(format!("the variable {k}"));
                }
                fine
            });
        }
    }
    // (the game's own arguments, plain words; the variables taken away, by plain names)
    if let Some(a) = r.get_mut("args").and_then(Value::as_array_mut) {
        a.retain(|x| {
            let fine = x.as_str().is_some_and(|x| !x.contains('\0') && x.len() <= 512);
            if !fine {
                out.push(format!("the argument {x}"));
            }
            fine
        });
    }
    if let Some(u) = r.get_mut("unset").and_then(Value::as_array_mut) {
        u.retain(|x| x.as_str().is_some_and(|x| x.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')));
    }
    (r, out)
}

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
    crate::files::write_whole(&p, text.as_bytes()).map_err(|e| e.to_string())
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
pub fn listen(tell: crate::shell::Sender<Start>) {
    listen_at(&socket(), tell);
}

/// The same, on the socket at `path` (a test's own). (Elsewhere than Unix
/// nothing listens yet: Steam's own launcher for Ectomatter runs on Linux.)
#[cfg(not(unix))]
fn listen_at(_: &Path, _: crate::shell::Sender<Start>) {}

#[cfg(unix)]
fn listen_at(path: &Path, tell: crate::shell::Sender<Start>) {
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
        if std::fs::read_to_string(crate::computer::here(format!("/proc/{at}/comm"))).is_ok_and(|c| c.trim_end() == "reaper") && std::fs::read(crate::computer::here(format!("/proc/{at}/cmdline"))).ok().and_then(|c| crate::steam::app_of_reaper(&c)).is_some() {
            return Some(at);
        }
        at = crate::steam::parent_in(&std::fs::read_to_string(crate::computer::here(format!("/proc/{at}/stat"))).ok()?)?;
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
            "protons": self.builds.iter().map(|b| json!({ "id": b.id, "name": b.name, "path": b.path, "runtime": b.runtime, "from_steam": b.from_steam })).collect::<Vec<_>>(),
            "main": self.main,
            "cores": self.cores,
        })
    }
}

/// The main monitor as a game's start needs it: its size, its refresh rate
/// in hertz, and whether HDR is on (from ECTOMATTER_MONITORS's file
/// offscreen, as the plugins' monitors are).
pub fn main_monitor() -> Value {
    // (a snapshot's monitors: those of the computer described for it, or of ECTOMATTER_MONITORS's file)
    use crate::core_host::Computer;
    let told = match (crate::computer::described(), std::env::var_os("ECTOMATTER_MONITORS")) {
        (Some(c), _) => Some(c.monitors().unwrap_or(Value::Null)),
        (None, Some(f)) => Some(std::fs::read_to_string(&f).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null)),
        (None, None) => None,
    };
    let list: Vec<crate::monitors::Monitor> = match told {
        Some(v) => {
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

/// A program run for a game's prefix with the game's Proton build: inside
/// it (`inside`, a Windows program, as `proton run` runs one), or a program
/// of this computer's given the prefix and the build's own Wine (`WINEPREFIX`,
/// `WINE`, `WINESERVER`), as Wine's tools that work on a prefix take them.
pub fn in_prefix(root: &Path, library: &Path, app: u32, proton: &Path, inside: bool, what: &[String]) -> std::process::Command {
    let compat = prefix_of(library, app);
    let mut c;
    if inside {
        c = std::process::Command::new(proton.join("proton"));
        c.arg("run").args(what);
        c.env("STEAM_COMPAT_DATA_PATH", &compat);
        c.env("STEAM_COMPAT_CLIENT_INSTALL_PATH", root);
        c.env("SteamAppId", app.to_string());
        c.env("SteamGameId", app.to_string());
    } else {
        c = std::process::Command::new(what.first().map_or("", String::as_str));
        c.args(what.iter().skip(1));
        c.env("WINEPREFIX", compat.join("pfx"));
        for (var, bin) in [("WINE", "wine"), ("WINESERVER", "wineserver")] {
            let p = proton.join("files/bin").join(bin);
            if p.is_file() {
                c.env(var, p);
            }
        }
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
    for e in std::fs::read_dir(crate::computer::here("/sys/devices/system/cpu")).ok()?.flatten() {
        let d = e.path().join("cache/index3");
        let (Ok(size), Ok(list)) = (std::fs::read_to_string(d.join("size")), std::fs::read_to_string(d.join("shared_cpu_list"))) else { continue };
        let Ok(kb) = size.trim().trim_end_matches('K').parse::<u64>() else { continue };
        if best.as_ref().is_none_or(|(b, _)| kb > *b) {
            best = Some((kb, list.trim().to_string()));
        }
    }
    best.map(|(_, l)| l)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Game Configurator's grant, as its manifest gives it.
    fn configurator() -> Starts {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/game-configurator");
        crate::core_host::Manifest::read(&dir).unwrap().uses.starts
    }

    /// docs/core-protocol.md, section 11: a plugin's recipe holds only what
    /// its manifest names. The Game Configurator's own (gamescope with its
    /// options, GameMode, MangoHud, a build Steam has, Proton's and DXVK's
    /// variables, the person's own) is kept whole; a program to wrap the game
    /// in that its manifest doesn't name, a program slipped into gamescope's
    /// options, a build or runtime Steam doesn't have, or a variable that
    /// loads code is left out, and said.
    #[test]
    fn a_recipe_holds_only_what_its_plugins_manifest_names() {
        let starts = configurator();
        let facts = json!({ "protons": [{ "path": "/steam/compatibilitytools.d/GE-Proton10-1", "runtime": "/steam/steamapps/common/SteamLinuxRuntime_sniper" }] });
        let good = json!({ "proton": "/steam/compatibilitytools.d/GE-Proton10-1", "runtime": "/steam/steamapps/common/SteamLinuxRuntime_sniper",
            "wrap": [["gamescope", "-W", "3440", "-H", "1440", "-w", "2560", "-h", "1080", "-r", "165", "-f", "-F", "fsr", "--hdr-enabled", "--hdr-itm-enabled", "--hdr-itm-sdr-nits", "203", "--mangoapp", "--"], ["gamemoderun"], ["mangohud"]],
            "env": { "PROTON_ENABLE_HDR": "1", "DXVK_HDR": "1", "WINEDLLOVERRIDES": "dwrite=n,b", "MY_MOD": "on" }, "env_default": { "PROTON_SONY_AUTO_XINPUT": "1" }, "args": ["-novid"] });
        let (r, out) = checked_recipe(&good, &facts, &starts, &["MY_MOD".to_string()]);
        assert_eq!((r, out), (good.clone(), Vec::<String>::new()), "kept whole");
        let bad = json!({ "proton": "/tmp/evil",  "runtime": "/tmp/evil-runtime",
            "wrap": [["sh", "-c", "evil"], ["gamescope", "evil", "--"], ["gamescope", "-W", "3440", "--", "evil"], ["gamescope", "-W", "$(evil)", "--"], ["gamemoderun", "evil"], ["mangohud"]],
            "env": { "LD_PRELOAD": "/tmp/evil.so", "PROTON_LOG": "1", "SDL_DYNAMIC_API": "/data/x.so", "VK_ICD_FILENAMES": "/data/x.json", "MANGOHUD_CONFIG": "exec=evil", "PROTON_REMOTE_DEBUG_CMD": "evil", "PROTON_ENABLE_HDR": "$(evil)", "WINEDLLOVERRIDES": "../evil=n" },
            "env_append": { "PATH": "/tmp" }, "args": ["-novid"], "unset": ["PROTON_PREFER_SDL", "A;B"] });
        let (r, out) = checked_recipe(&bad, &facts, &starts, &[]);
        assert_eq!(r["wrap"], json!([["mangohud"]]));
        assert!(r.get("proton").is_none() && r.get("runtime").is_none(), "Steam's own builds only: the launcher picks one");
        assert_eq!(r["env"], json!({ "PROTON_LOG": "1" }), "the variables that load code or run a command, and values that aren't plain, left out");
        assert_eq!(r["env_append"], json!({}));
        assert_eq!((r["args"].clone(), r["unset"].clone()), (json!(["-novid"]), json!(["PROTON_PREFER_SDL"])));
        assert_eq!(out.len(), 15, "{out:?}");
        // (the person's own variable is theirs to set, unless it loads code, or its value is a path)
        let own = |k: &str, v: &str| checked_recipe(&json!({ "env": { k: v } }), &facts, &starts, &[k.to_string()]).0["env"][k].clone();
        assert_eq!(own("MY_MOD", "on"), "on");
        assert_eq!(own("LD_PRELOAD", "/tmp/evil.so"), Value::Null);
        assert_eq!(own("SDL_DYNAMIC_API", "x.so"), Value::Null);
        assert_eq!(own("MY_MOD", "/tmp/x"), Value::Null);
        // (and a recipe that isn't one is none, not a crash)
        assert_eq!(checked_recipe(&json!([]), &facts, &starts, &[]).0, Value::Null);
        assert_eq!(checked_recipe(&json!("x"), &facts, &starts, &[]).0, Value::Null);
    }

    /// Boundary: a plugin Ectomatter has never seen names its own start in
    /// its manifest, and Ectomatter writes just that: its own wrapper with the
    /// options it names, its own variables, each value written as it says.
    /// Whatever it names, no program that runs what it's given wraps a game,
    /// no variable that loads code is set, and no value names a command, a
    /// file or a folder; nor does it get another plugin's (gamescope here).
    #[test]
    fn a_plugin_names_its_own_start() {
        let starts = Starts::of(&json!({
            "wrap": { "foowrap": { "options": { "--level": "number", "--quiet": null } }, "bash": { "options": { "-c": "word" } }, "/tmp/evil": {}, "sh": {} },
            "env": { "FOO_TUNE": "word", "FOO_LIST": "list", "LD_PRELOAD": "word", "FOO_PATH": "word", "VK_LAYER_FOO": "word", "BAD-NAME": "word", "FOO_ODD": "colour" },
        }));
        assert_eq!(starts.wrappers.iter().map(|w| w.program.as_str()).collect::<Vec<_>>(), ["foowrap"], "no shell, no path");
        assert_eq!(starts.variables, [("FOO_LIST".to_string(), Written::List), ("FOO_TUNE".to_string(), Written::Word)], "none that loads code, none written an unknown way");
        let facts = json!({ "protons": [] });
        let good = json!({ "wrap": [["foowrap", "--level", "3", "--quiet"]], "env": { "FOO_TUNE": "fast-1", "FOO_LIST": "a=1,b;c:2,0xbeef/0xdead" } });
        assert_eq!(checked_recipe(&good, &facts, &starts, &[]), (good.clone(), Vec::<String>::new()), "kept whole");
        let bad = json!({
            "wrap": [["gamescope", "-W", "3440", "--"], ["bash", "-c", "evil"], ["foowrap", "--level", "$(evil)"], ["foowrap", "--other"], ["foowrap", "evil"]],
            "env": { "FOO_TUNE": "a b", "LD_PRELOAD": "x.so", "PROTON_ENABLE_HDR": "1" },
            "env_default": { "FOO_LIST": "exec=evil" }, "env_append": { "FOO_LIST": "output_folder=x" },
        });
        let (r, out) = checked_recipe(&bad, &facts, &starts, &[]);
        assert_eq!(r["wrap"], json!([]), "{r}");
        assert_eq!((r["env"].clone(), r["env_default"].clone(), r["env_append"].clone()), (json!({}), json!({}), json!({})), "{r}");
        assert_eq!(out.len(), 10, "{out:?}");
        for (v, fine) in [("dwrite=n,b;d3d9=n", true), ("fps_limit=120,position=top-left", true), ("config_file=x", false), ("../evil=n", false), ("/x=n", false), ("a,exec", false), ("log_duration=5", false)] {
            assert_eq!(plain_as(Written::List, v), fine, "{v}");
        }
    }

    /// Boundary: a button in the section of a plugin Ectomatter has never
    /// seen does what it says, with the programs its manifest names: its own
    /// tool, run with the game's prefix. A program its manifest doesn't name,
    /// a disabled button, one that names nothing Ectomatter makes, and any
    /// button of a plugin that doesn't use `launch`, do nothing.
    #[test]
    fn a_plugins_button_does_what_it_says() {
        let uses = crate::core_host::Uses::of(&json!([{ "launch": { "tools": ["foo-tool", "bash", "wine", "python3"] } }]));
        assert_eq!(uses.starts.tools, ["foo-tool"], "no shell, Wine's own launcher or interpreter");
        let does = |a: Value| Does::of(&a, &uses);
        assert_eq!(does(json!({ "id": "mine", "does": { "run-with-prefix": "foo-tool" } })), Some(Does::RunWithPrefix("foo-tool".into())));
        assert_eq!(does(json!({ "id": "pick", "does": { "run-in-prefix": "pick" } })), Some(Does::RunInPrefix(None)));
        assert_eq!(does(json!({ "id": "b", "does": "back-up-prefix" })), Some(Does::BackUpPrefix));
        assert_eq!(does(json!({ "id": "x", "does": { "run-with-prefix": "winetricks" } })), None, "not among its tools");
        assert_eq!(does(json!({ "id": "x", "does": { "run-with-prefix": "bash" } })), None, "a shell is never a tool");
        assert_eq!(does(json!({ "id": "x", "does": "open-prefix", "disabled": true })), None);
        assert_eq!(does(json!({ "id": "x", "does": "format-the-disk" })), None);
        assert_eq!(does(json!({ "id": "open-prefix" })), None, "the id is the plugin's, not what it does");
        let plain = crate::core_host::Uses::of(&json!(["monitors"]));
        assert_eq!(Does::of(&json!({ "does": "start-through" }), &plain), None, "a game's start is only a launch plugin's");
    }

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
        let dir = std::env::temp_dir().join(format!("ectomatter-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("start.sock");
        let (tx, rx) = crate::shell::quiet_channel::<Start>();
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
        let start = rx.recv_timeout(Duration::from_secs(10)).expect("the start told");
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
        put(lib.join("steamapps/appmanifest_9999990.acf"), "\"AppState\" { \"installdir\" \"Proton 11.0\" }");
        put(lib.join("steamapps/common/Proton 11.0/proton"), "");
        put(lib.join("steamapps/common/Proton 11.0/toolmanifest.vdf"), "\"manifest\" { \"require_tool_appid\" \"4183110\" }");
        put(lib.join("steamapps/appmanifest_9999991.acf"), "\"AppState\" { \"installdir\" \"Some Game\" }");
        put(lib.join("steamapps/common/Some Game/game.exe"), "");
        put(root.join("steamapps/appmanifest_4183110.acf"), "\"AppState\" { \"installdir\" \"SteamLinuxRuntime_4\" }");
        put(root.join("steamapps/common/SteamLinuxRuntime_4/_v2-entry-point"), "");
        let b = builds(&root);
        let ids: Vec<(&str, bool)> = b.iter().map(|b| (b.id.as_str(), b.from_steam)).collect();
        assert_eq!(ids, [("Proton-GE Latest", false), ("proton_11", true), ("proton_hotfix", true)], "Steam's own, a newer one too");
        assert_eq!((steam_id_of("Proton 6.3"), steam_id_of("Proton - Experimental"), steam_id_of("Proton 10.0")), ("proton_63".into(), "proton_experimental".into(), "proton_10".into()));
        assert!(b.iter().all(|b| b.runtime.as_deref() == Some(root.join("steamapps/common/SteamLinuxRuntime_4").as_path())));
        let _ = std::fs::remove_dir_all(&s);
    }

    /// Steam's Compatibility setting changed in place (in a made-up Steam,
    /// as Steam is only files): the games set to start through Ectomatter,
    /// the other games as they were, and the file copied aside first.
    #[test]
    fn steams_settings_change_for_the_games_moved() {
        let s = std::env::temp_dir().join(format!("ectomatter-steam-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&s);
        let root = s.join("Steam");
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(root.join("config/config.vdf"), "\"InstallConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"CompatToolMapping\"\n\t\t\t\t{\n\t\t\t\t\t\"1808500\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"name\"\t\t\"proton_hotfix\"\n\t\t\t\t\t\t\"config\"\t\t\"\"\n\t\t\t\t\t\t\"priority\"\t\t\"250\"\n\t\t\t\t\t}\n\t\t\t\t\t\"418530\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"name\"\t\t\"proton_9\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n").unwrap();
        // (the copies in the test's own folder, so Grant's data is never touched)
        let aside = change_files(&root, &[Switch { app: "1808500".into(), through: TOOL }], &s.join("backups")).unwrap();
        assert!(aside.starts_with(s.join("backups")));
        assert_eq!(mapping(&root).get("1808500").map(String::as_str), Some(TOOL));
        assert_eq!(mapping(&root).get("418530").map(String::as_str), Some("proton_9"), "another game as it was");
        assert!(std::fs::read_to_string(aside.join("config.vdf")).unwrap().contains("proton_hotfix"), "copied aside first");
        let _ = std::fs::remove_dir_all(&s);
    }
}
