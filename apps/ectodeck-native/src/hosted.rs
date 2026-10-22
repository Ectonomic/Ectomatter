//! The packages Ectodeck runs itself (packages.rs), each started as a core
//! is (core_host.rs) while it's on, and kept in two lists, as Settings keeps
//! them: the cores that have no page of their own, and the plugins. A plugin
//! describes its section of a game's page in the shared settings language
//! (game/describe), and says what a game's choices change as the game starts
//! (game/changes); Ectodeck draws the one and makes the other (putback.rs).
//! A plugin may also bring in what another program drives, as OpenRGB's
//! does: it tells its devices as a core does, each with its profiles, for
//! the Library's Devices.
//!
//! The keyboard's and the mouse's cores are started where their pages are
//! (main.rs); this is for the rest.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::core_host::{Computer, Core, NoDevices};
use crate::packages::{Kind, Package};

/// A package running: its package, its core, what it said last of how it is
/// (a warning to show beside it, such as a server it can't reach), and, for
/// a core, the devices it told, each as it told it.
pub struct Run {
    pub package: Package,
    pub core: Core,
    pub warning: Option<String>,
    pub devices: Vec<Value>,
    /// its sections, as last described for each page, and when
    described: Vec<(String, Value, Instant)>,
    /// the folder it was given offscreen or in a test, removed when it stops
    scratch: Option<PathBuf>,
}

