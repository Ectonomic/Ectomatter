//! Cores, run inside Ectodeck as WebAssembly components (the interface is
//! wit/core.wit). Each core runs on a thread of its own, where its calls and
//! its timers take turns. A core reaches the USB devices its manifest
//! (core.json) matches, and no others, through the connection Ectodeck lends
//! it; and the clocks and its own folder, seen as /data. Nothing else.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;
use wasmtime::component::{Component, HasSelf, Linker, Resource, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

/// The interface's bindings: the world's own type (`World`), and Ectodeck's
/// side of its imports.
mod world {
    wasmtime::component::bindgen!({
        path: "../../wit",
        world: "core",
        with: { "ectodeck:core/usb.hid-device": super::Lent },
    });
    pub use Core as World;
}

use world::World;
use world::ectodeck;
use world::ectodeck::core::usb::DeviceInfo;

/// The USB devices on this computer, as a core's connection reaches them:
/// hidapi's, or a test's.
pub trait Usb: Send {
    /// every HID collection plugged in now
    fn devices(&mut self) -> Vec<DeviceInfo>;
    fn open(&mut self, device: &DeviceInfo) -> Result<Box<dyn Hid>, String>;
}

/// One HID collection, opened.
pub trait Hid: Send {
    fn send_feature(&mut self, report: &[u8]) -> Result<(), String>;
    fn get_feature(&mut self, report_id: u8, len: usize) -> Result<Vec<u8>, String>;
    fn write(&mut self, report: &[u8]) -> Result<usize, String>;
    fn read(&mut self, timeout_ms: u32) -> Result<Vec<u8>, String>;
}

/// A device a core has open: its collection, behind the core's handle.
pub struct Lent(Box<dyn Hid>);

/// The devices on this computer, over hidapi.
pub struct Hidapi(Option<hidapi::HidApi>);

impl Hidapi {
    pub fn new() -> Hidapi {
        Hidapi(None)
    }
}

impl Usb for Hidapi {
    fn devices(&mut self) -> Vec<DeviceInfo> {
        let api = match &mut self.0 {
            Some(api) => {
                let _ = api.refresh_devices();
                api
            }
            none => match hidapi::HidApi::new() {
                Ok(api) => none.insert(api),
                Err(_) => return Vec::new(),
            },
        };
        api.device_list()
            .map(|d| DeviceInfo {
                vendor: d.vendor_id(),
                product: d.product_id(),
                interface: d.interface_number(),
                usage_page: d.usage_page(),
                usage: d.usage(),
                path: d.path().to_string_lossy().into_owned(),
            })
            .collect()
    }

    fn open(&mut self, device: &DeviceInfo) -> Result<Box<dyn Hid>, String> {
        let api = self.0.as_ref().ok_or("no USB devices have been listed")?;
        let path = std::ffi::CString::new(device.path.clone()).map_err(|e| e.to_string())?;
        let dev = api.open_path(&path).map_err(|e| e.to_string())?;
        Ok(Box::new(HidapiDevice(dev)))
    }
}

struct HidapiDevice(hidapi::HidDevice);

impl Hid for HidapiDevice {
    fn send_feature(&mut self, report: &[u8]) -> Result<(), String> {
        self.0.send_feature_report(report).map_err(|e| e.to_string())
    }
    /// as ectotype-core's own HidTransport reads one: the whole buffer, whatever
    /// length the system says it read
    fn get_feature(&mut self, report_id: u8, len: usize) -> Result<Vec<u8>, String> {
        let mut buf = vec![0u8; len];
        buf[0] = report_id;
        self.0.get_feature_report(&mut buf).map_err(|e| e.to_string())?;
        Ok(buf)
    }
    fn write(&mut self, report: &[u8]) -> Result<usize, String> {
        self.0.write(report).map_err(|e| e.to_string())
    }
    fn read(&mut self, timeout_ms: u32) -> Result<Vec<u8>, String> {
        let mut buf = vec![0u8; 1024];
        let n = self.0.read_timeout(&mut buf, timeout_ms as i32).map_err(|e| e.to_string())?;
        buf.truncate(n);
        Ok(buf)
    }
}

/// One of the devices a manifest's `matches` names: each field given must agree.
#[derive(Clone, Debug, Default, PartialEq)]
struct Match {
    vendor: Option<u16>,
    product: Option<u16>,
    usage_page: Option<u16>,
    usage: Option<u16>,
    interface: Option<i32>,
}

impl Match {
    fn of(m: &Value) -> Option<Match> {
        let u = m.get("usb")?;
        let hex = |k: &str| u.get(k).and_then(Value::as_str).and_then(|s| u16::from_str_radix(s, 16).ok());
        Some(Match {
            vendor: hex("vendor"),
            product: hex("product"),
            usage_page: hex("usagePage"),
            usage: hex("usage"),
            interface: u.get("interface").and_then(Value::as_i64).map(|i| i as i32),
        })
    }

    fn takes(&self, d: &DeviceInfo) -> bool {
        self.vendor.is_some_and(|v| v == d.vendor)
            && self.product.is_none_or(|p| p == d.product)
            && self.usage_page.is_none_or(|p| p == d.usage_page)
            && self.usage.is_none_or(|u| u == d.usage)
            && self.interface.is_none_or(|i| i == d.interface)
    }
}

/// What a core's thread holds: the system interface it is given, its
/// connection, what it may open, and the timers it has asked for.
struct Run {
    wasi: WasiCtx,
    table: ResourceTable,
    usb: Box<dyn Usb>,
    matches: Vec<Match>,
    timers: Vec<(Instant, u32)>,
    id: String,
}

impl WasiView for Run {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl Run {
    fn allowed(&self, d: &DeviceInfo) -> bool {
        self.matches.iter().any(|m| m.takes(d))
    }
}

/// The same collection: every field agrees.
fn same(a: &DeviceInfo, b: &DeviceInfo) -> bool {
    (a.vendor, a.product, a.interface, a.usage_page, a.usage, &a.path) == (b.vendor, b.product, b.interface, b.usage_page, b.usage, &b.path)
}

impl ectodeck::core::usb::Host for Run {
    fn devices(&mut self) -> Vec<DeviceInfo> {
        let all = self.usb.devices();
        all.into_iter().filter(|d| self.allowed(d)).collect()
    }

    fn open(&mut self, device: DeviceInfo) -> Result<Resource<Lent>, String> {
        // only a device the manifest matches, and one plugged in now
        if !self.allowed(&device) || !self.usb.devices().iter().any(|d| same(d, &device)) {
            return Err(format!("{} may not open that device", self.id));
        }
        let hid = self.usb.open(&device)?;
        self.table.push(Lent(hid)).map_err(|e| e.to_string())
    }
}

impl ectodeck::core::usb::HostHidDevice for Run {
    fn send_feature(&mut self, dev: Resource<Lent>, report: Vec<u8>) -> Result<(), String> {
        self.table.get_mut(&dev).map_err(|e| e.to_string())?.0.send_feature(&report)
    }
    fn get_feature(&mut self, dev: Resource<Lent>, report_id: u8, len: u32) -> Result<Vec<u8>, String> {
        self.table.get_mut(&dev).map_err(|e| e.to_string())?.0.get_feature(report_id, len as usize)
    }
    fn write(&mut self, dev: Resource<Lent>, report: Vec<u8>) -> Result<u32, String> {
        self.table.get_mut(&dev).map_err(|e| e.to_string())?.0.write(&report).map(|n| n as u32)
    }
    fn read(&mut self, dev: Resource<Lent>, timeout_ms: u32) -> Result<Vec<u8>, String> {
        self.table.get_mut(&dev).map_err(|e| e.to_string())?.0.read(timeout_ms)
    }
    fn drop(&mut self, dev: Resource<Lent>) -> wasmtime::Result<()> {
        self.table.delete(dev)?;
        Ok(())
    }
}

impl ectodeck::core::host::Host for Run {
    fn notify(&mut self, method: String, _params: String) {
        // nothing listens yet: the Z10's page asks for everything it shows
        eprintln!("ectodeck-native: {} told {method}", self.id);
    }
    fn after(&mut self, ms: u32, tag: u32) {
        self.timers.retain(|&(_, t)| t != tag);
        self.timers.push((Instant::now() + Duration::from_millis(ms as u64), tag));
    }
    fn log(&mut self, line: String) {
        eprintln!("ectodeck-native: {}: {line}", self.id);
    }
}

/// A core's manifest, as Ectodeck reads it.
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    module: PathBuf,
    matches: Vec<Match>,
}

impl Manifest {
    pub fn read(dir: &Path) -> Result<Manifest, String> {
        let path = dir.join("core.json");
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let m = ectodeck_draw::jsonc::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let s = |k: &str| m[k].as_str().map(str::to_string).ok_or_else(|| format!("{} names no {k}", path.display()));
        Ok(Manifest {
            id: s("id")?,
            name: s("name")?,
            version: s("version")?,
            module: dir.join(s("module")?),
            matches: m["matches"].as_array().map(|a| a.iter().filter_map(Match::of).collect()).unwrap_or_default(),
        })
    }
}

enum Ask {
    Call(String, String, mpsc::Sender<Result<String, String>>),
}

/// A running core: its calls go to its thread and wait there for the answer.
pub struct Core {
    pub id: String,
    ask: mpsc::Sender<Ask>,
}

impl Core {
    /// Start the core in `dir` (its manifest and module), its own files in
    /// `data`, its devices reached through `usb`. The module is made ready
    /// on the core's thread, so the first call waits for that.
    pub fn start(dir: &Path, data: PathBuf, usb: Box<dyn Usb>) -> Result<Core, String> {
        let manifest = Manifest::read(dir)?;
        let id = manifest.id.clone();
        let (ask, asked) = mpsc::channel::<Ask>();
        let name = format!("core {id}");
        std::thread::Builder::new()
            .name(name)
            .spawn(move || run(manifest, data, usb, asked))
            .map_err(|e| e.to_string())?;
        Ok(Core { id, ask })
    }

    /// Call `method` with `params`, and wait for the core's answer.
    pub fn call(&self, method: &str, params: &Value) -> Result<Value, String> {
        let (tell, told) = mpsc::channel();
        let params = if params.is_null() { String::new() } else { params.to_string() };
        self.ask.send(Ask::Call(method.to_string(), params, tell)).map_err(|_| format!("the {} core has stopped", self.id))?;
        let answer = told.recv().map_err(|_| format!("the {} core has stopped", self.id))??;
        if answer.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&answer).map_err(|e| format!("the {} core answered {method} with {e}", self.id))
    }
}

