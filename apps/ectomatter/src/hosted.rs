//! The packages Ectomatter runs (packages.rs), each started as a core is
//! (core_host.rs) while it's on, and kept in two lists, as Settings keeps
//! them: the cores and the plugins. A plugin
//! describes its section of a game's page in the shared settings language
//! (game/describe), and says what a game's choices change as the game starts
//! (game/changes); Ectomatter draws the one and makes the other (putback.rs).
//! A plugin may also bring in what another program drives, as OpenRGB's
//! does: it tells its devices as a core does, each with its profiles, for
//! the Library's Devices.
//!
//! Every core is started here, one that drives a device with a page of its
//! own (a keyboard, a mouse) too: the page is handed its core (drive), and
//! hears it from then on.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::core_host::{Computer, Core, NoDevices, Usb};
use crate::packages::{Kind, Package};

/// A package running: its package, its core, what it said last of how it is
/// (a warning to show beside it, such as a server it can't reach), and, for
/// a core, the devices it told, each as it told it, and what it describes
/// before any is met (`device/describe`).
pub struct Run {
    pub package: Package,
    pub core: Core,
    pub warning: Option<String>,
    pub devices: Vec<Value>,
    pub describes: Value,
    /// whether a device's page talks to it (drive), and hears what it tells
    driven: bool,
    /// its sections, as last described for each page, and when
    described: Vec<(String, Value, Instant)>,
    /// its own Settings section, as it last told it (settings/section)
    pub section: Option<Value>,
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

/// How long the window's own thread waits on a package (a page's sections,
/// a game's changes and its start, a package stopped): no plugin may hold
/// the window longer, whatever its own time.
const WINDOW_WAITS: Duration = Duration::from_secs(3);

/// How long a package being started may take to be made ready and meet
/// Ectomatter: the window waits on it, as a package is turned on in
/// Settings.
const START_WAITS: Duration = Duration::from_secs(10);

pub struct Hosted {
    pub cores: Vec<Run>,
    pub plugins: Vec<Run>,
    computer: Arc<dyn Computer>,
    /// what the plugins with readings told of them, for the page and the
    /// keyboard's screen alike
    readings: Readings,
    /// the devices told added since last asked, by plugin/device
    added: Vec<String>,
    /// the packages that wouldn't start, each with why, for Settings to say
    refused: std::collections::HashMap<String, String>,
    /// the packages started again since last asked, after they stopped
    restarted: Vec<String>,
    /// how long and how large each package may run (core_host::Limits)
    pub limits: crate::core_host::Limits,
    /// the USB devices a core is lent, through which it reaches those its
    /// manifest names and no others (core_host.rs): this computer's, but
    /// none offscreen or in a test
    usb: Box<dyn Fn() -> Box<dyn Usb>>,
}

/// The readings the plugins told, by plugin: what each lists it can read
/// (readings/list: each reading's id, label, unit and kind, and a catalog of
/// the computer's sensors where it reads them), and each reading's latest
/// value (readings/changed); and
/// which readings each one showing them wants read, which the plugin is told
/// (readings/watch). Shared with the keyboard screen's own thread, which
/// never waits on a plugin.
#[derive(Clone, Default)]
pub struct Readings(Arc<std::sync::Mutex<Kept>>);

#[derive(Default)]
struct Kept {
    told: std::collections::HashMap<String, Told>,
    /// by plugin, by who shows them: the readings wanted
    wants: std::collections::HashMap<String, std::collections::HashMap<String, Vec<String>>>,
    /// the plugins whose watch has changed since it was last told
    changed: std::collections::BTreeSet<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Told {
    /// its readings, each `{id, label, unit, kind}`
    pub list: Value,
    /// the computer's sensors, as ectomatter_sensors' Catalog lists them,
    /// where it reads them (Null where it doesn't)
    pub catalog: Value,
    pub values: serde_json::Map<String, Value>,
}

impl Readings {
    fn kept(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// What a plugin told, if it's running and has told any.
    pub fn of(&self, plugin: &str) -> Option<Told> {
        self.kept().told.get(plugin).cloned()
    }

    /// The first plugin (by its id) telling a reading of `kind`
    /// ("outside-temperature", "sky"), and the reading's id: what the
    /// frontend's own blocks of that kind show.
    pub fn of_kind(&self, kind: &str) -> Option<(String, String)> {
        let k = self.kept();
        let mut plugins: Vec<&String> = k.told.keys().collect();
        plugins.sort();
        plugins.into_iter().find_map(|p| {
            let r = k.told[p].list.as_array()?.iter().find(|r| r["kind"] == kind)?;
            Some((p.clone(), r["id"].as_str()?.to_string()))
        })
    }

    /// The first plugin (by its id) telling a catalog of the computer's
    /// sensors: the one Settings' sensors and the screen's stats read.
    pub fn with_catalog(&self) -> Option<String> {
        let k = self.kept();
        let mut plugins: Vec<&String> = k.told.iter().filter(|(_, t)| t.catalog.is_object()).map(|(p, _)| p).collect();
        plugins.sort();
        plugins.first().map(|p| p.to_string())
    }

    /// Every reading every plugin lists, each with its plugin's id, by plugin.
    pub fn all(&self) -> Vec<(String, Value)> {
        let k = self.kept();
        let mut plugins: Vec<&String> = k.told.keys().collect();
        plugins.sort();
        plugins.into_iter().flat_map(|p| k.told[p].list.as_array().into_iter().flatten().map(move |r| (p.clone(), r.clone()))).collect()
    }

    /// One reading's latest value.
    pub fn value(&self, plugin: &str, id: &str) -> Option<Value> {
        self.kept().told.get(plugin)?.values.get(id).cloned()
    }

    /// The readings `who` wants read from now on (none: it has stopped
    /// showing them); the plugin is told all that's wanted of it as it changes.
    pub fn want(&self, plugin: &str, who: &str, mut ids: Vec<String>) {
        ids.sort();
        ids.dedup();
        let mut k = self.kept();
        let had = k.wants.entry(plugin.to_string()).or_default().insert(who.to_string(), ids.clone());
        if had.unwrap_or_default() != ids {
            k.changed.insert(plugin.to_string());
        }
    }

    /// Nothing read for `who` any longer, from any plugin (nor for what it
    /// asks as "<who>/…"): it has stopped showing readings.
    pub fn rest(&self, who: &str) {
        let mut k = self.kept();
        let mine = |w: &String| w == who || w.strip_prefix(who).is_some_and(|r| r.starts_with('/'));
        let mut changed = Vec::new();
        for (plugin, wants) in k.wants.iter_mut() {
            let before = wants.len();
            wants.retain(|w, ids| !mine(w) || ids.is_empty());
            if wants.len() != before {
                changed.push(plugin.clone());
            }
        }
        k.changed.extend(changed);
    }

    /// Each plugin whose watch changed, with all that's wanted of it now.
    fn watches(&self) -> Vec<(String, Vec<String>)> {
        let mut k = self.kept();
        let changed: Vec<String> = std::mem::take(&mut k.changed).into_iter().collect();
        changed
            .into_iter()
            .map(|p| {
                let mut all: Vec<String> = k.wants.get(&p).map(|w| w.values().flatten().cloned().collect()).unwrap_or_default();
                all.sort();
                all.dedup();
                (p, all)
            })
            .collect()
    }

    /// A plugin started again: what's wanted of it is told it anew.
    fn told_again(&self, plugin: &str) {
        let mut k = self.kept();
        if k.wants.contains_key(plugin) {
            k.changed.insert(plugin.to_string());
        }
    }

    fn forget(&self, plugin: &str) {
        self.kept().told.remove(plugin);
    }

    fn with(&self, plugin: &str, f: impl FnOnce(&mut Told)) {
        f(self.kept().told.entry(plugin.to_string()).or_default());
    }
}

impl Hosted {
    pub fn new(computer: Arc<dyn Computer>) -> Hosted {
        Hosted::with_usb(computer, Box::new(crate::core_host::computers_usb))
    }