impl Drop for Run {
    fn drop(&mut self) {
        if let Some(d) = &self.scratch {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

pub struct Hosted {
    pub cores: Vec<Run>,
    pub plugins: Vec<Run>,
    computer: Arc<dyn Computer>,
}

/// Packages Ectodeck starts elsewhere: the keyboard's and the mouse's cores.
pub fn started_elsewhere(id: &str) -> bool {
    id == crate::z10::CORE || id == crate::mouse::CORE
}

impl Hosted {
    pub fn new(computer: Arc<dyn Computer>) -> Hosted {
        Hosted { cores: Vec::new(), plugins: Vec::new(), computer }
    }

    fn all(&self) -> impl Iterator<Item = &Run> {
        self.cores.iter().chain(self.plugins.iter())
    }

    /// Each package that's on and isn't started elsewhere, started (offscreen
    /// too: a page's sections are drawn from what plugins describe; none of
    /// them changes anything there, nothing being made offscreen).
    pub fn start_all(&mut self, settings: &crate::settings::Settings) {
        for p in crate::packages::all() {
            if started_elsewhere(p.id()) || !settings.on(&p.switch()) {
                continue;
            }
            self.start(p);
        }
    }

    pub fn start(&mut self, p: Package) {
        if self.get(p.id()).is_some() {
            return;
        }
        let kind = p.kind();
        // (offscreen or in a test, a folder of its own that's gone after, so a check keeps nothing)
        let scratch = crate::desk::keeps_nothing().then(|| std::env::temp_dir().join(format!("ectodeck-{}-{}-{}", kind.word(), p.id(), std::process::id())));
        let data = scratch.clone().unwrap_or_else(|| p.data());
        match Core::start_with(&p.dir, data, Box::new(NoDevices), Some(self.computer.clone())) {
            Ok(core) => {
                match core.call("initialize", &json!({ "protocol": 4, "frontend": concat!("ectodeck ", env!("CARGO_PKG_VERSION")), "platform": platform() })) {
                    Ok(_) => eprintln!("ectodeck-native: the {} {} started", p.id(), kind.word()),
                    Err(e) => eprintln!("ectodeck-native: the {} {}: {e}", p.id(), kind.word()),
                }
                let run = Run { package: p, core, warning: None, devices: Vec::new(), described: Vec::new(), scratch };
                match kind {
                    Kind::Core => self.cores.push(run),
                    Kind::Plugin => self.plugins.push(run),
                }
            }
            Err(e) => {
                eprintln!("ectodeck-native: the {} {} wouldn’t start: {e}", p.id(), kind.word());
                if let Some(d) = scratch {
                    let _ = std::fs::remove_dir_all(d);
                }
            }
        }
    }

    /// A package turned off: stopped, a core's devices gone with it.
    pub fn stop(&mut self, id: &str) {
        for list in [&mut self.cores, &mut self.plugins] {
            if let Some(i) = list.iter().position(|x| x.package.id() == id) {
                let p = list.remove(i);
                let _ = p.core.call("shutdown", &Value::Null);
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&Run> {
        self.all().find(|p| p.package.id() == id)
    }

    /// What the packages running told since last asked: their warnings, and
    /// the devices they brought in. True if anything changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for p in self.cores.iter_mut().chain(self.plugins.iter_mut()) {
            for (method, params) in p.core.told() {
                match method.as_str() {
                    "plugin/status" => {
                        let w = params["warning"].as_str().filter(|w| !w.is_empty()).map(String::from);
                        if w != p.warning {
                            p.warning = w;
                            changed = true;
                        }
                    }
                    "device/added" => {
                        let id = params["device"].clone();
                        p.devices.retain(|d| d["device"] != id);
                        p.devices.push(params);
                        changed = true;
                    }
                    "device/removed" => {
                        let id = params["device"].clone();
                        p.devices.retain(|d| d["device"] != id);
                        changed = true;
                    }
                    "game/changed" => {
                        p.described.clear();
                        changed = true;
                    }
                    _ => {}
                }
            }
        }
        changed
    }

    /// The plugins whose sections a page gets, in Settings' order: each one's
    /// id and its section, as it describes it for this page (`page` is
    /// "game", "app" or "every"). A description a few seconds old is asked again.
    pub fn sections(&mut self, page: &str, name: &str) -> Vec<(String, Value)> {
        let key = format!("{page}\u{0}{name}");
        let mut out = Vec::new();
        for p in &mut self.plugins {
            if !p.package.adds_to_games() {
                continue;
            }
            let fresh = p.described.iter().find(|(k, _, t)| *k == key && t.elapsed() < Duration::from_secs(3)).map(|(_, v, _)| v.clone());
            let section = match fresh {
                Some(s) => s,
                None => {
                    let s = p.core.call("game/describe", &json!({ "for": page, "name": name })).map(|v| v["section"].clone()).unwrap_or_else(|e| json!({ "label": p.package.name(), "layout": [{ "note": e }] }));
                    p.described.retain(|(k, _, _)| *k != key);
                    p.described.push((key.clone(), s.clone(), Instant::now()));
                    s
                }
            };
            if section.is_object() {
                out.push((p.package.id().to_string(), section));
            }
        }
        out
    }

    /// Asked again next time, every plugin's sections (a page opened afresh).
    pub fn forget_sections(&mut self) {
        for p in &mut self.plugins {
            p.described.clear();
        }
    }

    /// What each plugin changes as a game starts, in Settings' order, each
    /// change checked against what its plugin's manifest allows: `values`
    /// are each plugin's choices for the game (its own over Every game's),
    pub fn changes(&self, game: &crate::putback::Game, kind: &str, values: &dyn Fn(&str) -> Value) -> Vec<(String, Value, String, String)> {
        let mut out = Vec::new();
        for p in &self.plugins {
            if !p.package.adds_to_games() {
                continue;
            }
            let uses = &p.package.manifest.uses;
            let asked = json!({ "game": { "id": game.id, "name": game.name, "kind": kind }, "values": values(p.package.id()) });
            let answer = match p.core.call("game/changes", &asked) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("ectodeck-native: the {} plugin’s changes for {}: {e}", p.package.id(), game.name);
                    continue;
                }
            };
            for c in answer["changes"].as_array().into_iter().flatten() {
                let what = c["change"].clone();
                // (a change of a kind its manifest doesn't name is left unmade)
                let allowed = what.get("monitor").is_some() && uses.monitors;
                if !allowed {
                    eprintln!("ectodeck-native: the {} plugin asked for a change it may not make: {what}", p.package.id());
                    continue;
                }
                let label: String = c["label"].as_str().unwrap_or("").chars().take(20).collect();
                out.push((p.package.id().to_string(), what, label, c["words"].as_str().unwrap_or("").to_string()));
            }
        }
        out
    }

    /// The devices the packages running here told (a plugin's, from the
    /// program it reaches, as OpenRGB's), each by its package and as it told it.
    pub fn devices(&self) -> Vec<(String, Value)> {
        self.all().flat_map(|p| p.devices.iter().map(|d| (p.package.id().to_string(), d.clone()))).collect()
    }
}

/// The system Ectodeck runs on, as the protocol's initialize names it.
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Monitors as a fake computer has them: Grant's two.
    pub struct Desk;

    impl Computer for Desk {
        fn monitors(&self) -> Result<Value, String> {
            Ok(json!([
                { "id": "edid:MSI-3DA4-1", "name": "Wide 340 OLED", "maker": "MSI", "port": "DP-1", "enabled": true, "main": true,
                  "modes": [{ "width": 3440, "height": 1440, "refresh": 239999 }, { "width": 3440, "height": 1440, "refresh": 164990 }, { "width": 3440, "height": 1440, "refresh": 59970 }, { "width": 2560, "height": 1440, "refresh": 120000 }, { "width": 2560, "height": 1440, "refresh": 59951 }],
                  "mode": { "width": 3440, "height": 1440, "refresh": 239999 }, "hdr": true, "vrr": "automatic", "brightness": 10000, "ddc": true },
                { "id": "edid:WSD-0001-2", "name": "Side 13 2K", "maker": "", "port": "DP-6", "enabled": true, "main": false,
                  "modes": [{ "width": 2560, "height": 1440, "refresh": 59998 }, { "width": 1920, "height": 1080, "refresh": 60000 }],
                  "mode": { "width": 2560, "height": 1440, "refresh": 59998 }, "brightness": 3500, "ddc": false }
            ]))
        }
    }

    /// One package that comes with Ectodeck, started alone.
    fn hosted(kind: Kind, id: &str) -> Hosted {
        let mut ps = Hosted::new(Arc::new(Desk));
        let p = crate::packages::Package::read(&crate::packages::builtin_dir(kind).join(id), false).unwrap();
        assert!(p.dir.join("core.wasm").exists(), "the {id} {} isn't built: run tools/build-{}.sh", kind.word(), kind.folder());
        ps.start(p);
        assert_eq!(ps.cores.len() + ps.plugins.len(), 1, "it started");
        ps
    }

    fn plugin(id: &str) -> Hosted {
        hosted(Kind::Plugin, id)
    }

    /// The Monitor Configurator's section: a column for each monitor, a cell
    /// saying why where a monitor can't do something, and the main monitor.
    #[test]
    fn the_monitors_section_has_a_column_for_each() {
        let mut ps = plugin("monitor-configurator");
        let s = &ps.sections("game", "ARC Raiders")[0].1;
        let grid = &s["layout"][0]["grid"];
        let cols: Vec<&str> = grid["columns"].as_array().unwrap().iter().map(|c| c["label"].as_str().unwrap()).collect();
        assert_eq!(cols, vec!["MSI Wide 340 OLED", "Side 13 2K"]);
        assert_eq!(grid["columns"][0]["note"], "3440 × 1440, 240 Hz, HDR · Main");
        let row = |label: &str| grid["rows"].as_array().unwrap().iter().find(|r| r["label"] == label).cloned().unwrap();
        assert_eq!(row("Refresh rate")["cells"][1]["none"], "Only 60 Hz");
        assert_eq!(row("HDR")["cells"][1]["none"], "No HDR");
        let side_bright = row("Brightness")["cells"][1].as_str().unwrap().to_string();
        let labels: Vec<&str> = s["settings"][&side_bright]["options"].as_array().unwrap().iter().map(|o| o["label"].as_str().unwrap()).collect();
        assert_eq!(labels, vec!["Leave as is", "100%", "75%", "50%", "25%", "Lowest", "Turn off"]);
        assert_eq!(s["layout"][1]["show"][0]["id"], "main");
        assert!(s["layout"][1]["note"].as_str().unwrap().starts_with("Ectodeck sets these when ARC Raiders starts"));
    }

    /// A game's choices become changes, in order, each a change of a monitor;
    /// one the monitor is already at changes nothing.
    #[test]
    fn choices_become_changes() {
        let ps = plugin("monitor-configurator");
        let g = crate::putback::Game { id: "steam_app_1808500".into(), name: "ARC Raiders".into(), pid: 1, since: 1 };
        let values = |_: &str| json!({ "edid:MSI-3DA4-1/brightness": "100", "edid:WSD-0001-2/brightness": "lowest", "edid:MSI-3DA4-1/refresh": "165", "main": "edid:MSI-3DA4-1" });
        let c = ps.changes(&g, "game", &values);
        let whats: Vec<Value> = c.iter().map(|x| x.1.clone()).collect();
        assert_eq!(whats, vec![
            json!({ "monitor": "edid:MSI-3DA4-1", "set": "mode", "to": { "width": 3440, "height": 1440, "refresh": 164990 } }),
            json!({ "monitor": "edid:WSD-0001-2", "set": "brightness", "to": 0 }),
        ]);
        assert_eq!(c[1].2, "BRIGHTNESS");
        assert_eq!(c[1].3, "the Side 13 2K’s brightness");
        // turning the side monitor off is the last change, and every monitor off leaves the main one on
        let c = ps.changes(&g, "game", &|_: &str| json!({ "edid:WSD-0001-2/brightness": "off", "edid:MSI-3DA4-1/brightness": "off" }));
        assert_eq!(c.iter().map(|x| x.1.clone()).collect::<Vec<_>>(), vec![json!({ "monitor": "edid:WSD-0001-2", "set": "enabled", "to": false })]);
    }

    /// A stand-in for OpenRGB's SDK server on its port (OpenRGB's network
    /// protocol, as its NetworkServer.cpp answers at protocols 5 and 6): Grant's
    /// devices and profiles, "Studio Low" in use; each profile loaded told back.
    /// None where the port is taken (OpenRGB's own server running).
    pub fn fake_openrgb() -> Option<std::sync::mpsc::Receiver<String>> {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:6742").ok()?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let mut c = conn;
                    let mut version = 0u32;
                    let devices: [(i32, &str); 8] = [(1, "Corsair Vengeance RGB DDR5"), (1, "Corsair Vengeance RGB DDR5"), (1, "Corsair Vengeance RGB DDR5"), (1, "Corsair Vengeance RGB DDR5"), (0, "MSI MEG X670E ACE (MS-7D69)"), (4, "Airgoo AG-DRGB16"), (6, "SteelSeries Sensei 310"), (2, "NVIDIA GeForce RTX 5090 FE")];
                    let profiles = ["ARC Raiders", "Studio High", "Studio Low", "Deepdriver II"];
                    let mut active = "Studio Low".to_string();
                    let send = |c: &mut std::net::TcpStream, dev: u32, id: u32, body: &[u8]| {
                        let mut p = b"ORGB".to_vec();
                        p.extend(dev.to_le_bytes());
                        p.extend(id.to_le_bytes());
                        p.extend((body.len() as u32).to_le_bytes());
                        p.extend(body);
                        let _ = c.write_all(&p);
                    };
                    let s = |t: &str| {
                        let mut b = ((t.len() + 1) as u16).to_le_bytes().to_vec();
                        b.extend(t.as_bytes());
                        b.push(0);
                        b
                    };
                    loop {
                        let mut h = [0u8; 16];
                        if c.read_exact(&mut h).is_err() || &h[..4] != b"ORGB" {
                            return;
                        }
                        let n = |i: usize| u32::from_le_bytes([h[i], h[i + 1], h[i + 2], h[i + 3]]);
                        let (dev, id, size) = (n(4), n(8), n(12) as usize);
                        let mut body = vec![0u8; size];
                        if c.read_exact(&mut body).is_err() {
                            return;
                        }
                        let ack = |c: &mut std::net::TcpStream, of: u32, version: u32| {
                            if version >= 6 {
                                let mut b = of.to_le_bytes().to_vec();
                                b.extend(0u32.to_le_bytes());
                                send(c, 0, 10, &b);
                            }
                        };
                        match id {
                            40 => {
                                let asked = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
                                version = asked.min(6);
                                send(&mut c, 0, 40, &6u32.to_le_bytes());
                                if version >= 6 {
                                    send(&mut c, 0, 51, &s("OpenRGB 1.0+ (git)")[2..]);
                                    ack(&mut c, 40, version);
                                }
                            }
                            50 | 52 => ack(&mut c, id, version),
                            0 => send(&mut c, 0, 0, &(devices.len() as u32).to_le_bytes()),
                            1 => {
                                let (kind, name) = devices[dev as usize];
                                let mut b = Vec::new();
                                b.extend(kind.to_le_bytes());
                                b.extend(s(name));
                                b.extend(s("Vendor"));
                                for t in ["A device", "1.0", "", "I2C"] {
                                    b.extend(s(t));
                                }
                                b.extend(0u16.to_le_bytes());
                                b.extend(0i32.to_le_bytes());
                                b.extend(0u16.to_le_bytes());
                                b.extend(0u16.to_le_bytes());
                                b.extend(0u16.to_le_bytes());
                                b.extend(0u16.to_le_bytes());
                                b.extend(0u32.to_le_bytes());
                                let mut whole = ((b.len() + 4) as u32).to_le_bytes().to_vec();
                                whole.extend(b);
                                send(&mut c, dev, 1, &whole);
                            }
                            150 => {
                                let mut b = (profiles.len() as u16).to_le_bytes().to_vec();
                                for p in profiles {
                                    b.extend(s(p));
                                }
                                let mut whole = ((b.len() + 4) as u32).to_le_bytes().to_vec();
                                whole.extend(b);
                                send(&mut c, 0, 150, &whole);
                            }
                            152 => {
                                let name = String::from_utf8_lossy(body.strip_suffix(&[0]).unwrap_or(&body)).to_string();
                                active = name.clone();
                                let _ = tx.send(name);
                                ack(&mut c, 152, version);
                            }
                            156 => {
                                let mut b = active.as_bytes().to_vec();
                                b.push(0);
                                send(&mut c, 0, 156, &b);
                                ack(&mut c, 156, version);
                            }
                            _ => {}
                        }
                    }
                });
            }
        });
        Some(rx)
    }

    /// The OpenRGB plugin brings everything OpenRGB lights in as one device,
    /// its profiles OpenRGB's; switching loads one, and switching back loads
    /// the one OpenRGB was on before. Run alone, with OpenRGB's own server
    /// off (its port free): `cargo test openrgb -- --ignored`.
    #[test]
    #[ignore]
    fn openrgb_switches_and_puts_back() {
        let Some(loaded) = fake_openrgb() else { panic!("OpenRGB’s port is taken: its own server is running") };
        let mut ps = plugin("openrgb");
        let mut device = None;
        for _ in 0..100 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            ps.poll();
            if let Some((_, d)) = ps.devices().into_iter().next() {
                device = Some(d);
                break;
            }
        }
        let d = device.expect("the plugin told its device");
        assert_eq!(d["name"], "OpenRGB");
        assert_eq!(d["sub"], "Motherboard, RAM, graphics card, LED strips and mouse");
        let names: Vec<&str> = d["capabilities"]["profiles"]["list"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["ARC Raiders", "Studio High", "Studio Low", "Deepdriver II"]);
        assert!(ps.plugins[0].warning.is_none(), "no warning once it reaches the server");
        let core = ps.plugins[0].core.clone();
        core.call("profiles/activate", &json!({ "device": "openrgb", "profile": "ARC Raiders" })).unwrap();
        assert_eq!(loaded.recv_timeout(std::time::Duration::from_secs(3)).unwrap(), "ARC Raiders");
        // a second switch keeps what it was before the first
        core.call("profiles/activate", &json!({ "device": "openrgb", "profile": "Deepdriver II" })).unwrap();
        assert_eq!(loaded.recv_timeout(std::time::Duration::from_secs(3)).unwrap(), "Deepdriver II");
        let back = core.call("profiles/restore", &json!({ "device": "openrgb" })).unwrap();
        assert_eq!(back["profile"], "Studio Low");
        assert_eq!(loaded.recv_timeout(std::time::Duration::from_secs(3)).unwrap(), "Studio Low");
    }

    /// The stand-in OpenRGB server for a minute, for a snapshot of a page with
    /// OpenRGB's device: `cargo test openrgb_stand_in -- --ignored`.
    #[test]
    #[ignore]
    fn openrgb_stand_in_for_a_minute() {
        let _loaded = fake_openrgb().expect("OpenRGB’s port is free");
        std::thread::sleep(std::time::Duration::from_secs(60));
    }

}
