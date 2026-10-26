//! This computer as a package reaches it through Ectomatter, beyond its USB
//! devices: its monitors, its files (read only), NVIDIA's driver, and the
//! web. core_host.rs checks what a package's manifest allows before asking
//! any of these; this only does what's asked. For a check run offscreen, the
//! monitors are those in the file ECTOMATTER_MONITORS names.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use ectomatter_sensors::list::{Nv, Nvidia, Val};

/// The most of a file or a web answer a package is handed.
const MOST: u64 = 8 << 20;

pub struct ThisComputer {
    nvidia: Mutex<crate::nvml::Nvml>,
}

impl ThisComputer {
    pub fn new() -> ThisComputer {
        ThisComputer { nvidia: Mutex::new(crate::nvml::Nvml::default()) }
    }
}

/// A reading of NVIDIA's driver by the word the system interface names it with.
fn nv_of(what: &str) -> Option<Nv> {
    Some(match what {
        "temp" => Nv::Temp,
        "load" => Nv::Load,
        "clock" => Nv::Clock,
        "mem-clock" => Nv::MemClock,
        "vram" => Nv::Vram,
        "power" => Nv::Power,
        "fan" => Nv::Fan,
        _ => return None,
    })
}

/// The word for a reading of NVIDIA's driver, as the system interface names it.
pub fn nv_word(n: Nv) -> &'static str {
    match n {
        Nv::Temp => "temp",
        Nv::Load => "load",
        Nv::Clock => "clock",
        Nv::MemClock => "mem-clock",
        Nv::Vram => "vram",
        Nv::Power => "power",
        Nv::Fan => "fan",
    }
}

impl crate::core_host::Computer for ThisComputer {
    fn monitors(&self) -> Result<Value, String> {
        if let Some(f) = std::env::var_os("ECTOMATTER_MONITORS") {
            let t = std::fs::read_to_string(&f).map_err(|e| e.to_string())?;
            return serde_json::from_str(&t).map_err(|e| e.to_string());
        }
        crate::monitors::list().map(|l| crate::monitors::to_json(&l))
    }

    fn read(&self, path: &Path) -> Result<String, String> {
        use std::io::Read;
        let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut text = String::new();
        f.take(MOST + 1).read_to_string(&mut text).map_err(|e| e.to_string())?;
        if text.len() as u64 > MOST {
            return Err(format!("{} is larger than Ectomatter hands a package", path.display()));
        }
        Ok(text)
    }

    fn entries(&self, path: &Path) -> Result<Vec<String>, String> {
        let mut names: Vec<String> = std::fs::read_dir(path).map_err(|e| e.to_string())?.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        names.sort();
        Ok(names)
    }

    fn resolve(&self, path: &Path) -> Result<PathBuf, String> {
        std::fs::canonicalize(path).map_err(|e| e.to_string())
    }

    fn nvidia(&self, pci: &str, what: &str) -> Result<Value, String> {
        let mut nv = self.nvidia.lock().unwrap_or_else(|p| p.into_inner());
        if what == "probe" {
            let p = nv.probe(pci).ok_or("NVIDIA’s driver doesn’t know that card")?;
            return Ok(json!({ "has": p.has.iter().map(|n| nv_word(*n)).collect::<Vec<_>>(), "vram": p.vram }));
        }
        let n = nv_of(what).ok_or_else(|| format!("NVIDIA’s driver has no reading called {what}"))?;
        match nv.read(pci, n).ok_or("NVIDIA’s driver gave no reading")? {
            Val::One(v) => Ok(json!(v)),
            Val::Of(u, t) => Ok(json!([u, t])),
        }
    }

    fn fetch(&self, url: &str, timeout: Duration) -> Result<String, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(timeout)).user_agent("Ectomatter (github.com/Ectonomic/Ectomatter)").build().into();
        let mut answer = agent.get(url).call().map_err(|e| match e {
            ureq::Error::StatusCode(code) => format!("{url} answered {code}"),
            e => e.to_string(),
        })?;
        answer.body_mut().with_config().limit(MOST).read_to_string().map_err(|e| e.to_string())
    }
}
