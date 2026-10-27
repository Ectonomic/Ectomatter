//! Ectomatter's launcher. Steam hands each game whose Compatibility setting
//! names Ectomatter to this program, as it hands others to a Proton build:
//! `ectomatter-launch <verb> <the game's command…>`. It starts the game the
//! way the game's page in Ectomatter's Library sets it up, from the recipe
//! Ectomatter keeps for it (launch/<app>.json in Ectomatter's data folder,
//! else launch/default.json, Every game's): the Proton build, the Steam Linux
//! Runtime that build runs in, the environment, what wraps the game
//! (the programs its plugin names) and the game's own arguments.
//!
//! Before the game starts, it asks Ectomatter, if it's running, to make the
//! game's changes first (its devices' profiles, its monitors, its scripts),
//! and waits up to half a minute for them, so a game finds its monitor in
//! HDR as it starts. Anything but a game's start (Steam's own verbs, for an
//! install script or a path) goes to Proton as it came.
//!
//! A game Steam starts in its Linux version (through the tool's second name,
//! "Ectomatter (Linux version)") starts without Proton: what wraps it, then
//! the Steam Linux Runtime its recipe names, then the game's own command.
//!
//! It runs with nothing but Steam's environment, so a recipe it can't read,
//! a Proton build that's gone or a wrapper that isn't installed is passed
//! over for what is there, and said in the game's log (launch/<app>.log): a
//! game always starts.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::{json, Value};

/// How long a game's start waits for Ectomatter to make its changes.
const HOLD: Duration = Duration::from_secs(30);

/// The verb Steam starts a game with; the others pass straight through.
const START: &str = "waitforexitandrun";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((verb, cmd)) = args.split_first() else {
        eprintln!("ectomatter-launch: Steam starts this with a verb and the game's command");
        std::process::exit(2);
    };
    let env: BTreeMap<String, String> = std::env::vars().collect();
    // (the hold first: Ectomatter writes the game's recipe afresh as it makes
    // the game's changes, so the recipe is read after)
    if verb == START {
        hold(&app_of(&env), &env);
    }
    let plan = plan(verb, cmd, &env);
    log(&plan, verb, &env);
    // (a path asked of a Linux version: the same path, as nothing translates it)
    if let Some(said) = &plan.say {
        println!("{said}");
        return;
    }
    let Some(first) = plan.argv.first() else {
        eprintln!("ectomatter-launch: no Proton build is installed to start {}", plan.name);
        std::process::exit(1);
    };
    let mut c = Command::new(first);
    c.args(&plan.argv[1..]);
    for k in &plan.unset {
        c.env_remove(k);
    }
    for (k, v) in &plan.env {
        c.env(k, v);
    }
    let err = c.exec();
    eprintln!("ectomatter-launch: couldn't start {first}: {err}");
    std::process::exit(1);
}

/// What the launcher runs: the command, the environment it changes, the
/// game it's for (its Steam number and name), and what went otherwise than
/// the recipe said, for the game's log.
#[derive(Debug, Default, PartialEq)]
struct Plan {
    argv: Vec<String>,
    env: Vec<(String, String)>,
    unset: Vec<String>,
    app: String,
    name: String,
    notes: Vec<String>,
    /// words to answer with in place of running anything
    say: Option<String>,
}

/// Whether a program is Windows's, by its name: what Steam's command for a
/// game starts with tells a Windows version from a Linux one.
fn windows_program(p: &str) -> bool {
    let l = p.to_ascii_lowercase();
    [".exe", ".bat", ".cmd", ".com", ".msi", ".lnk"].iter().any(|e| l.ends_with(e))
}

