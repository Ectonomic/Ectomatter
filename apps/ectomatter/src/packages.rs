//! Ectomatter's packages: the cores, each for a device Ectomatter talks to
//! itself (the keyboard's, the mouse's), and the plugins, which add to
//! Ectomatter's pages (the Monitors plugin adds a section to each game's
//! page) or bring in what another program drives (OpenRGB's lighting). Both
//! are packaged the
//! same way (a folder holding its manifest, core.json, the module it names,
//! and its glyph) and run the same way, but they're kept apart: in
//! Ectomatter's source (cores/ and plugins/), in its data folder, and in
//! Settings (each core with its device under Devices, the plugins under
//! Plugins). A manifest says which it is
//! (its `kind`). Some come with Ectomatter, as it runs from its source; the
//! rest were added, from their GitHub releases or a file, into Ectomatter's
//! data folder (added/cores/<id>/<version>/, added/plugins/...), where an
//! update lands too. Where one of each has the same id, the newer version
//! is the one used.
//!
//! What a package can do on the computer is said in Ectomatter's own words,
//! from what its manifest declares (its USB devices and its `uses`), never in
//! the package's: a package can't describe itself as doing less than it may.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::core_host::Manifest;

/// Whether a package is a core, for a device Ectomatter talks to itself, or a
/// plugin, which adds to Ectomatter's pages or brings in what another program
/// drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Core,
    Plugin,
}

impl Kind {
    /// What a manifest says it is: its `kind`, or where it has none, a core
    /// for a package that names the USB devices it reaches (its `matches`)
    /// and a plugin for any other.
    pub fn of(m: &Value) -> Kind {
        match m["kind"].as_str() {
            Some("plugin") => Kind::Plugin,
            Some("core") => Kind::Core,
            _ if m["matches"].as_array().is_some_and(|a| !a.is_empty()) => Kind::Core,
            _ => Kind::Plugin,
        }
    }

    /// Its folder's name, in Ectomatter's source and its data folder alike.
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Core => "cores",
            Kind::Plugin => "plugins",
        }
    }

    /// The word for one, as Settings says it.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Core => "core",
            Kind::Plugin => "plugin",
        }
    }
}

#[derive(Clone)]
pub struct Package {
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// added, rather than come with Ectomatter
    pub added: bool,
}

impl Package {
    pub fn read(dir: &Path, added: bool) -> Result<Package, String> {
        Ok(Package { dir: dir.to_path_buf(), manifest: Manifest::read(dir)?, added })
    }

    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    /// What it's for, in its own words.
    pub fn description(&self) -> String {
        self.manifest.all["description"].as_str().unwrap_or("").to_string()
    }

    /// Its glyph, an SVG file in its folder.
    pub fn glyph(&self) -> Option<PathBuf> {
        let g = self.manifest.all["glyph"].as_str()?;
        let p = self.dir.join(g);
        (p.starts_with(&self.dir) && p.is_file()).then_some(p)
    }

    /// Where its releases are on GitHub, as "owner/name": as its manifest says,
    /// or for one added from a GitHub address, that address (kept beside it).
    pub fn repository(&self) -> Option<String> {
        if let Some(r) = self.manifest.all["repository"].as_str() {
            return repository_of(r);
        }
        let kept = std::fs::read(self.dir.parent()?.join("source.json")).ok()?;
        repository_of(serde_json::from_slice::<Value>(&kept).ok()?["repository"].as_str()?)
    }

    /// Whether it adds a section to each game's and app's page.
    pub fn adds_to_games(&self) -> bool {
        self.manifest.all["adds"].as_array().is_some_and(|a| a.iter().any(|x| x == "game"))
    }

    /// Whether it works on this system: everywhere, unless its manifest
    /// names the systems it works on (`"systems": ["linux"]`); one that
    /// doesn't work here isn't offered here.
    pub fn works_here(&self) -> bool {
        works_here(&self.manifest.all)
    }

