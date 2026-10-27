//! A device's core, as Ectomatter reads it: its device description (JSON with
//! comments), the file its manifest names (`deviceFile`), and the art the
//! description names, from the core's own folder.
//!
//! The packages that come with Ectomatter are found as it runs (`home`), never
//! through a path built into it, so a copy installed anywhere finds its own.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub struct Core {
    /// the core's id, as its manifest gives it, and its package's folder
    pub id: String,
    pub folder: PathBuf,
    /// its device description as it was read, where it was read from (where
    /// the lab saves), and the art it names
    pub device: serde_json::Value,
    pub path: PathBuf,
    pub art: String,
    /// what the core says of the device as it runs (`device/describe`): its
    /// capabilities, so its profiles, its lights and its displays; set once
    /// it's said (`describe`)
    described: OnceLock<serde_json::Value>,
}

impl Core {
    /// A core from its package folder: the device description its manifest
    /// names. ECTOMATTER_CORE_DEVICE names another copy of the description
    /// to read and save instead, for checking the Light lab's Save without
    /// touching the core.
    pub fn load(folder: &Path) -> Result<Core, String> {
        let manifest = crate::core_host::Manifest::read(folder)?;
        let file = manifest.spec.device_file.as_deref().ok_or_else(|| format!("the {} core names no device description", manifest.id))?;
        let path = std::env::var_os("ECTOMATTER_CORE_DEVICE").map(PathBuf::from).unwrap_or_else(|| folder.join(file));
        let read = tuning_read(&manifest.id, &path);
        let text = std::fs::read_to_string(&read).map_err(|e| format!("{}: {e}", read.display()))?;
        let device = ectomatter_draw::jsonc::parse(&text).map_err(|e| format!("{}: {e}", read.display()))?;
        let art_path = folder.join(device["art"].as_str().ok_or("its device description names no art")?);
        let art = std::fs::read_to_string(&art_path).map_err(|e| format!("{}: {e}", art_path.display()))?;
        Ok(Core { id: manifest.id, folder: folder.to_path_buf(), device, path, art, described: OnceLock::new() })
    }

    /// Its art, read as the device is drawn from it.
    pub fn tree(&self) -> resvg::usvg::Tree {
        resvg::usvg::Tree::from_str(&self.art, &resvg::usvg::Options::default()).unwrap_or_else(|e| panic!("the {} core's art: {e}", self.id))
    }

    /// Its typefaces' files: those its art's legends are drawn in.
    pub fn fonts(&self) -> Vec<PathBuf> {
        let folder = &self.folder;
        self.device["print"]["fonts"].as_array().into_iter().flatten().filter_map(|f| f.as_str()).map(|f| folder.join(f)).collect()
    }

    /// What its core says of the device as it runs, kept from the first time
    /// it's said: the keyboard's page, its screen and its counts are drawn
    /// from it.
    pub fn describe(&self, described: &serde_json::Value) {
        if described.is_object() {
            let _ = self.described.set(described.clone());
        }
    }

    pub fn described(&self) -> &serde_json::Value {
        static NONE: serde_json::Value = serde_json::Value::Null;
        self.described.get().unwrap_or(&NONE)
    }

    /// Its profiles, by number from 1, as many as its core describes.
    pub fn profiles(&self) -> Vec<u8> {
        let count = self.described()["capabilities"]["profiles"]["count"].as_u64().unwrap_or(0).min(u8::MAX as u64) as u8;
        (1..=count).collect()
    }

    /// How many controls it lights one by one: a color for each in the
    /// order of their places (`lighting`, `direct`), or one for each control
    /// with a place where the core doesn't say.
    pub fn places(&self) -> usize {
        self.described()["capabilities"]["lighting"]["direct"]["controls"].as_u64().map(|n| n as usize).unwrap_or_else(|| self.device["controls"].as_array().map_or(0, |c| c.iter().filter_map(|c| c["pos"].as_u64()).max().map_or(0, |m| m as usize + 1)))
    }

    /// What the device is called.
    pub fn name(&self) -> &str {
        self.device["name"].as_str().unwrap_or(&self.id)
    }

    /// Its device description as its file has it now, which the lab
    /// may have saved into since it was read.
    pub fn device_now(&self) -> serde_json::Value {
        std::fs::read_to_string(tuning_read(&self.id, &self.path)).ok().and_then(|t| ectomatter_draw::jsonc::parse(&t).ok()).unwrap_or_else(|| self.device.clone())
    }
}

/// The folder holding the packages that come with Ectomatter, its `cores`
/// and `plugins`, found once as it runs: the one ECTOMATTER_PACKAGES names;
/// else the source folder a build runs from (a folder above the program with
/// the workspace's Cargo.toml and `cores`, as `target/release/ectomatter`
/// has); else `share/ectomatter` beside the program's own folder, as an
/// install lays it out (`/usr/bin/ectomatter`, `/usr/share/ectomatter`); else
/// the system's data folders' (XDG_DATA_DIRS). Where none has them, the
/// install's place, with nothing in it.
pub fn home() -> &'static Path {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let home = find_home();
        eprintln!("ectomatter: its packages are in {}", home.display());
        home
    })
}