/// The game's start as its recipe sets it up, for `verb` and the game's
/// command as Steam gave it.
fn plan(verb: &str, cmd: &[String], env: &BTreeMap<String, String>) -> Plan {
    let app = app_of(env);
    let mut notes = Vec::new();
    let recipe = recipe(&data_dir(env), &app, &mut notes);
    let steam = steam_root(env);
    let libraries = libraries(&steam);
    let name = recipe["name"].as_str().map(String::from).unwrap_or_else(|| format!("app {app}"));
    // a Linux version: its own program, by Steam's command where there's one to run
    let starts = verb == START || verb == "run";
    if (starts && cmd.first().is_some_and(|c| !windows_program(c))) || (!starts && recipe["native"] == true) {
        return linux_plan(verb, cmd, &recipe, env, Plan { app, name, notes, ..Plan::default() });
    }
    let Some(proton) = proton_of(&recipe, &steam, &libraries, &mut notes) else {
        notes.push("no Proton build is installed".into());
        return Plan { app, name, notes, ..Plan::default() };
    };
    let runtime = runtime_of(&recipe, &proton, &libraries, &mut notes);
    let script = proton.join("proton").to_string_lossy().into_owned();
    let mut argv = Vec::new();
    let (mut set, mut unset) = (Vec::new(), Vec::new());
    if verb == START {
        // what wraps the game, outside the runtime, as a launch line would have it
        for w in recipe["wrap"].as_array().into_iter().flatten() {
            let w: Vec<String> = w.as_array().into_iter().flatten().filter_map(|a| a.as_str().map(String::from)).collect();
            match w.first() {
                Some(bin) if found(bin, env) => argv.extend(w),
                Some(bin) => notes.push(format!("{bin} isn't installed, so the game starts without it")),
                None => {}
            }
        }
        if let Some(rt) = &runtime {
            argv.extend([rt.join("_v2-entry-point").to_string_lossy().into_owned(), format!("--verb={verb}"), "--".into()]);
        }
        argv.extend([script, verb.to_string()]);
        argv.extend(cmd.iter().cloned());
        argv.extend(recipe["args"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(String::from)));
        set = recipe_env(&recipe, env);
        unset = recipe["unset"].as_array().into_iter().flatten().filter_map(|k| k.as_str().map(String::from)).collect();
    } else {
        // (an install script, a path asked for, a verb Proton knows and this doesn't: in
        // the runtime as Steam would run it, nothing added)
        if let Some(rt) = &runtime {
            argv.extend([rt.join("_v2-entry-point").to_string_lossy().into_owned(), format!("--verb={verb}"), "--".into()]);
        }
        argv.extend([script, verb.to_string()]);
        argv.extend(cmd.iter().cloned());
    }
    // the build and its runtime shared into the runtime's container, as Steam
    // shares the tools of its own chain (umu-launcher does the same)
    set.extend(shared(&[Some(proton.as_path()), runtime.as_deref()], env));
    Plan { argv, env: set, unset, app, name, notes, say: None }
}

/// The variables a recipe sets, as game-helper set them: `env` over what's
/// there; `env_default` only where nothing (a launch line in Steam, say) has
/// set it already; `env_append` added to what's there, after a comma.
fn recipe_env(recipe: &Value, env: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (k, v) in recipe["env_default"].as_object().into_iter().flatten() {
        if let (Some(v), true) = (v.as_str(), env.get(k).is_none_or(|x| x.is_empty())) {
            out.push((k.clone(), v.to_string()));
        }
    }
    for (k, v) in recipe["env_append"].as_object().into_iter().flatten() {
        if let Some(v) = v.as_str() {
            out.push((k.clone(), match env.get(k).filter(|x| !x.is_empty()) {
                Some(x) => format!("{x},{v}"),
                None => v.to_string(),
            }));
        }
    }
    out.extend(recipe["env"].as_object().into_iter().flatten().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))));
    out
}