/// The module, ready to call, and what it runs with.
struct Live {
    store: Store<Run>,
    core: World,
}

fn make(engine: &Engine, component: &Component, m: &Manifest, data: &Path, usb: Box<dyn Usb>) -> Result<Live, String> {
    std::fs::create_dir_all(data).map_err(|e| format!("{}: {e}", data.display()))?;
    let wasi = WasiCtxBuilder::new()
        .inherit_stderr()
        .preopened_dir(data, "/data", FsPerms::ReadWrite)
        .map_err(|e| format!("{}: {e}", data.display()))?
        .build();
    let run = Run { wasi, table: ResourceTable::new(), usb, matches: m.matches.clone(), timers: Vec::new(), id: m.id.clone() };
    let mut linker = Linker::<Run>::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(|e| e.to_string())?;
    World::add_to_linker::<_, HasSelf<_>>(&mut linker, |r| r).map_err(|e| e.to_string())?;
    let mut store = Store::new(engine, run);
    let core = World::instantiate(&mut store, component, &linker).map_err(|e| format!("the {} core would not start: {e}", m.id))?;
    Ok(Live { store, core })
}

fn run(m: Manifest, data: PathBuf, usb: Box<dyn Usb>, asked: mpsc::Receiver<Ask>) {
    let t0 = Instant::now();
    let engine = Engine::default();
    let made = Component::from_file(&engine, &m.module)
        .map_err(|e| format!("the {} core's module ({}) would not load: {e}", m.id, m.module.display()))
        .and_then(|c| make(&engine, &c, &m, &data, usb));
    eprintln!("ectodeck-native: the {} core ({} {}) made ready in {} ms", m.id, m.name, m.version, t0.elapsed().as_millis());
    let mut live = match made {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ectodeck-native: {e}");
            // every call hears why
            while let Ok(Ask::Call(_, _, tell)) = asked.recv() {
                let _ = tell.send(Err(e.clone()));
            }
            return;
        }
    };
    loop {
        // the next timer due, if any
        let due = live.store.data().timers.iter().map(|&(at, _)| at).min();
        let a = match due {
            Some(at) => match asked.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(a) => Some(a),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
            None => match asked.recv() {
                Ok(a) => Some(a),
                Err(_) => break,
            },
        };
        if let Some(Ask::Call(method, params, tell)) = a {
            let t = Instant::now();
            let r = live.core.call_call(&mut live.store, &method, &params).unwrap_or_else(|trap| Err(format!("the {} core failed: {trap}", m.id)));
            slow(&m.id, &method, t);
            let _ = tell.send(r);
        }
        let now = Instant::now();
        while let Some(i) = live.store.data().timers.iter().position(|&(at, _)| at <= now) {
            let (_, tag) = live.store.data_mut().timers.remove(i);
            let t = Instant::now();
            if let Err(trap) = live.core.call_timer(&mut live.store, tag) {
                eprintln!("ectodeck-native: the {} core failed on its timer: {trap}", m.id);
            }
            slow(&m.id, &format!("timer {tag}"), t);
        }
    }
}