fn find_home() -> PathBuf {
    if let Some(p) = std::env::var_os("ECTOMATTER_PACKAGES") {
        return PathBuf::from(p);
    }
    let exe = std::env::current_exe().ok().and_then(|e| e.canonicalize().ok()).unwrap_or_default();
    let source = exe.ancestors().skip(1).find(|d| d.join("Cargo.toml").is_file() && d.join("cores").is_dir() && d.join("apps/ectomatter").is_dir());
    let installed = exe.parent().and_then(Path::parent).map(|d| d.join("share/ectomatter")).unwrap_or_default();
    let system = std::env::var("XDG_DATA_DIRS").ok().filter(|d| !d.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let has = |d: &Path| d.join("cores").is_dir() || d.join("plugins").is_dir();
    source
        .map(Path::to_path_buf)
        .or_else(|| has(&installed).then(|| installed.clone()))
        .or_else(|| system.split(':').map(|d| Path::new(d).join("ectomatter")).find(|d| has(d)))
        .unwrap_or(installed)
}

/// Whether Ectomatter runs from its source folder (its packages found there,
/// as a build from it finds them): a developer's, where the lab's tuning is
/// saved into each core's own file and ships as the core's default.
pub fn from_source() -> bool {
    let h = home();
    h.join("Cargo.toml").is_file() && h.join("apps/ectomatter").is_dir()
}

/// Where the lab keeps a core's file it tunes (its device's description, or
/// a mouse's light): the core's own file, run from the source folder; else a
/// copy in the person's own data folder, the core's file left as it came.
pub fn tuning_written(core: &str, shipped: &Path) -> PathBuf {
    tuning_at(from_source(), &crate::data_dir(), core, shipped)
}

/// The same, for Ectomatter run from its source folder or not (`source`),
/// with the person's data folder `data`.
fn tuning_at(source: bool, data: &Path, core: &str, shipped: &Path) -> PathBuf {
    if source {
        return shipped.to_path_buf();
    }
    let name = shipped.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    data.join("tuning").join(core).join(name)
}

/// Where a core's file the lab tunes is read from: the person's own copy,
/// where the lab has saved one, else the core's.
pub fn tuning_read(core: &str, shipped: &Path) -> PathBuf {
    let own = tuning_written(core, shipped);
    if own.is_file() { own } else { shipped.to_path_buf() }
}

/// Where the cores that come with Ectomatter are.
pub fn dir() -> PathBuf {
    home().join("cores")
}

/// Where the plugins that come with Ectomatter are, apart from the cores.
pub fn plugins_dir() -> PathBuf {
    home().join("plugins")
}

/// The keyboard's key, what Ectomatter keeps its jobs, scenes, Library
/// choices and place on the desk under: its device's id as its core names it
/// ("" with no keyboard).
pub fn keyboard_key() -> &'static str {
    keyboard().and_then(|k| k.described()["device"].as_str()).unwrap_or("")
}

/// Where a setting of the keyboard's is kept in Ectomatter's settings: under
/// its device's id, as its core describes it ("devices.<id>.<rest>").
pub fn keyboard_setting(rest: &str) -> String {
    format!("devices.{}.{rest}", keyboard_key())
}

/// The keyboard's profiles, by number (none with no keyboard).
pub fn profiles() -> Vec<u8> {
    keyboard().map(Core::profiles).unwrap_or_default()
}

/// How many of the keyboard's controls are lit one by one (none with no keyboard).
pub fn places() -> usize {
    keyboard().map_or(0, Core::places)
}

/// The keyboard Ectomatter shows, read once: the first core whose device
/// description says it's a keyboard, of those Ectomatter has (packages.rs
/// orders them). None when there's none, and Ectomatter runs without a
/// keyboard's page.
pub fn keyboard() -> Option<&'static Core> {
    static SHOWN: OnceLock<Option<Core>> = OnceLock::new();
    SHOWN
        .get_or_init(|| {
            crate::packages::of(crate::packages::Kind::Core).into_iter().filter(|p| p.manifest.spec.device_file.is_some()).find_map(|p| match Core::load(&p.dir) {
                Ok(c) if c.device["kind"] == "keyboard" => {
                    // (a test reads it as its core, run with no device, describes it, as a check offscreen does)
                    #[cfg(test)]
                    c.describe(&described_alone(&p));
                    Some(c)
                }
                Ok(_) => None,
                Err(e) => {
                    eprintln!("ectomatter: {e}");
                    None
                }
            })
        })
        .as_ref()
}

/// What a core describes, run alone with no device, for a test.
#[cfg(test)]
fn described_alone(p: &crate::packages::Package) -> serde_json::Value {
    let d = crate::core_host::described_alone(&p.dir, std::time::Duration::from_secs(60));
    assert!(d.is_object(), "the keyboard's core describes it");
    d
}

#[cfg(test)]
mod tuning_tests {
    use super::*;

    /// The lab's tuning goes into the core's own file run from the source
    /// folder, and into the person's own folder otherwise, never into an
    /// installed core's.
    #[test]
    fn an_installed_cores_tuning_is_kept_in_the_persons_folder() {
        let shipped = Path::new("/usr/share/ectomatter/cores/evga-z10/device.json");
        let data = Path::new("/home/someone/.local/share/com.ectonomic.ectomatter");
        assert_eq!(tuning_at(true, data, "evga-z10", shipped), shipped);
        let own = tuning_at(false, data, "evga-z10", shipped);
        assert_eq!(own, data.join("tuning/evga-z10/device.json"));
        assert!(!own.starts_with("/usr"), "never into the installed core");
    }
}
