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
}

impl Core {
    /// A core from its package folder: the device description its manifest
    /// names. ECTOMATTER_CORE_DEVICE names another copy of the description
    /// to read and save instead, for checking the Light lab's Save without
    /// touching the core.
    pub fn load(folder: &Path) -> Result<Core, String> {
        let manifest = crate::core_host::Manifest::read(folder)?;
        let file = manifest.all["deviceFile"].as_str().ok_or_else(|| format!("the {} core names no device description", manifest.id))?;
        let path = std::env::var_os("ECTOMATTER_CORE_DEVICE").map(PathBuf::from).unwrap_or_else(|| folder.join(file));
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let device = ectomatter_draw::jsonc::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let art_path = folder.join(device["art"].as_str().ok_or("its device description names no art")?);
        let art = std::fs::read_to_string(&art_path).map_err(|e| format!("{}: {e}", art_path.display()))?;
        Ok(Core { id: manifest.id, folder: folder.to_path_buf(), device, path, art })
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

    /// What the device is called.
    pub fn name(&self) -> &str {
        self.device["name"].as_str().unwrap_or(&self.id)
    }

    /// Its device description as its file has it now, which the lab
    /// may have saved into since it was read.
    pub fn device_now(&self) -> serde_json::Value {
        std::fs::read_to_string(&self.path).ok().and_then(|t| ectomatter_draw::jsonc::parse(&t).ok()).unwrap_or_else(|| self.device.clone())
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

/// Where the cores that come with Ectomatter are.
pub fn dir() -> PathBuf {
    home().join("cores")
}

/// Where the plugins that come with Ectomatter are, apart from the cores.
pub fn plugins_dir() -> PathBuf {
    home().join("plugins")
}

/// The keyboard Ectomatter shows, read once: the first core whose device
/// description says it's a keyboard, of those Ectomatter has (packages.rs
/// orders them). None when there's none, and Ectomatter runs without a
/// keyboard's page.
pub fn keyboard() -> Option<&'static Core> {
    static SHOWN: OnceLock<Option<Core>> = OnceLock::new();
    SHOWN
        .get_or_init(|| {
            crate::packages::of(crate::packages::Kind::Core).into_iter().filter(|p| p.manifest.all["deviceFile"].is_string()).find_map(|p| match Core::load(&p.dir) {
                Ok(c) if c.device["kind"] == "keyboard" => Some(c),
                Ok(_) => None,
                Err(e) => {
                    eprintln!("ectomatter: {e}");
                    None
                }
            })
        })
        .as_ref()
}