    /// Hosted with the USB devices a core is lent made by `usb`, a test's own.
    pub fn with_usb(computer: Arc<dyn Computer>, usb: Box<dyn Fn() -> Box<dyn Usb>>) -> Hosted {
        Hosted { cores: Vec::new(), plugins: Vec::new(), computer, readings: Readings::default(), added: Vec::new(), refused: std::collections::HashMap::new(), restarted: Vec::new(), limits: Default::default(), usb }
    }

    /// The plugins' readings as they tell them, for whoever shows them.
    pub fn readings(&self) -> Readings {
        self.readings.clone()
    }

    /// The plugins with Settings sections of their own, in Settings' order:
    /// each one's id and its section as it last told it.
    pub fn settings_sections(&self) -> Vec<(String, Value)> {
        self.plugins.iter().filter_map(|p| Some((p.package.id().to_string(), p.section.clone()?))).collect()
    }

    /// Hand a running package something without waiting on it (a plugin's
    /// Settings values, what readings to watch).
    pub fn tell(&self, id: &str, method: &str, params: &Value) {
        if let Some(r) = self.get(id) {
            r.core.tell(method, params);
        }
    }

    /// A running package's core, for a thread of its own to call (a place
    /// search, which waits on the network).
    pub fn core_of(&self, id: &str) -> Option<Core> {
        self.get(id).map(|r| r.core.clone())
    }

    fn all(&self) -> impl Iterator<Item = &Run> {
        self.cores.iter().chain(self.plugins.iter())
    }

    /// Each package that's on, started (offscreen too: a page's sections are
    /// drawn from what plugins describe, and a device's page from what its
    /// core describes; none of them changes anything there, nothing being
    /// made offscreen, and no core is lent a device).
    pub fn start_all(&mut self, settings: &crate::settings::Settings) {
        for p in crate::packages::all() {
            if settings.on(&p.switch()) {
                self.start(p);
            }
        }
    }

    /// The core running of the package `id`, for its device's page to talk
    /// to, as drive does.
    pub fn drive_id(&mut self, id: &str) -> Option<(Package, Core)> {
        let r = self.cores.iter_mut().find(|r| !r.driven && r.package.id() == id)?;
        r.driven = true;
        Some((r.package.clone(), r.core.clone()))
    }

    /// The first core running that describes a device of `kind` (`keyboard`,
    /// `mouse`) and no page talks to yet, for that kind's page to talk to:
    /// its package and its core. From then on the page hears what it tells.
    pub fn drive(&mut self, kind: &str) -> Option<(Package, Core)> {
        let r = self.cores.iter_mut().find(|r| !r.driven && r.describes["kind"] == kind)?;
        r.driven = true;
        Some((r.package.clone(), r.core.clone()))
    }