/// A line for the log when a call kept the core's thread past a few of the
/// preview's beats: everything else the core does waited for it.
fn slow(id: &str, what: &str, since: Instant) {
    let ms = since.elapsed().as_millis();
    if ms > 200 {
        eprintln!("ectodeck-native: the {id} core took {ms} ms on {what}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io::Read;
    use std::sync::{Arc, Mutex};

    /// The Z10's collection, as a fake computer lists it.
    fn z10() -> DeviceInfo {
        DeviceInfo { vendor: 0x3842, product: 0x2611, interface: 1, usage_page: 0x0008, usage: 0x004B, path: "fake".into() }
    }

    /// A computer with one fake Z10, whose collection is `hid`.
    struct One(Option<Box<dyn Hid>>);

    impl Usb for One {
        fn devices(&mut self) -> Vec<DeviceInfo> {
            vec![z10(), DeviceInfo { vendor: 0x1038, product: 0x1722, interface: 0, usage_page: 0xFFC0, usage: 1, path: "mouse".into() }]
        }
        fn open(&mut self, _: &DeviceInfo) -> Result<Box<dyn Hid>, String> {
            self.0.take().ok_or_else(|| "opened twice".to_string())
        }
    }

    /// A recorded exchange: each report sent ("set") or answer read ("get").
    type Log = Arc<Mutex<VecDeque<(String, Vec<u8>)>>>;

    /// A keyboard that replays a recorded exchange, as ectotype-core's parity
    /// test does: every report sent must be the next in the log, and every read
    /// gives the next recorded answer. The first difference is kept.
    struct Replay {
        log: Log,
        wrong: Arc<Mutex<Option<String>>>,
    }

    impl Replay {
        fn next(&mut self, kind: &str) -> Result<Vec<u8>, String> {
            let e = self.log.lock().unwrap().pop_front();
            let r = match e {
                Some((k, b)) if k == kind => Ok(b),
                Some((k, _)) => Err(format!("{kind} where the original did {k}")),
                None => Err(format!("{kind} past the end of the original")),
            };
            if let Err(e) = &r {
                self.wrong.lock().unwrap().get_or_insert(e.clone());
            }
            r
        }
    }

    impl Hid for Replay {
        fn send_feature(&mut self, report: &[u8]) -> Result<(), String> {
            let want = self.next("set")?;
            if want != report {
                let e = format!("sent {report:02x?}, the original sent {want:02x?}");
                self.wrong.lock().unwrap().get_or_insert(e.clone());
                return Err(e);
            }
            Ok(())
        }
        fn get_feature(&mut self, report_id: u8, len: usize) -> Result<Vec<u8>, String> {
            let answer = self.next("get")?;
            if (answer[0], answer.len()) != (report_id, len) {
                let e = format!("read report {report_id} of {len}, the original read {} of {}", answer[0], answer.len());
                self.wrong.lock().unwrap().get_or_insert(e.clone());
                return Err(e);
            }
            Ok(answer)
        }
        fn write(&mut self, _: &[u8]) -> Result<usize, String> {
            Err("the Z10 is sent only feature reports".into())
        }
        fn read(&mut self, _: u32) -> Result<Vec<u8>, String> {
            Err("the Z10 is sent only feature reports".into())
        }
    }

    fn core_dir() -> PathBuf {
        let dir = crate::cores::dir().join("evga-z10");
        assert!(dir.join("core.wasm").exists(), "the Z10's module isn't built: run tools/build-cores.sh");
        dir
    }

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    /// Every editor action in ectotype-core's parity vectors (the original
    /// editor's exchanges with the keyboard), run through the Z10's module:
    /// the same reports sent byte for byte, the same answers given.
    #[test]
    fn the_z10_module_sends_what_the_original_editor_sent() {
        let root = crate::cores::dir().join("../parity");
        let mut files = vec![root.join("vectors.json.gz")];
        if let Ok(dir) = std::fs::read_dir(root.join("private")) {
            files.extend(dir.flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with(".json.gz")));
        }
        let data = std::env::temp_dir().join(format!("ectodeck-core-test-{}", std::process::id()));
        let mut cases = 0;
        for f in files {
            let mut text = String::new();
            flate2::read::GzDecoder::new(std::fs::File::open(&f).unwrap()).read_to_string(&mut text).unwrap();
            let v: Value = serde_json::from_str(&text).unwrap();
            for c in v["device"]["editor"].as_array().unwrap() {
                let name = format!("{}: {}", f.file_name().unwrap().to_string_lossy(), c["name"].as_str().unwrap());
                let log: Log = Arc::new(Mutex::new(c["log"].as_array().unwrap().iter().map(|e| (e[0].as_str().unwrap().to_string(), unhex(e[1].as_str().unwrap()))).collect()));
                let wrong = Arc::new(Mutex::new(None));
                let hid = Replay { log: log.clone(), wrong: wrong.clone() };
                let core = Core::start(&core_dir(), data.clone(), Box::new(One(Some(Box::new(hid))))).unwrap();
                core.call("initialize", &serde_json::json!({ "backups": false })).unwrap();
                let got = match c["name"].as_str().unwrap().split(' ').next().unwrap() {
                    "state" => core.call("state", &Value::Null),
                    "save" => core.call("save", &serde_json::json!({ "n": c["n"], "body": c["body"] })),
                    "write_macro" => core.call("write_macro", &serde_json::json!({ "index": c["index"], "body": c["body"] })),
                    "activate" => core.call("activate", &serde_json::json!({ "n": c["n"] })),
                    other => panic!("no replay for {other}"),
                };
                if c["error"] == true {
                    assert!(got.is_err(), "{name}: should fail");
                } else {
                    assert_eq!(*wrong.lock().unwrap(), None, "{name}");
                    assert_eq!(got.unwrap_or_else(|e| panic!("{name}: {e}")), c["result"], "{name}: result");
                }
                assert!(log.lock().unwrap().is_empty(), "{name}: stopped short of the original");
                cases += 1;
            }
        }
        let _ = std::fs::remove_dir_all(&data);
        assert!(cases > 0);
        eprintln!("{cases} editor actions replayed through the module");
    }

    /// A core opens only what its manifest matches: the Z10's never sees the mouse.
    #[test]
    fn a_core_sees_only_the_devices_its_manifest_names() {
        let m = Manifest::read(&core_dir()).unwrap();
        let mut run_ok = 0;
        for d in One(None).devices() {
            if m.matches.iter().any(|x| x.takes(&d)) {
                run_ok += 1;
                assert_eq!(d.vendor, 0x3842);
            }
        }
        assert_eq!(run_ok, 1);
    }

    /// A keyboard that takes every report and says it did, keeping when each
    /// preview frame (report 7) came.
    struct Clock(Arc<Mutex<Vec<Instant>>>, Vec<u8>);

    impl Hid for Clock {
        fn send_feature(&mut self, report: &[u8]) -> Result<(), String> {
            if report[0] == 7 {
                self.0.lock().unwrap().push(Instant::now());
            }
            self.1 = report.to_vec();
            Ok(())
        }
        fn get_feature(&mut self, report_id: u8, len: usize) -> Result<Vec<u8>, String> {
            let last = &self.1;
            let mut r = vec![report_id, 0xEA, 0x02, *last.get(3).unwrap_or(&0), *last.get(4).unwrap_or(&0), 0, 0xC0];
            r.resize(len, 0);
            Ok(r)
        }
        fn write(&mut self, _: &[u8]) -> Result<usize, String> {
            Ok(0)
        }
        fn read(&mut self, _: u32) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
    }

    /// A fake Z10 that keeps every report sent to it, answering reads as
    /// `Clock` does.
    struct Sent(Arc<Mutex<Vec<Vec<u8>>>>, Clock);

    impl Hid for Sent {
        fn send_feature(&mut self, report: &[u8]) -> Result<(), String> {
            self.0.lock().unwrap().push(report.to_vec());
            self.1.send_feature(report)
        }
        fn get_feature(&mut self, report_id: u8, len: usize) -> Result<Vec<u8>, String> {
            self.1.get_feature(report_id, len)
        }
        fn write(&mut self, r: &[u8]) -> Result<usize, String> {
            self.1.write(r)
        }
        fn read(&mut self, t: u32) -> Result<Vec<u8>, String> {
            self.1.read(t)
        }
    }

    /// Off previewed after Wave: the keys stay in the preview, every one
    /// black, and the frame that leaves the preview (which showed the stored
    /// lighting) is not sent again after the switch.
    #[test]
    fn off_previews_every_key_black() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let data = std::env::temp_dir().join(format!("ectodeck-core-off-{}", std::process::id()));
        let fake = Sent(sent.clone(), Clock(Arc::new(Mutex::new(Vec::new())), Vec::new()));
        let core = Core::start(&core_dir(), data.clone(), Box::new(One(Some(Box::new(fake))))).unwrap();
        core.call("initialize", &serde_json::json!({ "backups": false })).unwrap();
        let wave = serde_json::json!({ "mode": "wave", "wave": { "cycle": 1000 } });
        core.call("preview", &serde_json::json!({ "lighting": wave, "tab": "test" })).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let before = sent.lock().unwrap().len();
        let r = core.call("preview", &serde_json::json!({ "lighting": { "mode": "off" }, "tab": "test" })).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let after: Vec<Vec<u8>> = sent.lock().unwrap()[before..].to_vec();
        let _ = std::fs::remove_dir_all(&data);
        assert_eq!(r["mode"], "keys");
        // the switch: leave the effect, then nothing but black keys
        assert_eq!(after[0][..4], [6, 0xEA, 0x02, 0], "stop the key stream first");
        assert_eq!(after[1][..4], [7, 0xEA, 0x02, 0], "leave the effect");
        let keys = &after[2..];
        assert!(keys.len() > 4, "{} frames of keys in 300 ms", keys.len());
        for k in keys {
            assert_eq!(k[..4], [6, 0xEA, 0x02, 1], "a key frame");
            assert_eq!(k.len(), 8 + 4 * ectotype_core::protocol::NKEYS);
            assert!(k[8..].chunks(4).all(|c| c[1..] == [0, 0, 0]), "every key black");
        }
    }

    /// The preview's beat as the module keeps it, on Ectodeck's timer: a frame
    /// every 40 ms, as the keyboard code's own thread sends them. Prints the
    /// spacing; fails if the frames come more than 5 ms late on average.
    #[test]
    fn the_preview_keeps_its_beat() {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let data = std::env::temp_dir().join(format!("ectodeck-core-beat-{}", std::process::id()));
        let core = Core::start(&core_dir(), data.clone(), Box::new(One(Some(Box::new(Clock(frames.clone(), Vec::new())))))).unwrap();
        core.call("initialize", &serde_json::json!({ "backups": false })).unwrap();
        let lighting = serde_json::json!({ "mode": "breathing", "breathing": { "speed": 2, "colors": [{ "brightness": 255, "color": [255, 0, 0] }] } });
        core.call("preview", &serde_json::json!({ "lighting": lighting, "tab": "test" })).unwrap();
        std::thread::sleep(Duration::from_millis(2100));
        core.call("preview", &serde_json::json!({ "stop": true, "tab": "test" })).unwrap();
        let spacing = |f: &[Instant]| {
            let gaps: Vec<f64> = f.windows(2).map(|w| (w[1] - w[0]).as_secs_f64() * 1000.0).collect();
            (gaps.iter().sum::<f64>() / gaps.len() as f64, gaps.iter().cloned().fold(0.0, f64::max), gaps.iter().cloned().fold(f64::MAX, f64::min))
        };
        let f = frames.lock().unwrap().clone();
        let (mean, worst, least) = spacing(&f);
        eprintln!("module: {} frames, {mean:.2} ms apart on average, {least:.2} to {worst:.2} ms", f.len());
        // the keyboard code's own thread, on the same fake keyboard, for comparison
        struct Kept(Clock);
        impl ectotype_core::Transport for Kept {
            fn set_feature(&mut self, b: &[u8]) -> ectotype_core::Result<()> {
                self.0.send_feature(b).map_err(ectotype_core::Error::Link)
            }
            fn get_feature(&mut self, rid: u8, len: usize) -> ectotype_core::Result<Vec<u8>> {
                self.0.get_feature(rid, len).map_err(ectotype_core::Error::Link)
            }
            fn path(&self) -> String {
                "fake".into()
            }
        }
        let native = Arc::new(Mutex::new(Vec::new()));
        let n2 = native.clone();
        let opener: ectotype_core::conn::Opener = Arc::new(move || Ok(ectotype_core::Z10::new(Box::new(Kept(Clock(n2.clone(), Vec::new()))) as Box<dyn ectotype_core::Transport>)));
        let kb = ectotype_core::Keyboard::new(ectotype_core::Conn::new(opener), None);
        kb.preview(&serde_json::json!({ "lighting": lighting, "tab": "test" })).unwrap();
        std::thread::sleep(Duration::from_millis(2100));
        kb.preview(&serde_json::json!({ "stop": true, "tab": "test" })).unwrap();
        let g = native.lock().unwrap().clone();
        let (nmean, nworst, nleast) = spacing(&g);
        eprintln!("thread: {} frames, {nmean:.2} ms apart on average, {nleast:.2} to {nworst:.2} ms", g.len());
        let _ = std::fs::remove_dir_all(&data);
        assert!(f.len() > 40, "{} frames in 2.1 s", f.len());
        assert!((mean - nmean).abs() < 2.0, "the module {mean:.2} ms apart, the thread {nmean:.2}");
    }
}
