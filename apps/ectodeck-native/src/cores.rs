//! A device's core, as Ectodeck reads it: its device description (JSON with
//! comments), and the art the description names, from the core's own folder. Ectodeck runs from
//! its source, so the cores are the source's folders; the Light lab saves the
//! light it tunes into the core it came from.

use std::path::PathBuf;
use std::sync::OnceLock;

pub struct Core {
    /// the core's name: its folder's
    pub id: String,
    /// its device description as it was read, where it was read from (where
    /// the Light lab saves), and the art it names
    pub device: serde_json::Value,
    pub path: PathBuf,
    pub art: String,
}

impl Core {
    /// A core by its name. ECTODECK_CORE_DEVICE names another copy of its
    /// device description to read and save instead, for checking the Light
    /// lab's Save without touching the core.
    pub fn load(id: &str) -> Result<Core, String> {
        let folder = dir().join(id);
        let path = std::env::var_os("ECTODECK_CORE_DEVICE").map(PathBuf::from).unwrap_or_else(|| folder.join("device.json"));
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let device = ectodeck_draw::jsonc::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let art_path = folder.join(device["art"].as_str().ok_or("its device description names no art")?);
        let art = std::fs::read_to_string(&art_path).map_err(|e| format!("{}: {e}", art_path.display()))?;
        Ok(Core { id: id.to_string(), device, path, art })
    }

    /// Its art, read as the device is drawn from it.
    pub fn tree(&self) -> resvg::usvg::Tree {
        resvg::usvg::Tree::from_str(&self.art, &resvg::usvg::Options::default()).unwrap_or_else(|e| panic!("the {} core's art: {e}", self.id))
    }

    /// Its typefaces' files: those its art's legends are drawn in.
    pub fn fonts(&self) -> Vec<PathBuf> {
        let folder = dir().join(&self.id);
        self.device["print"]["fonts"].as_array().into_iter().flatten().filter_map(|f| f.as_str()).map(|f| folder.join(f)).collect()
    }

    /// What the device is called.
    pub fn name(&self) -> &str {
        self.device["name"].as_str().unwrap_or(&self.id)
    }

    /// Its device description as its file has it now, which the Light lab
    /// may have saved into since it was read.
    pub fn device_now(&self) -> serde_json::Value {
        std::fs::read_to_string(&self.path).ok().and_then(|t| ectodeck_draw::jsonc::parse(&t).ok()).unwrap_or_else(|| self.device.clone())
    }
}

/// Where the cores are: the source's own folder, as Ectodeck runs from it.
pub fn dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../cores"))
}

/// The core of the device Ectodeck shows, read once. Ectodeck shows one
/// device so far, the Z10, whose page names its core.
pub fn shown() -> &'static Core {
    static SHOWN: OnceLock<Core> = OnceLock::new();
    SHOWN.get_or_init(|| Core::load(crate::z10::CORE).unwrap_or_else(|e| panic!("the {} core: {e}", crate::z10::CORE)))
}