    /// The systems it works on, by their names, as Settings tags it.
    pub fn systems(&self) -> Vec<&'static str> {
        systems_of(&self.manifest.all)
    }

    pub fn kind(&self) -> Kind {
        Kind::of(&self.manifest.all)
    }

    /// Its own folder of files, which it sees as /data: a core's in Ectomatter's
    /// data folder's cores/, a plugin's in its plugins/.
    pub fn data(&self) -> PathBuf {
        crate::data_dir().join(self.kind().folder()).join(self.id())
    }

    /// What it can do on the computer, a sentence each, in Ectomatter's words.
    pub fn can(&self) -> Vec<String> {
        can_words(&self.manifest.all)
    }

    /// The setting that turns it on or off. The keyboard's core is the
    /// keyboard's own Control, which Settings' Devices shows too.
    pub fn switch(&self) -> String {
        if self.id() == crate::z10::CORE {
            "devices.z10.control".into()
        } else {
            format!("{}.{}.on", self.kind().folder(), self.id())
        }
    }
}

/// What a package can do on the computer, a sentence each, in Ectomatter's
/// words, from its manifest (a package being added has nothing else yet):
/// its USB devices, and its `uses`.
pub fn can_words(m: &Value) -> Vec<String> {
    let uses = crate::core_host::Uses::of(&m["uses"]);
    let mut out = Vec::new();
    if m["matches"].as_array().is_some_and(|a| !a.is_empty()) {
        let names: Vec<String> = m["devices"].as_array().into_iter().flatten().filter_map(|d| d.as_str()).map(|d| format!("the {d}")).collect();
        out.push(match names.len() {
            0 => "Talks to its devices over USB.".to_string(),
            1 => format!("Talks to {} over USB.", names[0]),
            _ => format!("Talks to {} and {} over USB.", names[..names.len() - 1].join(", "), names[names.len() - 1]),
        });
    }
    if uses.monitors {
        out.push("Changes your monitors’ settings while a game runs.".into());
    }
    for (address, to) in &uses.network {
        let here = address.starts_with("127.") || address.starts_with("localhost:") || address.starts_with("[::1]");
        out.push(if here { format!("Talks to {to} on this computer.") } else { format!("Talks to {to} at {address}.") });
    }
    if !uses.web.is_empty() {
        let mut whose: Vec<String> = Vec::new();
        for (_, to) in &uses.web {
            if !whose.contains(to) {
                whose.push(to.clone());
            }
        }
        out.push(format!("Reaches {} over the internet.", joined(&whose)));
    }
    if !uses.read.is_empty() {
        out.push(format!("Reads this computer’s own information without changing it: {}.", joined(&uses.read)));
    }
    if uses.nvidia {
        out.push("Reads NVIDIA graphics cards through NVIDIA’s driver.".into());
    }
    if uses.launch {
        out.push("Changes how games start, and runs Wine’s tools.".into());
    }
    for o in &uses.other {
        out.push(match o.as_str() {
            "sound-capture" => "Hears what your computer is playing.".to_string(),
            "network" => "Uses the internet.".to_string(),
            other => format!("Uses {other}."),
        });
    }
    if out.is_empty() {
        out.push("Reaches nothing outside Ectomatter.".into());
    }
    out
}

/// The systems Ectomatter is made for, by the names the standard library
/// and a manifest's `systems` give them, and as people say them.
const SYSTEMS: [(&str, &str); 3] = [("linux", "Linux"), ("windows", "Windows"), ("macos", "macOS")];

/// The systems a manifest says it works on, as people say them: all of
/// them, where it names none.
pub fn systems_of(m: &Value) -> Vec<&'static str> {
    match m["systems"].as_array() {
        Some(named) => SYSTEMS.iter().filter(|(os, _)| named.iter().any(|s| s.as_str() == Some(os))).map(|(_, n)| *n).collect(),
        None => SYSTEMS.iter().map(|(_, n)| *n).collect(),
    }
}

/// Whether a manifest's package works on this system.
pub fn works_here(m: &Value) -> bool {
    m["systems"].as_array().is_none_or(|a| a.iter().any(|s| s.as_str() == Some(std::env::consts::OS)))
}

/// This system, as people say it.
pub fn this_system() -> &'static str {
    SYSTEMS.iter().find(|(os, _)| *os == std::env::consts::OS).map_or(std::env::consts::OS, |(_, n)| n)
}