/// The folders the Steam Linux Runtime is to share into its container, ahead
/// of those Steam gave (STEAM_COMPAT_TOOL_PATHS and STEAM_COMPAT_MOUNTS):
/// one outside the home folder and Steam's libraries isn't seen inside otherwise.
fn shared(dirs: &[Option<&Path>], env: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let ours: Vec<String> = dirs.iter().flatten().map(|d| d.to_string_lossy().into_owned()).collect();
    if ours.is_empty() {
        return Vec::new();
    }
    ["STEAM_COMPAT_TOOL_PATHS", "STEAM_COMPAT_MOUNTS"]
        .iter()
        .map(|k| {
            let mut all = ours.clone();
            all.extend(env.get(*k).into_iter().flat_map(|x| x.split(':')).filter(|x| !x.is_empty() && !ours.iter().any(|o| o == x)).map(String::from));
            (k.to_string(), all.join(":"))
        })
        .collect()
}

/// A Linux version's start: what wraps it and its arguments, as its recipe
/// says; and, where the recipe is the Linux version's own, its environment
/// and the Steam Linux Runtime it runs in (a recipe for Proton names neither
/// for it). An install script runs in that runtime with nothing added; a
/// path asked for is the same path; another verb runs the command as it came.
fn linux_plan(verb: &str, cmd: &[String], recipe: &Value, env: &BTreeMap<String, String>, mut plan: Plan) -> Plan {
    let own = recipe["native"] == true;
    let runtime = recipe["runtime"].as_str().filter(|_| own).map(PathBuf::from).filter(|p| {
        let there = p.join("_v2-entry-point").is_file();
        if !there {
            plan.notes.push(format!("{} isn't there, so the game starts without it", p.display()));
        }
        there
    });
    let entry = |argv: &mut Vec<String>| {
        if let Some(rt) = &runtime {
            argv.extend([rt.join("_v2-entry-point").to_string_lossy().into_owned(), format!("--verb={verb}"), "--".into()]);
        }
    };
    match verb {
        START => {
            for w in recipe["wrap"].as_array().into_iter().flatten() {
                let w: Vec<String> = w.as_array().into_iter().flatten().filter_map(|a| a.as_str().map(String::from)).collect();
                match w.first() {
                    Some(bin) if found(bin, env) => plan.argv.extend(w),
                    Some(bin) => plan.notes.push(format!("{bin} isn't installed, so the game starts without it")),
                    None => {}
                }
            }
            entry(&mut plan.argv);
            plan.argv.extend(cmd.iter().cloned());
            plan.argv.extend(recipe["args"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(String::from)));
            if own {
                plan.env = recipe_env(recipe, env);
                plan.unset = recipe["unset"].as_array().into_iter().flatten().filter_map(|k| k.as_str().map(String::from)).collect();
            }
        }
        "run" => {
            entry(&mut plan.argv);
            plan.argv.extend(cmd.iter().cloned());
        }
        "getcompatpath" | "getnativepath" => plan.say = Some(cmd.first().cloned().unwrap_or_default()),
        _ => plan.argv.extend(cmd.iter().cloned()),
    }
    plan.env.extend(shared(&[runtime.as_deref()], env));
    plan
}

/// The game's Steam number, as Steam gives a compatibility tool it.
fn app_of(env: &BTreeMap<String, String>) -> String {
    ["STEAM_COMPAT_APP_ID", "SteamAppId", "SteamGameId"].iter().find_map(|k| env.get(*k).filter(|v| !v.is_empty() && *v != "0").cloned()).unwrap_or_else(|| "0".into())
}

/// The game's recipe, else Every game's, else none (each read as JSON; one
/// that can't be read is passed over, and said).
fn recipe(data: &Path, app: &str, notes: &mut Vec<String>) -> Value {
    for file in [format!("{app}.json"), "default.json".into()] {
        let p = data.join("launch").join(&file);
        match std::fs::read_to_string(&p) {
            Ok(t) => match serde_json::from_str::<Value>(&t) {
                Ok(v) if v.is_object() => return v,
                _ => notes.push(format!("{} couldn't be read", p.display())),
            },
            Err(_) if file != "default.json" => notes.push(format!("no recipe of its own ({file}); Every game's")),
            Err(_) => notes.push("no recipe for Every game either; Steam's own way".into()),
        }
    }
    Value::Null
}

