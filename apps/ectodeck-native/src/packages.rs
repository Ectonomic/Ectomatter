//! Ectodeck's packages: each device's core and each plugin, one kind of
//! package (a folder holding its manifest, core.json, the module it names,
//! and its glyph), listed together in Settings as Plugins. Some come with
//! Ectodeck (the source's cores folder, as Ectodeck runs from its source);
//! the rest were added, from a plugin's GitHub releases or a file, into
//! Ectodeck's data folder (plugins/<id>/<version>/), where an update lands
//! too. Where one of each has the same id, the newer version is the one used.
//!
//! What a package can do on the computer is said in Ectodeck's own words,
//! from what its manifest declares (its USB devices and its `uses`), never in
//! the package's: a package can't describe itself as doing less than it may.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::core_host::Manifest;

/// Whether a package is a device's core (it reaches USB devices, or finds
/// its devices through another program, as OpenRGB's does) or a plugin that
/// only adds to Ectodeck's pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Device,
    Plugin,
}

#[derive(Clone)]
pub struct Package {
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// added, rather than come with Ectodeck
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

    pub fn kind(&self) -> Kind {
        if self.manifest.has_usb() || self.manifest.all["devices"].is_array() || !self.manifest.uses.network.is_empty() && !self.adds_to_games() {
            Kind::Device
        } else {
            Kind::Plugin
        }
    }

    /// What it can do on the computer, a sentence each, in Ectodeck's words.
    pub fn can(&self) -> Vec<String> {
        can_words(&self.manifest.all)
    }

    /// The setting that turns it on or off. The keyboard's core is the
    /// keyboard's own Control, which Settings' Devices shows too.
    pub fn switch(&self) -> String {
        if self.id() == crate::z10::CORE {
            "devices.z10.control".into()
        } else {
            format!("plugins.{}.on", self.id())
        }
    }
}

/// What a package can do on the computer, a sentence each, in Ectodeck's
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
    if uses.commands {
        out.push("Runs the commands you type, and nothing else.".into());
    }
    for (address, to) in &uses.network {
        let here = address.starts_with("127.") || address.starts_with("localhost:") || address.starts_with("[::1]");
        out.push(if here { format!("Talks to {to} on this computer.") } else { format!("Talks to {to} at {address}.") });
    }
    for o in &uses.other {
        out.push(match o.as_str() {
            "sound-capture" => "Hears what your computer is playing.".to_string(),
            "network" => "Uses the internet.".to_string(),
            other => format!("Uses {other}."),
        });
    }
    if out.is_empty() {
        out.push("Reaches nothing outside Ectodeck.".into());
    }
    out
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

/// Where packages that were added are kept: one folder for each, a folder
/// for each version in it.
pub fn added_dir() -> PathBuf {
    crate::data_dir().join("plugins")
}

/// A version as numbers, for comparing: "1.10.0" is after "1.9.2".
pub fn version_key(v: &str) -> Vec<u64> {
    v.trim_start_matches('v').split(['.', '-', '+']).map(|p| p.parse::<u64>().unwrap_or(0)).collect()
}

/// Every package: those that come with Ectodeck, then those added, each id
/// once (the newer version where there are two).
pub fn all() -> Vec<Package> {
    all_in(&crate::cores::dir(), &added_dir())
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
            Ok(p) => take(p),
            Err(e) => eprintln!("ectodeck-native: {e}"),
        }
    }
    // (an added package's newest version; a version folder Ectodeck didn't finish is left out)
    let mut ids: Vec<PathBuf> = std::fs::read_dir(added).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    ids.sort();
    for id in ids {
        let mut versions: Vec<PathBuf> = std::fs::read_dir(&id).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.join("core.json").is_file() && !p.join(".unfinished").exists()).collect();
        versions.sort_by_key(|p| version_key(&p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
        if let Some(v) = versions.last() {
            match Package::read(v, true) {
                Ok(p) => take(p),
                Err(e) => eprintln!("ectodeck-native: {e}"),
            }
        }
    }
    // the devices' cores first, then the plugins, each by name
    out.sort_by_key(|p| (!p.manifest.has_usb(), p.name().to_lowercase()));
    out
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
    ectodeck_draw::jsonc::parse(&text).map_err(|e| format!("Its core.json can’t be read: {e}"))
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
/// (plugins/<id>/<version>/): every file inside the folder, nothing outside
/// it; the folder marked unfinished until the last file is in. Its manifest.
pub fn install_file(file: &Path, into: &Path) -> Result<Manifest, String> {
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|_| "That file isn’t a plugin: it isn’t a zip file.".to_string())?;
    let (root, n) = package_root(&mut z)?;
    let m = manifest_in_file(file)?;
    let id = m["id"].as_str().filter(|i| !i.is_empty() && i.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))).ok_or("Its core.json names no id Ectodeck can use.")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repository_is_read_from_an_address() {
        assert_eq!(repository_of("OMGrant/ectodeck-monitors").as_deref(), Some("OMGrant/ectodeck-monitors"));
        assert_eq!(repository_of("https://github.com/OMGrant/ectodeck-monitors/").as_deref(), Some("OMGrant/ectodeck-monitors"));
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

    /// Every package with Ectodeck says what it can do in Ectodeck's words:
    /// the cores their USB devices, the plugins their uses.
    #[test]
    fn each_package_says_what_it_can_do() {
        let none = std::env::temp_dir().join(format!("ectodeck-no-plugins-{}", std::process::id()));
        let all = all_in(&crate::cores::dir(), &none);
        let can = |id: &str| all.iter().find(|p| p.id() == id).map(|p| p.can()).unwrap_or_default();
        assert_eq!(can("evga-z10"), vec!["Talks to the EVGA Z10 RGB over USB.".to_string()]);
        assert_eq!(can("scripts"), vec!["Runs the commands you type, and nothing else.".to_string()]);
        assert_eq!(can("monitors"), vec!["Changes your monitors’ settings while a game runs.".to_string()]);
        let kinds: Vec<(&str, Kind)> = all.iter().map(|p| (p.id(), p.kind())).collect();
        assert!(kinds.contains(&("evga-z10", Kind::Device)) && kinds.contains(&("scripts", Kind::Plugin)) && kinds.contains(&("monitors", Kind::Plugin)));
        assert!(all.iter().all(|p| p.glyph().is_some()), "each package brings its glyph");
    }
}