/// Names in a sentence: "A", "A and B", "A, B and C".
pub fn joined(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// "owner/name", from that or a GitHub address.
pub fn repository_of(s: &str) -> Option<String> {
    let s = s.trim().trim_end_matches('/').trim_end_matches(".git");
    let s = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
    let s = s.strip_prefix("www.").unwrap_or(s);
    let s = s.strip_prefix("github.com/").unwrap_or(s);
    let mut parts = s.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    let ok = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) && p != "." && p != "..";
    (ok(owner) && ok(name)).then(|| format!("{owner}/{name}"))
}

/// Where packages of a kind that were added are kept: one folder for each,
/// a folder for each version in it.
pub fn added_dir(kind: Kind) -> PathBuf {
    crate::data_dir().join("added").join(kind.folder())
}

/// Where the packages of a kind that come with Ectomatter are: the source's own
/// folders, as Ectomatter runs from its source.
pub fn builtin_dir(kind: Kind) -> PathBuf {
    match kind {
        Kind::Core => crate::cores::dir(),
        Kind::Plugin => crate::cores::plugins_dir(),
    }
}

/// A version as numbers, for comparing: "1.10.0" is after "1.9.2".
pub fn version_key(v: &str) -> Vec<u64> {
    v.trim_start_matches('v').split(['.', '-', '+']).map(|p| p.parse::<u64>().unwrap_or(0)).collect()
}

/// Every package of a kind: those that come with Ectomatter, then those added,
/// each id once (the newer version where there are two).
pub fn of(kind: Kind) -> Vec<Package> {
    all_in(&builtin_dir(kind), &added_dir(kind)).into_iter().filter(|p| p.kind() == kind).collect()
}

/// Every package: the cores, then the plugins.
pub fn all() -> Vec<Package> {
    let mut out = of(Kind::Core);
    out.extend(of(Kind::Plugin));
    out
}

pub fn all_in(builtin: &Path, added: &Path) -> Vec<Package> {
    let mut out: Vec<Package> = Vec::new();
    let mut take = |p: Package| match out.iter().position(|q| q.id() == p.id()) {
        Some(i) if version_key(p.version()) > version_key(out[i].version()) => out[i] = p,
        Some(_) => {}
        None => out.push(p),
    };
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(builtin).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.join("core.json").is_file()).collect();
    dirs.sort();
    for d in dirs {
        match Package::read(&d, false) {
            Ok(p) if !p.works_here() => {}
            Ok(p) => take(p),
            Err(e) => eprintln!("ectomatter: {e}"),
        }
    }
    // (an added package's newest version; a version folder Ectomatter didn't finish is left out)
    let mut ids: Vec<PathBuf> = std::fs::read_dir(added).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    ids.sort();
    for id in ids {
        let mut versions: Vec<PathBuf> = std::fs::read_dir(&id).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.join("core.json").is_file() && !p.join(".unfinished").exists()).collect();
        versions.sort_by_key(|p| version_key(&p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
        if let Some(v) = versions.last() {
            match Package::read(v, true) {
                Ok(p) if !p.works_here() => {}
                Ok(p) => take(p),
                Err(e) => eprintln!("ectomatter: {e}"),
            }
        }
    }
    // (the keyboard's and the mouse's cores, which reach their devices themselves, before those through another program; each by name)
    out.sort_by_key(|p| (!p.manifest.has_usb(), p.name().to_lowercase()));
    out
}

/// Where each package's own files were kept before cores and plugins were
/// kept apart (every one in the data folder's cores/, the mouse's core as
/// "steelseries", the Monitors plugin as "monitors", OpenRGB's plugin as
/// "openrgb"), and while the Monitors plugin was named Monitor Configurator,
/// moved to where they're kept now, once.
pub fn move_old_folders() {
    let data = crate::data_dir();
    // (Custom Scripts, a plugin once, is Ectomatter's own now: its empty folder goes)
    let _ = std::fs::remove_dir(data.join("cores").join("scripts"));
    let moves = [
        (data.join("cores").join("steelseries"), data.join("cores").join(crate::mouse::CORE)),
        (data.join("cores").join("monitors"), data.join("plugins").join("monitors")),
        (data.join("plugins").join("monitor-configurator"), data.join("plugins").join("monitors")),
        (data.join("cores").join("openrgb"), data.join("plugins").join("openrgb")),
    ];
    for (from, to) in moves {
        // (a folder already where it goes is taken only if it's empty)
        if from.is_dir() && (!to.exists() || std::fs::remove_dir(&to).is_ok()) {
            if let Some(parent) = to.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::rename(&from, &to) {
                Ok(()) => eprintln!("ectomatter: moved {} to {}", from.display(), to.display()),
                Err(e) => eprintln!("ectomatter: couldn’t move {}: {e}", from.display()),
            }
        }
    }
}