    pub fn start(&mut self, p: Package) {
        if self.get(p.id()).is_some() {
            return;
        }
        let kind = p.kind();
        // (offscreen or in a test, a folder of its own that's gone after, so a check keeps nothing)
        let scratch = crate::desk::keeps_nothing().then(|| std::env::temp_dir().join(format!("ectomatter-{}-{}-{}", kind.word(), p.id(), std::process::id())));
        let data = scratch.clone().unwrap_or_else(|| p.data());
        self.refused.remove(p.id());
        // (a manifest saying another version of the protocol is never started)
        let speaks = p.manifest.all["protocol"].as_u64();
        if speaks != Some(ectomatter_protocol::PROTOCOL as u64) {
            let e = format!("It speaks version {} of Ectomatter’s core protocol, and this Ectomatter speaks {}.", speaks.map_or("(none said)".to_string(), |n| n.to_string()), ectomatter_protocol::PROTOCOL);
            eprintln!("ectomatter: the {} {} wouldn’t start: {e}", p.id(), kind.word());
            self.refused.insert(p.id().to_string(), e);
            return;
        }
        // (a core reaches the devices its manifest names; a plugin, none)
        let usb: Box<dyn Usb> = if kind == Kind::Core { (self.usb)() } else { Box::new(NoDevices) };
        match Core::start_within(&p.dir, data, usb, Some(self.computer.clone()), self.limits) {
            Ok(core) => {
                // (one that speaks another protocol, or won't start, is stopped, and Settings says why)
                if let Err(e) = crate::core_host::initialize_within(&core, Value::Null, START_WAITS) {
                    eprintln!("ectomatter: the {} {} wouldn’t start: {e}", p.id(), kind.word());
                    core.tell("shutdown", &Value::Null);
                    self.refused.insert(p.id().to_string(), e);
                    if let Some(d) = scratch {
                        let _ = std::fs::remove_dir_all(d);
                    }
                    return;
                }
                eprintln!("ectomatter: the {} {} started", p.id(), kind.word());
                // (what a core's devices are, before any is met: a core that can't say, nothing)
                let describes = if kind == Kind::Core { core.call_within("device/describe", &Value::Null, WINDOW_WAITS).unwrap_or(Value::Null) } else { Value::Null };
                self.readings.told_again(p.id());
                let run = Run { package: p, core, warning: None, devices: Vec::new(), describes, driven: false, described: Vec::new(), section: None, scratch };
                match kind {
                    Kind::Core => self.cores.push(run),
                    Kind::Plugin => self.plugins.push(run),
                }
            }
            Err(e) => {
                eprintln!("ectomatter: the {} {} wouldn’t start: {e}", p.id(), kind.word());
                self.refused.insert(p.id().to_string(), e);
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
                self.readings.forget(id);
                let _ = p.core.call_within("shutdown", &Value::Null, WINDOW_WAITS);
            }
        }
    }

    /// Why a package wouldn't start, if it wouldn't.
    pub fn refused(&self, id: &str) -> Option<&str> {
        self.refused.get(id).map(String::as_str)
    }

    pub fn get(&self, id: &str) -> Option<&Run> {
        self.all().find(|p| p.package.id() == id)
    }

    /// What the packages running told since last asked: their warnings, and
    /// the devices they brought in. True if anything changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        // (a core a device's page talks to tells the page)
        for p in self.cores.iter_mut().filter(|r| !r.driven).chain(self.plugins.iter_mut()) {
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
                        self.added.push(format!("{}/{}", p.package.id(), id.as_str().unwrap_or("")));
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
                    // (a plugin's own Settings section, and its readings, as they change)
                    "settings/section" => {
                        p.section = params.get("section").cloned().filter(Value::is_object);
                        changed = true;
                    }
                    "readings/list" => {
                        self.readings.with(p.package.id(), |t| (t.list, t.catalog) = (params["readings"].clone(), params["catalog"].clone()));
                        changed = true;
                    }
                    "readings/changed" => {
                        if let Some(v) = params["values"].as_object() {
                            self.readings.with(p.package.id(), |t| t.values.extend(v.clone()));
                        }
                    }
                    // (it stopped: Settings says so, its devices are gone until it's back;
                    // started again, it's told again what it was told before)
                    crate::core_host::STOPPED => {
                        let why = params["why"].as_str().unwrap_or("it stopped");
                        p.warning = Some(format!("It stopped: {why}. Ectomatter starts it again {}.", crate::core_host::again_in(params["again"].as_u64().unwrap_or(1))));
                        p.devices.clear();
                        p.described.clear();
                        changed = true;
                    }
                    crate::core_host::RESTARTED => {
                        p.warning = None;
                        self.readings.told_again(p.package.id());
                        self.restarted.push(p.package.id().to_string());
                        changed = true;
                    }
                    _ => {}
                }
            }
        }
        // (each plugin told what's wanted read of it, as that changes)
        for (plugin, ids) in self.readings.watches() {
            self.tell(&plugin, "readings/watch", &json!({ "ids": ids }));
        }
        changed
    }