/// The Proton build to start the game with: the recipe's, while it's
/// installed; else the first installed of those it names instead, in its
/// order (the plugin's); else any installed, the last by name first.
fn proton_of(recipe: &Value, steam: &Path, libraries: &[PathBuf], notes: &mut Vec<String>) -> Option<PathBuf> {
    let is_proton = |p: &Path| p.join("proton").is_file();
    if let Some(p) = recipe["proton"].as_str().map(PathBuf::from) {
        if is_proton(&p) {
            return Some(p);
        }
        notes.push(format!("{} isn't installed anymore", p.display()));
    }
    for p in recipe["instead"].as_array().into_iter().flatten().filter_map(Value::as_str).map(PathBuf::from) {
        if is_proton(&p) {
            notes.push(format!("started with {}", p.display()));
            return Some(p);
        }
    }
    let mut found: Vec<PathBuf> = Vec::new();
    for dir in std::iter::once(steam.join("compatibilitytools.d")).chain(libraries.iter().map(|l| l.join("steamapps/common"))) {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            found.extend(rd.flatten().map(|e| e.path()).filter(|p| is_proton(p)));
        }
    }
    found.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    let pick = found.into_iter().next()?;
    notes.push(format!("started with {}", pick.display()));
    Some(pick)
}

/// The Steam Linux Runtime a Proton build runs in: the recipe's, else the
/// one the build's own manifest asks for (`require_tool_appid`), wherever
/// Steam installed it; none for a build that asks for none.
fn runtime_of(recipe: &Value, proton: &Path, libraries: &[PathBuf], notes: &mut Vec<String>) -> Option<PathBuf> {
    let entry = |p: &Path| p.join("_v2-entry-point").is_file();
    if let Some(p) = recipe["runtime"].as_str().map(PathBuf::from) {
        if entry(&p) {
            return Some(p);
        }
        notes.push(format!("{} isn't there; the one the build asks for", p.display()));
    }
    let manifest = std::fs::read_to_string(proton.join("toolmanifest.vdf")).ok()?;
    let id = values(&manifest, "require_tool_appid").into_iter().next()?;
    for lib in libraries {
        let Ok(acf) = std::fs::read_to_string(lib.join(format!("steamapps/appmanifest_{id}.acf"))) else { continue };
        if let Some(dir) = values(&acf, "installdir").into_iter().next() {
            let p = lib.join("steamapps/common").join(dir);
            if entry(&p) {
                return Some(p);
            }
        }
    }
    notes.push(format!("the runtime {id} isn't installed; Proton starts without it"));
    None
}

/// Every value given for `key` in a file in Steam's own text format (VDF),
/// at any depth.
fn values(text: &str, key: &str) -> Vec<String> {
    let mut toks: Vec<String> = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(n) = chars.next() {
                                s.push(n);
                            }
                        }
                        '"' => break,
                        c => s.push(c),
                    }
                }
                toks.push(s);
            }
            '{' | '}' => toks.push(c.to_string()),
            // (a comment, to the end of its line)
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    toks.windows(2).filter(|w| w[0] == key && w[1] != "{" && w[1] != "}").map(|w| w[1].clone()).collect()
}

/// Steam's own folder: where Steam says it is, else where it installs itself.
fn steam_root(env: &BTreeMap<String, String>) -> PathBuf {
    if let Some(p) = env.get("STEAM_COMPAT_CLIENT_INSTALL_PATH").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    home(env).join(".local/share/Steam")
}