/// A package's manifest read from a package file (an .ectocore, a zip of
/// its folder), without putting it anywhere: for saying what it is and what
/// it can do before it's added.
pub fn manifest_in_file(file: &Path) -> Result<Value, String> {
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|_| "That file isn’t a plugin: it isn’t a zip file.".to_string())?;
    let (root, _) = package_root(&mut z)?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut z.by_name(&format!("{root}core.json")).map_err(|e| e.to_string())?, &mut text).map_err(|e| e.to_string())?;
    ectomatter_draw::jsonc::parse(&text).map_err(|e| format!("Its core.json can’t be read: {e}"))
}

/// Where a package file's core.json is: at its top, or in the one folder at its top.
fn package_root<R: std::io::Read + std::io::Seek>(z: &mut zip::ZipArchive<R>) -> Result<(String, usize), String> {
    let names: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).collect();
    if names.iter().any(|n| n == "core.json") {
        return Ok((String::new(), names.len()));
    }
    let mut roots = names.iter().filter_map(|n| n.split_once('/').map(|(r, _)| r)).collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    match roots.as_slice() {
        [r] if names.iter().any(|n| *n == format!("{r}/core.json")) => Ok((format!("{r}/"), names.len())),
        _ => Err("That file isn’t a plugin: it has no core.json.".into()),
    }
}

/// A package file put in place, as its version's folder under `into`
/// (<id>/<version>/; by default the added folder of the package's kind,
/// see install): every file inside the folder, nothing outside it; the
/// folder marked unfinished until the last file is in. Its manifest.
pub fn install_file(file: &Path, into: &Path) -> Result<Manifest, String> {
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|_| "That file isn’t a plugin: it isn’t a zip file.".to_string())?;
    let (root, n) = package_root(&mut z)?;
    let m = manifest_in_file(file)?;
    let id = m["id"].as_str().filter(|i| !i.is_empty() && i.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))).ok_or("Its core.json names no id Ectomatter can use.")?;
    let version = m["version"].as_str().filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || ".-+".contains(c))).ok_or("Its core.json names no version.")?;
    let dir = into.join(id).join(version);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mark = dir.join(".unfinished");
    std::fs::write(&mark, b"").map_err(|e| e.to_string())?;
    for i in 0..n {
        let mut e = z.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = e.enclosed_name() else { continue };
        let Ok(rel) = name.strip_prefix(root.trim_end_matches('/')) else { continue };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dir.join(rel);
        if !out.starts_with(&dir) {
            continue;
        }
        if e.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut w = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut e, &mut w).map_err(|e| e.to_string())?;
    }
    let manifest = Manifest::read(&dir).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&dir);
    })?;
    if !dir.join(manifest.all["module"].as_str().unwrap_or("core.wasm")).is_file() {
        let _ = std::fs::remove_dir_all(&dir);
        return Err("That plugin is missing its module.".into());
    }
    let _ = std::fs::remove_file(&mark);
    Ok(manifest)
}