    /// The plugins whose sections a page gets, in Settings' order: each one's
    /// id and its section, as it describes it for this page (`page` is
    /// "game", "app" or "every"), given what `more` adds for each plugin (the
    /// page's values; for the plugin that says how games start, what
    /// Ectomatter knows). A description a few seconds old, or asked with
    /// anything changed, is asked again.
    pub fn sections(&mut self, page: &str, name: &str, more: &dyn Fn(&Package) -> Value) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        for p in &mut self.plugins {
            if !p.package.adds_to_games() {
                continue;
            }
            let mut asked = json!({ "for": page, "name": name });
            if let (Some(a), Value::Object(m)) = (asked.as_object_mut(), more(&p.package)) {
                a.extend(m);
            }
            let key = asked.to_string();
            let fresh = p.described.iter().find(|(k, _, t)| *k == key && t.elapsed() < Duration::from_secs(3)).map(|(_, v, _)| v.clone());
            let section = match fresh {
                Some(s) => s,
                None => {
                    let s = p.core.call_within("game/describe", &asked, WINDOW_WAITS).map(|v| v["section"].clone()).unwrap_or_else(|e| json!({ "label": p.package.name(), "layout": [{ "note": e }] }));
                    p.described.retain(|(k, _, t)| *k != key && t.elapsed() < Duration::from_secs(3));
                    p.described.push((key, s.clone(), Instant::now()));
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
            let answer = match p.core.call_within("game/changes", &asked, WINDOW_WAITS) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("ectomatter: the {} plugin’s changes for {}: {e}", p.package.id(), game.name);
                    continue;
                }
            };
            for c in answer["changes"].as_array().into_iter().flatten() {
                let what = c["change"].clone();
                // (a change of a kind its manifest doesn't name is left unmade)
                let allowed = what.get("monitor").is_some() && uses.monitors;
                if !allowed {
                    eprintln!("ectomatter: the {} plugin asked for a change it may not make: {what}", p.package.id());
                    continue;
                }
                let label: String = c["label"].as_str().unwrap_or("").chars().take(20).collect();
                out.push((p.package.id().to_string(), what, label, c["words"].as_str().unwrap_or("").to_string()));
            }
        }
        out
    }

    /// The plugin that says how games start (the first on with `launch`
    /// among its uses: the Game Configurator), by its id.
    pub fn launcher(&self) -> Option<&str> {
        self.plugins.iter().find(|p| p.package.adds_to_games() && p.package.manifest.uses.launch).map(|p| p.package.id())
    }

    /// A game's start as that plugin says it (game/launch): the recipe the
    /// launcher starts it from, for the game (`game`: its id, name and Steam
    /// number), its values (its own over Every game's) and what Ectomatter
    /// knows. None without such a plugin.
    pub fn launch(&self, game: &Value, values: &dyn Fn(&str) -> Value, every: &dyn Fn(&str) -> Value, facts: &Value) -> Option<Result<Value, String>> {
        let p = self.plugins.iter().find(|p| p.package.adds_to_games() && p.package.manifest.uses.launch)?;
        Some(p.core.call_within("game/launch", &json!({ "game": game, "values": values(p.package.id()), "every": every(p.package.id()), "facts": facts }), WINDOW_WAITS).map(|v| v["recipe"].clone()))
    }

    /// The devices the packages running here told (a plugin's, from the
    /// program it reaches, as OpenRGB's), each by its package and as it told it.
    /// The devices told added since this was last asked, by plugin/device.
    pub fn take_added(&mut self) -> Vec<String> {
        std::mem::take(&mut self.added)
    }

    /// The packages started again since last asked, after they stopped: each
    /// to be handed again what it was handed (its Settings values).
    pub fn take_restarted(&mut self) -> Vec<String> {
        std::mem::take(&mut self.restarted)
    }

    pub fn devices(&self) -> Vec<(String, Value)> {
        self.all().flat_map(|p| p.devices.iter().map(|d| (p.package.id().to_string(), d.clone()))).collect()
    }
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
        fn read(&self, path: &std::path::Path) -> Result<String, String> {
            Err(format!("{} isn’t on this test’s computer", path.display()))
        }
        fn entries(&self, path: &std::path::Path) -> Result<Vec<String>, String> {
            Err(format!("{} isn’t on this test’s computer", path.display()))
        }
        fn resolve(&self, path: &std::path::Path) -> Result<std::path::PathBuf, String> {
            Err(format!("{} isn’t on this test’s computer", path.display()))
        }
        fn nvidia(&self, _: &str, _: &str) -> Result<Value, String> {
            Err("this test’s computer has no NVIDIA card".into())
        }
        fn fetch(&self, url: &str, _: std::time::Duration) -> Result<String, String> {
            Err(format!("this test’s computer is offline ({url})"))
        }
    }

    /// One package that comes with Ectomatter, started alone.
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

    /// The keyboard's screen asks a plugin to read only the readings its
    /// pages show (apart from what it asks of the sensors' catalog), and
    /// once it rests, asks nothing of any plugin.
    #[test]
    fn the_screen_asks_only_for_what_it_shows_and_rests_from_every_plugin() {
        let r = Readings::default();
        r.with("made-up-sensors", |t| t.catalog = json!({ "devices": [] }));
        r.with("made-up-stocks", |t| t.list = json!([{ "id": "acme", "label": "ACME shares", "unit": "$" }, { "id": "room", "label": "Room", "unit": "°C" }]));
        let mut s = crate::lcd_stats::Stats::of(r.clone(), "screen");
        let acme = crate::lcd_page::reading_number("made-up-stocks", "acme");
        s.showing([acme].into());
        let _ = s.read();
        let w: std::collections::HashMap<String, Vec<String>> = r.watches().into_iter().collect();
        assert_eq!(w.get("made-up-stocks").map(Vec::as_slice), Some(&["acme".to_string()][..]), "{w:?}");
        s.rest();
        let w = r.watches();
        assert!(w.iter().any(|(p, _)| p == "made-up-stocks"), "{w:?}");
        assert!(w.iter().all(|(_, ids)| ids.is_empty()), "{w:?}");
    }

    /// docs/core-protocol.md, section 11, Readings: readings are found by what
    /// they are, from any plugin: the outside temperature and the sky by
    /// their kinds, and the computer's sensors from whichever plugin tells
    /// a catalog of them, not from the Weather Feed and the Sensor Feed by name.
    #[test]
    fn readings_are_found_by_what_they_are_from_any_plugin() {
        let r = Readings::default();
        r.with("made-up-weather", |t| {
            t.list = json!([{ "id": "t", "label": "Outdoors", "unit": "°C", "kind": "outside-temperature" }, { "id": "s", "label": "Sky", "kind": "sky" }, { "id": "uv", "label": "UV index" }]);
            t.values.insert("t".into(), json!(12.5));
        });
        r.with("made-up-sensors", |t| t.catalog = json!({ "devices": [], "missing": [] }));
        assert_eq!(r.of_kind("outside-temperature"), Some(("made-up-weather".to_string(), "t".to_string())));
        assert_eq!(r.of_kind("sky"), Some(("made-up-weather".to_string(), "s".to_string())));
        assert_eq!(r.of_kind("rain"), None);
        assert_eq!(r.with_catalog().as_deref(), Some("made-up-sensors"));
        assert_eq!(r.all().len(), 3, "every reading every plugin lists");
        assert_eq!(r.value("made-up-weather", "t"), Some(json!(12.5)));
    }

    /// A copy of the Sensei 310's core under `id`, its manifest changed by `change`.
    fn made_up_core(id: &str, change: impl Fn(String) -> String) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ectomatter-{id}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let from = crate::packages::builtin_dir(Kind::Core).join("steelseries-sensei-310");
        assert!(from.join("core.wasm").exists(), "the SteelSeries module isn't built: run tools/build-cores.sh");
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["core.wasm", "glyph.svg", "light.json"] {
            std::fs::copy(from.join(f), dir.join(f)).unwrap();
        }
        let manifest = std::fs::read_to_string(from.join("core.json")).unwrap().replace("\"steelseries-sensei-310\"", &format!("\"{id}\""));
        std::fs::write(dir.join("core.json"), change(manifest)).unwrap();
        dir
    }

    /// Whether a core tells it met a device in two seconds.
    fn meets(core: &Core) -> bool {
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            if core.told().iter().any(|(m, _)| m == "device/added") {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// docs/core-protocol.md, principle 1: a core Ectomatter has never seen,
    /// started as every core is, is lent the devices its manifest names and
    /// meets its own, and the page for the kind of device it describes talks
    /// to it; the same core naming another device is lent none.
    #[test]
    fn a_core_of_its_own_reaches_its_device_and_its_page() {
        let fake = || -> Box<dyn crate::core_host::Usb> { Box::new(crate::core_host::tests::One(Some(Box::new(crate::core_host::tests::FakeMouse::new())))) };
        let dir = made_up_core("made-up-mouse", |m| m);
        let mut ps = Hosted::with_usb(Arc::new(Desk), Box::new(fake));
        ps.start(crate::packages::Package::read(&dir, true).unwrap());
        let (p, core) = ps.drive("mouse").expect("the mouse's page talks to it");
        assert_eq!(p.id(), "made-up-mouse");
        assert!(meets(&core), "it met the mouse");
        assert!(ps.drive("mouse").is_none(), "one page to a core");
        ps.stop("made-up-mouse");

        let other = made_up_core("made-up-other", |m| m.replace("\"vendor\": \"1038\"", "\"vendor\": \"1234\""));
        let mut ps = Hosted::with_usb(Arc::new(Desk), Box::new(fake));
        ps.start(crate::packages::Package::read(&other, true).unwrap());
        let (_, core) = ps.drive("mouse").expect("it describes a mouse, met or not");
        assert!(!meets(&core), "it reached a device its manifest doesn't name");
        ps.stop("made-up-other");
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(other);
    }

    /// docs/core-protocol.md, section 3: a plugin that stops is said to have
    /// stopped, in Settings, until it's started again, and is then handed
    /// again what it was handed before.
    #[test]
    fn a_plugin_that_stops_says_so_until_it_is_back() {
        let mut ps = Hosted::new(Arc::new(Desk));
        ps.limits = crate::core_host::Limits { call: Duration::from_millis(300), timer: Duration::from_millis(300), memory: 64 << 20, wait: Duration::from_secs(10) };
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/packages/misbehaving");
        assert!(dir.join("core.wasm").exists(), "the misbehaving plugin isn't built: run tools/build-test-packages.sh");
        ps.start(crate::packages::Package::read(&dir, true).unwrap());
        let core = ps.core_of("misbehaving").expect("started");
        assert!(core.call("crash", &Value::Null).is_err());
        let until = Instant::now() + Duration::from_secs(3);
        while ps.get("misbehaving").unwrap().warning.is_none() && Instant::now() < until {
            ps.poll();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(ps.get("misbehaving").unwrap().warning.as_deref(), Some("It stopped: it crashed. Ectomatter starts it again in a second."));
        let until = Instant::now() + Duration::from_secs(5);
        while ps.get("misbehaving").unwrap().warning.is_some() && Instant::now() < until {
            ps.poll();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(ps.get("misbehaving").unwrap().warning.is_none(), "back");
        assert_eq!(ps.take_restarted(), ["misbehaving"], "to be handed its values again");
        ps.stop("misbehaving");
    }

    /// docs/core-protocol.md, section 9: a package whose manifest says another
    /// version of the protocol isn't started, and Settings is told why; the
    /// same package saying this one starts.
    #[test]
    fn a_package_speaking_another_protocol_isnt_started() {
        let dir = std::env::temp_dir().join(format!("ectomatter-old-protocol-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let from = crate::packages::builtin_dir(Kind::Plugin).join("monitor-switcher");
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["core.json", "core.wasm", "glyph.svg"] {
            std::fs::copy(from.join(f), dir.join(f)).unwrap();
        }
        let manifest = std::fs::read_to_string(dir.join("core.json")).unwrap();
        std::fs::write(dir.join("core.json"), manifest.replace("\"protocol\": 4", "\"protocol\": 3")).unwrap();
        let mut ps = Hosted::new(Arc::new(Desk));
        ps.start(crate::packages::Package::read(&dir, true).unwrap());
        assert!(ps.get("monitor-switcher").is_none(), "not started");
        assert_eq!(ps.refused("monitor-switcher"), Some("It speaks version 3 of Ectomatter’s core protocol, and this Ectomatter speaks 4."));
        std::fs::write(dir.join("core.json"), manifest).unwrap();
        ps.start(crate::packages::Package::read(&dir, true).unwrap());
        assert!(ps.get("monitor-switcher").is_some() && ps.refused("monitor-switcher").is_none(), "started, saying this one");
        ps.stop("monitor-switcher");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A computer whose web answers as Open-Meteo, the National Weather
    /// Service and the place search would (a clear 15 °C morning in Springfield
    /// Lakes, with a tornado warning in force), and nothing else; and the
    /// addresses asked, in order.
    struct Web(std::sync::Mutex<Vec<String>>);

    impl Computer for Web {
        fn monitors(&self) -> Result<Value, String> {
            Err("no monitors here".into())
        }
        fn read(&self, path: &std::path::Path) -> Result<String, String> {
            Err(format!("{} isn’t on this test’s computer", path.display()))
        }
        fn entries(&self, path: &std::path::Path) -> Result<Vec<String>, String> {
            Err(format!("{} isn’t on this test’s computer", path.display()))
        }
        fn resolve(&self, path: &std::path::Path) -> Result<std::path::PathBuf, String> {
            Err(format!("{} isn’t on this test’s computer", path.display()))
        }
        fn nvidia(&self, _: &str, _: &str) -> Result<Value, String> {
            Err("no NVIDIA card here".into())
        }
        fn fetch(&self, url: &str, _: std::time::Duration) -> Result<String, String> {
            self.0.lock().unwrap().push(url.to_string());
            let reply = if url.starts_with("https://api.open-meteo.com/v1/forecast") {
                json!({ "utc_offset_seconds": -14400,
                        "current": { "weather_code": 0, "cloud_cover": 0, "rain": 0, "showers": 0, "snowfall": 0, "wind_speed_10m": 6.4, "wind_direction_10m": 90, "wind_gusts_10m": 9.0, "visibility": 24000, "temperature_2m": 15.0 },
                        "daily": { "sunrise": ["2026-10-22T07:28"], "sunset": ["2026-10-22T18:54"] } })
            } else if url.starts_with("https://air-quality-api.open-meteo.com/") {
                json!({ "current": { "pm2_5": 4.0, "dust": 0.0, "aerosol_optical_depth": 0.1 } })
            } else if url.starts_with("https://api.weather.gov/alerts/active") {
                json!({ "features": [{ "properties": { "event": "Tornado Warning" } }] })
            } else if url.starts_with("https://geocoding-api.open-meteo.com/v1/search") {
                json!({ "results": [{ "id": 4172086, "name": "Springfield", "admin1": "Florida", "country": "United States", "country_code": "US", "latitude": 30.15326, "longitude": -85.61132 }] })
            } else {
                return Err(format!("offline: {url}"));
            };
            Ok(reply.to_string())
        }
    }

    /// A made-up computer's files (ectomatter_sensors::fake's, Grant's like),
    /// kept under a folder of its own but seen from / as a computer's are;
    /// and its NVIDIA card as the made-up driver reads it.
    struct Files(std::path::PathBuf, std::sync::Mutex<ectomatter_sensors::fake::FakeNvidia>);

    impl Files {
        fn here(&self, path: &std::path::Path) -> std::path::PathBuf {
            self.0.join(path.strip_prefix("/").unwrap_or(path))
        }
    }

    impl Computer for Files {
        fn monitors(&self) -> Result<Value, String> {
            Err("no monitors here".into())
        }
        fn read(&self, path: &std::path::Path) -> Result<String, String> {
            std::fs::read_to_string(self.here(path)).map_err(|e| e.to_string())
        }
        fn entries(&self, path: &std::path::Path) -> Result<Vec<String>, String> {
            let mut names: Vec<String> = std::fs::read_dir(self.here(path)).map_err(|e| e.to_string())?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            Ok(names)
        }
        fn resolve(&self, path: &std::path::Path) -> Result<std::path::PathBuf, String> {
            let real = std::fs::canonicalize(self.here(path)).map_err(|e| e.to_string())?;
            Ok(std::path::Path::new("/").join(real.strip_prefix(&self.0).map_err(|_| "it leads off the made-up computer".to_string())?))
        }
        fn nvidia(&self, pci: &str, what: &str) -> Result<Value, String> {
            use ectomatter_sensors::list::{Nv, Nvidia, Val};
            let mut nv = self.1.lock().unwrap();
            if what == "probe" {
                let p = nv.probe(pci).ok_or("no such card")?;
                return Ok(json!({ "has": p.has.iter().map(|n| crate::computer::nv_word(*n)).collect::<Vec<_>>(), "vram": p.vram }));
            }
            let n = [Nv::Temp, Nv::Load, Nv::Clock, Nv::MemClock, Nv::Vram, Nv::Power, Nv::Fan].into_iter().find(|n| crate::computer::nv_word(*n) == what).ok_or("no such reading")?;
            match nv.read(pci, n).ok_or("no reading")? {
                Val::One(v) => Ok(json!(v)),
                Val::Of(u, t) => Ok(json!([u, t])),
            }
        }
        fn fetch(&self, url: &str, _: std::time::Duration) -> Result<String, String> {
            Err(format!("offline: {url}"))
        }
    }

    /// The Sensor Feed lists a computer's sensors, each named and in its
    /// part, as the Sensor Feed box shows them, and reads those it's asked to
    /// watch, reaching the files and NVIDIA's driver only through Ectomatter.
    #[test]
    fn the_sensors_plugin_lists_and_reads_what_it_is_asked() {
        let dir = ectomatter_sensors::fake::temp_dir("sensor-feed-plugin");
        ectomatter_sensors::fake::computer(&dir);
        let mut ps = Hosted::new(Arc::new(Files(dir.clone(), std::sync::Mutex::new(ectomatter_sensors::fake::nvidia()))));
        ps.start(crate::packages::Package::read(&crate::packages::builtin_dir(Kind::Plugin).join("sensor-feed"), false).unwrap());
        let r = ps.readings();
        let mut cat = ectomatter_sensors::list::Catalog::default();
        for _ in 0..200 {
            ps.poll();
            if let Some(c) = r.of("sensor-feed").and_then(|t| serde_json::from_value::<ectomatter_sensors::list::Catalog>(t.catalog).ok()) {
                cat = c;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let keys: Vec<&str> = cat.devices.iter().map(|d| d.key.as_str()).collect();
        assert!(keys.contains(&"cpu") && keys.contains(&"card@0000:01:00.0") && keys.contains(&"board"), "{keys:?}");
        // what's wanted, read and told
        r.want("sensor-feed", "test", vec!["card@0000:01:00.0/temperature".into(), "memory/used".into()]);
        let mut got = (None, None);
        for _ in 0..200 {
            ps.poll();
            got = (r.value("sensor-feed", "card@0000:01:00.0/temperature"), r.value("sensor-feed", "memory/used"));
            if got.0.is_some() && got.1.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(got.0, Some(json!(43.0)), "the card's temperature, through NVIDIA's driver as Ectomatter reaches it");
        assert!(got.1.as_ref().is_some_and(|v| v.as_array().is_some_and(|a| a.len() == 2)), "memory in use, of all of it: {got:?}");
        assert!(r.value("sensor-feed", "cpu/load").is_none(), "what isn't watched isn't read");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Weather Feed tells its Settings section and its readings as they
    /// change: without a place, it asks for one; with one, the weather there
    /// in the units chosen, the warning in force and whose it is; and a place
    /// typed in is looked up. It asks only the web its manifest names, through
    /// Ectomatter.
    #[test]
    fn the_weather_plugin_tells_its_section_and_readings() {
        let web = Arc::new(Web(Default::default()));
        let mut ps = Hosted::new(web.clone());
        ps.start(crate::packages::Package::read(&crate::packages::builtin_dir(Kind::Plugin).join("weather-feed"), false).unwrap());
        let section = |ps: &mut Hosted| {
            ps.poll();
            ps.settings_sections().into_iter().find(|(id, _)| id == "weather-feed").map(|(_, s)| s)
        };
        let first = section(&mut ps).expect("it told its section as it started");
        assert_eq!(first["layout"][0]["note"], "Every device that shows weather uses this place.");
        assert_eq!(first["layout"][1]["status"]["words"], "Chosen with the place");
        assert!(ps.readings().of("weather-feed").is_some_and(|t| t.list.as_array().is_some_and(|l| l.len() == 2)), "it lists what it reads");
        // a place, Fahrenheit and miles an hour
        let place = json!({ "id": 4172086, "name": "Springfield", "latin": "Springfield", "region": "Florida", "country": "United States", "country_code": "US", "lat": 30.15326, "lon": -85.61132 });
        ps.tell("weather-feed", "settings/set", &json!({ "values": { "place": place, "temperature": "f", "wind": "mph" } }));
        let mut now = Value::Null;
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            if let Some(s) = section(&mut ps).filter(|s| s["layout"][1]["card"]["big"].is_string()) {
                now = s;
                break;
            }
        }
        let card = &now["layout"][1]["card"];
        assert_eq!((card["big"].as_str(), card["beside"].as_str()), (Some("59°F"), Some("Clear")));
        assert_eq!(card["facts"], json!([["Cloud", "0%"], ["Wind", "4 mph"], ["Sunrise", "7:28 AM"], ["Sunset", "6:54 PM"]]));
        assert_eq!(now["layout"][3]["note"], "", "both units chosen: no word of the region's");
        assert_eq!(now["layout"][0]["note"], "Springfield, Florida, United States");
        let warn = &now["layout"][2]["status"];
        assert_eq!((warn["words"].as_str(), warn["tone"].as_str()), (Some("National Weather Service"), Some("good")));
        assert_eq!(warn["note"], "Tornado warning in force. Official warnings for this place, checked every 3 minutes.");
        assert_eq!(now["settings"][0]["text"], "Springfield", "the box shows the place's name");
        let r = ps.readings();
        assert_eq!((r.value("weather-feed", "temperature"), r.value("weather-feed", "sky")), (Some(json!(15.0)), Some(json!("Clear"))), "the readings in °C, and the sky in words");
        // a place typed in, looked up
        let found = ps.core_of("weather-feed").unwrap().call("settings/search", &json!({ "id": "place", "text": "Land" })).unwrap();
        assert_eq!((found["found"][0]["label"].as_str(), found["found"][0]["sub"].as_str()), (Some("Springfield"), Some("Florida, United States")));
        assert_eq!(found["found"][0]["value"]["country_code"], "US", "the place itself, kept as chosen");
        // only what its manifest names, all over HTTPS
        let asked = web.0.lock().unwrap().clone();
        assert!(!asked.is_empty() && asked.iter().all(|u| u.starts_with("https://")), "{asked:?}");
    }

    /// The Monitor Switcher's section: a column for each monitor, a cell
    /// saying why where a monitor can't do something, and the main monitor.
    #[test]
    fn the_monitors_section_has_a_column_for_each() {
        let mut ps = plugin("monitor-switcher");
        let s = &ps.sections("game", "ARC Raiders", &|_| json!({}))[0].1;
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
        assert!(s["layout"][1]["note"].as_str().unwrap().starts_with("Ectomatter sets these when ARC Raiders starts"));
    }

    /// The Game Configurator, run as Ectomatter runs it: its section drawn
    /// from the page's values and what Ectomatter knows, asked again once a
    /// value changes; and a game's recipe for the launcher.
    #[test]
    fn the_game_configurator_says_how_a_game_starts() {
        let mut ps = plugin("game-configurator");
        assert_eq!(ps.launcher(), Some("game-configurator"));
        let facts = json!({
            "default": "Proton-GE Latest", "home": "/home/someone", "cores": "0-7,16-23",
            "protons": [{ "id": "Proton-GE Latest", "name": "Proton-GE Latest", "path": "/s/compatibilitytools.d/Proton-GE Latest", "runtime": "/s/common/SteamLinuxRuntime_4", "family": "ge" }],
            "main": { "width": 3440, "height": 1440, "refresh": 240, "hdr": true },
            "game": { "id": "steam_app_1808500", "name": "ARC Raiders", "app": 1808500, "native": false, "through": true, "prefix": { "path": "/home/someone/.local/share/Steam/steamapps/compatdata/1808500", "bytes": 662000000u64, "made": true }, "backups": 1 },
        });
        let more = |values: Value| { let facts = facts.clone(); move |p: &Package| if p.id() == "game-configurator" { json!({ "values": values, "facts": facts }) } else { json!({}) } };
        let s = ps.sections("game", "ARC Raiders", &more(json!({})))[0].1.clone();
        assert_eq!(s["label"], "Game Configurator");
        assert_eq!(s["layout"][0]["row"]["status"]["words"], "Ectomatter");
        assert!(s["layout"].to_string().contains("One backup so far."));
        assert!(s["layout"][7]["row"].get("nest").is_none(), "gamescope's own settings only while it runs");
        let s = ps.sections("game", "ARC Raiders", &more(json!({ "gs": true })))[0].1.clone();
        assert_eq!(s["layout"][7]["row"]["nest"][0]["label"], "Size", "asked again for the new value");
        let r = ps.launch(&json!({ "id": "steam_app_1808500", "name": "ARC Raiders", "app": 1808500 }), &|_| json!({ "hdr": "proton" }), &|_| json!({}), &facts).unwrap().unwrap();
        assert_eq!(r["proton"], "/s/compatibilitytools.d/Proton-GE Latest");
        assert_eq!(r["env"]["PROTON_ENABLE_HDR"], "1");
        assert_eq!(r["wrap"], json!([["gamemoderun"]]));
        // (what it makes, gamescope and MangoHud and the person's own variables
        // too, is all Ectomatter knows safe: written whole)
        for values in [json!({ "hdr": "proton" }), json!({ "hdr": "gamescope", "gs": true, "mangohud": true, "cap": "120", "env": { "MY_MOD": "1" }, "dll": "dwrite=n,b" })] {
            let r = ps.launch(&json!({ "id": "steam_app_1808500", "name": "ARC Raiders", "app": 1808500 }), &|_| values.clone(), &|_| json!({}), &facts).unwrap().unwrap();
            let (checked, left_out) = crate::launcher::checked_recipe(&r, &facts, &["MY_MOD".to_string()]);
            assert_eq!((&checked, left_out), (&r, Vec::<String>::new()), "{r}");
        }
    }

    /// A game's choices become changes, in order, each a change of a monitor;
    /// one the monitor is already at changes nothing.
    #[test]
    fn choices_become_changes() {
        let ps = plugin("monitor-switcher");
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

    /// OpenRGB Link brings everything OpenRGB lights in as one device,
    /// its profiles OpenRGB's; switching loads one, and switching back loads
    /// the one OpenRGB was on before. Run alone, with OpenRGB's own server
    /// off (its port free): `cargo test openrgb -- --ignored`.
    #[test]
    #[ignore]
    fn openrgb_switches_and_puts_back() {
        let Some(loaded) = fake_openrgb() else { panic!("OpenRGB’s port is taken: its own server is running") };
        let mut ps = plugin("openrgb-link");
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