/// Steam's libraries: its own folder, and each one its libraryfolders.vdf names.
fn libraries(steam: &Path) -> Vec<PathBuf> {
    let mut out = vec![steam.to_path_buf()];
    if let Ok(t) = std::fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")) {
        for p in values(&t, "path").into_iter().map(PathBuf::from) {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

fn home(env: &BTreeMap<String, String>) -> PathBuf {
    PathBuf::from(env.get("HOME").cloned().unwrap_or_default())
}

/// Ectomatter's data folder, as Ectomatter finds it.
fn data_dir(env: &BTreeMap<String, String>) -> PathBuf {
    env.get("XDG_DATA_HOME").filter(|p| !p.is_empty()).map(PathBuf::from).unwrap_or_else(|| home(env).join(".local/share")).join("com.ectonomic.ectomatter")
}

/// Whether a program is there to run: a path, or a name on the PATH.
fn found(bin: &str, env: &BTreeMap<String, String>) -> bool {
    if bin.contains('/') {
        return Path::new(bin).is_file();
    }
    env.get("PATH").is_some_and(|path| path.split(':').any(|d| Path::new(d).join(bin).is_file()))
}

/// Ectomatter told the game is starting, and its changes waited for: a line
/// back, or half a minute. Nothing waits where Ectomatter isn't running.
fn hold(app: &str, env: &BTreeMap<String, String>) {
    let Some(dir) = env.get("XDG_RUNTIME_DIR").filter(|p| !p.is_empty()) else { return };
    let Ok(mut s) = std::os::unix::net::UnixStream::connect(Path::new(dir).join("ectomatter/start.sock")) else { return };
    let _ = s.set_read_timeout(Some(HOLD));
    let _ = s.set_write_timeout(Some(Duration::from_secs(2)));
    // (this program's parent: Ectomatter finds Steam's reaper for the game,
    // the process it knows the game by once it runs, above it)
    let parent = unsafe { libc::getppid() };
    let name = recipe(&data_dir(env), app, &mut Vec::new())["name"].as_str().unwrap_or("").to_string();
    if writeln!(s, "{}", json!({ "app": app, "name": name, "pid": parent })).is_err() {
        return;
    }
    let mut line = String::new();
    let _ = BufReader::new(s).read_line(&mut line);
}

/// The game's log: what this start ran and set, and what went otherwise
/// than its recipe said (launch/<app>.log, the last start's).
fn log(plan: &Plan, verb: &str, env: &BTreeMap<String, String>) {
    if verb != START {
        return;
    }
    let dir = data_dir(env).join("launch");
    let _ = std::fs::create_dir_all(&dir);
    let mut out = format!("{} ({})\n", plan.name, plan.app);
    for n in &plan.notes {
        out.push_str(&format!("note: {n}\n"));
    }
    for (k, v) in &plan.env {
        out.push_str(&format!("set {k}={v}\n"));
    }
    for k in &plan.unset {
        out.push_str(&format!("unset {k}\n"));
    }
    out.push_str(&format!("ran: {}\n", plan.argv.join(" ")));
    let _ = std::fs::write(dir.join(format!("{}.log", plan.app)), out);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own for a test, gone after.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Scratch {
            let p = std::env::temp_dir().join(format!("ectomatter-launch-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Scratch(p)
        }
        fn file(&self, rel: &str, text: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text).unwrap();
            p
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A Steam laid out as a real one is: GE-Proton in compatibilitytools.d asking
    /// for the runtime 4183110, installed in a second library; and the
    /// environment Steam gives a compatibility tool for ARC Raiders.
    fn steam(s: &Scratch) -> BTreeMap<String, String> {
        let root = s.0.join("Steam");
        let lib = s.0.join("Games");
        s.file("Steam/compatibilitytools.d/Proton-GE Latest/proton", "#!/bin/sh\n");
        s.file("Steam/compatibilitytools.d/Proton-GE Latest/toolmanifest.vdf", "\"manifest\"\n{\n  // the runtime it runs in\n  \"version\" \"2\"\n  \"commandline\" \"/proton %verb%\"\n  \"require_tool_appid\" \"4183110\"\n}\n");
        s.file("Steam/steamapps/libraryfolders.vdf", &format!("\"libraryfolders\"\n{{\n \"0\" {{ \"path\" \"{}\" }}\n \"1\" {{ \"path\" \"{}\" }}\n}}\n", root.display(), lib.display()));
        s.file("Games/steamapps/appmanifest_4183110.acf", "\"AppState\"\n{\n \"appid\" \"4183110\"\n \"installdir\" \"SteamLinuxRuntime_4\"\n}\n");
        s.file("Games/steamapps/common/SteamLinuxRuntime_4/_v2-entry-point", "#!/bin/sh\n");
        s.file("bin/gamemoderun", "#!/bin/sh\n");
        let mut env = BTreeMap::new();
        env.insert("HOME".into(), s.0.display().to_string());
        env.insert("XDG_DATA_HOME".into(), s.0.join("data").display().to_string());
        env.insert("STEAM_COMPAT_CLIENT_INSTALL_PATH".into(), root.display().to_string());
        env.insert("SteamAppId".into(), "1808500".into());
        env.insert("PATH".into(), s.0.join("bin").display().to_string());
        env
    }

    fn game() -> Vec<String> {
        vec!["/games/ARC Raiders/PioneerGame.exe".into(), "-dx12".into()]
    }

    /// A game's start: what wraps it, then the runtime its Proton build asks
    /// for, then that build with Steam's verb, the game's command and its own
    /// arguments; the recipe's environment set and unset.
    #[test]
    fn a_game_starts_as_its_recipe_says() {
        let s = Scratch::new("start");
        let env = steam(&s);
        let ge = s.0.join("Steam/compatibilitytools.d/Proton-GE Latest");
        s.file("data/com.ectonomic.ectomatter/launch/1808500.json", &json!({
            "name": "ARC Raiders",
            "proton": ge,
            "env": { "PROTON_ENABLE_WAYLAND": "1", "WINE_CPU_TOPOLOGY": "16:0,1,2,3,4,5,6,7,16,17,18,19,20,21,22,23" },
            "unset": ["PROTON_DISABLE_HIDRAW"],
            "wrap": [["gamemoderun"]],
            "args": ["-nolauncher"],
        }).to_string());
        let p = plan(START, &game(), &env);
        let rt = s.0.join("Games/steamapps/common/SteamLinuxRuntime_4/_v2-entry-point");
        assert_eq!(p.argv, [
            "gamemoderun".to_string(),
            rt.display().to_string(), "--verb=waitforexitandrun".into(), "--".into(),
            ge.join("proton").display().to_string(), START.into(),
            "/games/ARC Raiders/PioneerGame.exe".into(), "-dx12".into(), "-nolauncher".into(),
        ]);
        let own: Vec<_> = p.env.iter().filter(|(k, _)| !k.starts_with("STEAM_COMPAT_")).cloned().collect();
        assert_eq!(own, [("PROTON_ENABLE_WAYLAND".to_string(), "1".to_string()), ("WINE_CPU_TOPOLOGY".into(), "16:0,1,2,3,4,5,6,7,16,17,18,19,20,21,22,23".into())]);
        assert!(p.env.iter().any(|(k, v)| k == "STEAM_COMPAT_TOOL_PATHS" && v.starts_with(&ge.display().to_string())));
        assert_eq!(p.unset, ["PROTON_DISABLE_HIDRAW"]);
        assert_eq!((p.app.as_str(), p.name.as_str()), ("1808500", "ARC Raiders"));
        assert!(p.notes.is_empty(), "{:?}", p.notes);
    }

    /// A game with no recipe of its own takes Every game's; a Proton build
    /// that's gone falls back to the first installed of those the recipe
    /// names instead, else to any installed; a wrapper that isn't installed
    /// is left out. Each is said in the notes, and the game starts.
    #[test]
    fn a_game_always_starts() {
        let s = Scratch::new("fallback");
        let env = steam(&s);
        s.file("Steam/compatibilitytools.d/Proton-Other 2/proton", "#!/bin/sh\n");
        let gone = s.0.join("Steam/compatibilitytools.d/proton-cachyos-10.0-20260420-slr-x86_64");
        let ge = s.0.join("Steam/compatibilitytools.d/Proton-GE Latest");
        let other = s.0.join("Steam/compatibilitytools.d/Proton-Other 2");
        s.file("data/com.ectonomic.ectomatter/launch/default.json", &json!({
            "proton": gone, "instead": [s.0.join("Steam/compatibilitytools.d/also-gone"), ge],
            "wrap": [["gamescope", "-W", "3440", "--"], ["gamemoderun"]],
        }).to_string());
        let p = plan(START, &game(), &env);
        assert_eq!(p.argv[0], "gamemoderun", "gamescope isn't installed here");
        assert!(p.argv.contains(&ge.join("proton").display().to_string()), "the recipe's own next pick: {:?}", p.argv);
        let notes = p.notes.join(" | ");
        assert!(notes.contains("no recipe of its own") && notes.contains("isn't installed anymore") && notes.contains("gamescope isn't installed"), "{notes}");
        // (and with none named instead, any installed)
        s.file("data/com.ectonomic.ectomatter/launch/default.json", &json!({ "proton": gone }).to_string());
        let p = plan(START, &game(), &env);
        assert!(p.argv.contains(&other.join("proton").display().to_string()) || p.argv.contains(&ge.join("proton").display().to_string()), "{:?}", p.argv);
    }

    /// Steam's other verbs: an install script and a path asked for run in
    /// the runtime, as Steam's own chain runs them, with nothing added.
    #[test]
    fn steams_other_verbs_pass_through() {
        let s = Scratch::new("verbs");
        let env = steam(&s);
        s.file("data/com.ectonomic.ectomatter/launch/1808500.json", &json!({ "wrap": [["gamemoderun"]], "args": ["-nolauncher"], "env": { "A": "1" } }).to_string());
        let run = plan("run", &["setup.exe".into()], &env);
        assert!(run.argv[0].ends_with("_v2-entry-point") && run.argv.ends_with(&["run".to_string(), "setup.exe".into()]), "{:?}", run.argv);
        assert!(run.env.iter().all(|(k, _)| k.starts_with("STEAM_COMPAT_")), "only the shared folders: {:?}", run.env);
        let path = plan("getcompatpath", &["/a/b".into()], &env);
        assert!(path.argv[0].ends_with("_v2-entry-point") && path.argv.ends_with(&["getcompatpath".to_string(), "/a/b".into()]), "{:?}", path.argv);
    }

    /// A Linux version (Counter-Strike 2's): what wraps it, then the runtime
    /// its recipe names, then its own command and arguments, in its recipe's
    /// environment; no Proton. A path asked for comes back as it was.
    #[test]
    fn a_linux_version_starts_in_its_runtime() {
        let s = Scratch::new("linux");
        let mut env = steam(&s);
        env.insert("SteamAppId".into(), "730".into());
        let sniper = s.0.join("Games/steamapps/common/SteamLinuxRuntime_sniper");
        s.file("Games/steamapps/common/SteamLinuxRuntime_sniper/_v2-entry-point", "#!/bin/sh\n");
        s.file("data/com.ectonomic.ectomatter/launch/730.json", &json!({
            "name": "Counter-Strike 2", "native": true, "runtime": sniper, "env": { "ENABLE_GAMESCOPE_WSI": "1" }, "unset": [], "wrap": [["gamemoderun"]], "args": ["-novid"],
        }).to_string());
        let cs2 = vec!["/games/Counter-Strike Global Offensive/cs2.sh".to_string(), "-steam".into()];
        let p = plan(START, &cs2, &env);
        assert_eq!(p.argv, [
            "gamemoderun".to_string(),
            sniper.join("_v2-entry-point").display().to_string(), "--verb=waitforexitandrun".into(), "--".into(),
            "/games/Counter-Strike Global Offensive/cs2.sh".into(), "-steam".into(), "-novid".into(),
        ]);
        let own: Vec<_> = p.env.iter().filter(|(k, _)| !k.starts_with("STEAM_COMPAT_")).cloned().collect();
        assert_eq!(own, [("ENABLE_GAMESCOPE_WSI".to_string(), "1".to_string())]);
        assert!(p.env.iter().any(|(k, v)| k == "STEAM_COMPAT_MOUNTS" && *v == sniper.display().to_string()), "the runtime shared");
        let path = plan("getcompatpath", &["/a/b".into()], &env);
        assert_eq!((path.say.as_deref(), path.argv.len()), (Some("/a/b"), 0));
        // (with no recipe of its own, Every game's for Proton: its wraps, nothing of Proton's)
        std::fs::remove_file(s.0.join("data/com.ectonomic.ectomatter/launch/730.json")).unwrap();
        s.file("data/com.ectonomic.ectomatter/launch/default.json", &json!({ "proton": "/x", "env": { "PROTON_ENABLE_HDR": "1" }, "wrap": [["gamemoderun"]] }).to_string());
        let p = plan(START, &cs2, &env);
        assert_eq!(p.argv, ["gamemoderun".to_string(), "/games/Counter-Strike Global Offensive/cs2.sh".into(), "-steam".into()]);
        assert!(p.env.is_empty());
    }

    /// game-helper's rules for its variables: a launch line's own Xbox-copy
    /// setting wins, the pad's ignore list is added to, the rest is the
    /// recipe's; the build and its runtime are shared into the container.
    #[test]
    fn a_launch_lines_variables_are_kept_as_game_helper_kept_them() {
        let s = Scratch::new("env");
        let mut env = steam(&s);
        let ge = s.0.join("Steam/compatibilitytools.d/Proton-GE Latest");
        s.file("data/com.ectonomic.ectomatter/launch/1808500.json", &json!({
            "proton": ge,
            "env": { "PROTON_ENABLE_HDR": "1" },
            "env_default": { "PROTON_SONY_AUTO_XINPUT": "0" },
            "env_append": { "SDL_GAMECONTROLLER_IGNORE_DEVICES": "0xbeef/0xdead" },
        }).to_string());
        env.insert("PROTON_SONY_AUTO_XINPUT".into(), "1".into());
        env.insert("SDL_GAMECONTROLLER_IGNORE_DEVICES".into(), "0x28de/0x11ff".into());
        env.insert("STEAM_COMPAT_TOOL_PATHS".into(), "/s/compatibilitytools.d/ectomatter".into());
        let p = plan(START, &game(), &env);
        let get = |k: &str| p.env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("PROTON_SONY_AUTO_XINPUT"), None, "the launch line's own 1 stays");
        assert_eq!(get("SDL_GAMECONTROLLER_IGNORE_DEVICES"), Some("0x28de/0x11ff,0xbeef/0xdead"));
        assert_eq!(get("PROTON_ENABLE_HDR"), Some("1"));
        let rt = s.0.join("Games/steamapps/common/SteamLinuxRuntime_4");
        assert_eq!(get("STEAM_COMPAT_TOOL_PATHS"), Some(format!("{}:{}:/s/compatibilitytools.d/ectomatter", ge.display(), rt.display()).as_str()));
        assert_eq!(get("STEAM_COMPAT_MOUNTS"), Some(format!("{}:{}", ge.display(), rt.display()).as_str()));
    }

    /// Steam's text format, as its tool manifests and library lists write it.
    #[test]
    fn steams_own_files_are_read() {
        let t = "\"a\" { \"path\" \"/x\" \"b\" { \"path\" \"/y \\\"q\\\"\" } } // \"path\" \"/z\"";
        assert_eq!(values(t, "path"), ["/x", "/y \"q\""]);
    }
}