/// A package file put in place where packages of its kind that were added are kept.
pub fn install(file: &Path) -> Result<Manifest, String> {
    let kind = Kind::of(&manifest_in_file(file)?);
    install_file(file, &added_dir(kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repository_is_read_from_an_address() {
        assert_eq!(repository_of("OMGrant/ectomatter-monitors").as_deref(), Some("OMGrant/ectomatter-monitors"));
        assert_eq!(repository_of("https://github.com/OMGrant/ectomatter-monitors/").as_deref(), Some("OMGrant/ectomatter-monitors"));
        assert_eq!(repository_of("github.com/a/b.git").as_deref(), Some("a/b"));
        assert_eq!(repository_of("../etc/passwd"), None);
        assert_eq!(repository_of("just-a-name"), None);
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(version_key("1.10.0") > version_key("1.9.2"));
        assert!(version_key("v2.0.0") > version_key("1.99.99"));
        assert_eq!(version_key("1.0.0"), version_key("1.0.0"));
    }

    /// Every package with Ectomatter says what it can do in Ectomatter's words:
    /// the cores their devices, the plugins their uses; and each kind is in
    /// its own folder, the cores in cores/ and the plugins in plugins/.
    /// Each plugin says the systems it works on, a tag each in Settings:
    /// every one, where its manifest names none; and one made for other
    /// systems isn't taken here, and says why.
    #[test]
    fn each_plugin_says_where_it_works() {
        let none = std::env::temp_dir().join(format!("ectomatter-no-packages-{}", std::process::id()));
        let plugins = all_in(&builtin_dir(Kind::Plugin), &none);
        let on = |id: &str| plugins.iter().find(|p| p.id() == id).map(|p| p.systems()).unwrap_or_default();
        assert_eq!(on("weather"), vec!["Linux", "Windows", "macOS"]);
        assert_eq!(on("openrgb"), vec!["Linux", "Windows", "macOS"]);
        assert_eq!(on("sensors"), vec!["Linux"]);
        assert_eq!(on("game-configurator"), vec!["Linux"], "Proton and its prefixes are Linux's");
        assert_eq!(on("monitors"), vec!["Linux"], "Ectomatter reaches the monitors on KDE Plasma alone, so far");
        assert_eq!(systems_of(&serde_json::json!({ "systems": ["macos", "windows"] })), vec!["Windows", "macOS"], "in the tags' order");
        if cfg!(target_os = "linux") {
            let elsewhere = serde_json::json!({ "ectomatter": "0.1.0", "protocol": 4, "systems": ["windows", "macos"] });
            assert_eq!(crate::updates::runs_here(&elsewhere), Err("it works on Windows and macOS, not on Linux".into()));
        }
    }

    #[test]
    fn each_package_says_what_it_can_do() {
        let none = std::env::temp_dir().join(format!("ectomatter-no-packages-{}", std::process::id()));
        let cores = all_in(&builtin_dir(Kind::Core), &none);
        let plugins = all_in(&builtin_dir(Kind::Plugin), &none);
        let can = |list: &[Package], id: &str| list.iter().find(|p| p.id() == id).map(|p| p.can()).unwrap_or_default();
        assert_eq!(can(&cores, "evga-z10"), vec!["Talks to the EVGA Z10 RGB over USB.".to_string()]);
        assert_eq!(can(&cores, crate::mouse::CORE), vec!["Talks to the SteelSeries Sensei 310 over USB.".to_string()]);
        assert_eq!(can(&plugins, "openrgb"), vec!["Talks to OpenRGB on this computer.".to_string()]);
        assert_eq!(can(&plugins, "monitors"), vec!["Changes your monitors’ settings while a game runs.".to_string()]);
        assert!(cores.iter().all(|p| p.kind() == Kind::Core), "the cores folder holds cores");
        assert!(plugins.iter().all(|p| p.kind() == Kind::Plugin), "the plugins folder holds plugins");
        assert_eq!(plugins.iter().map(|p| p.name()).collect::<Vec<_>>(), vec!["Game Configurator", "Monitors", "OpenRGB", "Sensors", "Weather"]);
        assert_eq!(can(&plugins, "game-configurator"), vec!["Changes how games start, and runs Wine’s tools.".to_string()]);
        assert_eq!(can(&plugins, "weather"), vec!["Reaches Open-Meteo, Photon, the National Weather Service, Environment and Climate Change Canada and MeteoAlarm over the internet.".to_string()]);
        assert_eq!(
            can(&plugins, "sensors"),
            vec![
                "Reads this computer’s own information without changing it: /sys, /proc/stat, /proc/meminfo, /proc/cpuinfo and /usr/share/hwdata/pci.ids.".to_string(),
                "Reads NVIDIA graphics cards through NVIDIA’s driver.".to_string(),
            ]
        );
        assert!(cores.iter().chain(&plugins).all(|p| p.glyph().is_some()), "each package brings its glyph");
        // (where each keeps its own files)
        assert!(cores[0].data().ends_with(format!("cores/{}", cores[0].id())));
        assert!(plugins[0].data().ends_with("plugins/game-configurator"));
    }
}
