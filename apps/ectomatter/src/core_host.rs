//! Cores, run inside Ectomatter as WebAssembly components (the interface is
//! wit/core.wit). Each core runs on a thread of its own, where its calls and
//! its timers take turns; in the middle of a long call, the core can take the
//! calls waiting that it can answer at once (a preview), so they don't wait. A core reaches the USB devices its manifest
//! (core.json) matches, and no others, through the connection Ectomatter lends
//! it; and the clocks and its own folder, seen as /data. Nothing else.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;
use wasmtime::component::{Component, HasSelf, Linker, Resource, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

/// The interface's bindings: the world's own type (`World`), and Ectomatter's
/// side of its imports.
mod world {
    wasmtime::component::bindgen!({
        path: "../../wit",
        world: "core",
        with: { "ectomatter:core/usb.hid-device": super::Lent, "ectomatter:core/net.tcp-stream": super::Stream },
    });
    pub use Core as World;
}

use world::World;
use world::ectomatter;
use world::ectomatter::core::usb::DeviceInfo;

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

/// A connection a core has made, to an address its manifest names.
pub struct Stream(std::net::TcpStream);

/// What a core's manifest says it does beyond its USB devices (its `uses`),
/// which is all of the computer it may reach through Ectomatter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Uses {
    /// the monitors: read them, and have Ectomatter change them while a game runs
    pub monitors: bool,
    /// the addresses it may connect to, each with what is there ("OpenRGB")
    pub network: Vec<(String, String)>,
    /// the web hosts it may ask, each with whose it is ("Open-Meteo")
    pub web: Vec<(String, String)>,
    /// the files and folders it may read, and nothing in them changed
    pub read: Vec<String>,
    /// NVIDIA's driver: what it says of each card
    pub nvidia: bool,
    /// how games start: the recipe Ectomatter's launcher starts each game
    /// from, and the Wine tools run in a game's prefix (game/launch)
    pub launch: bool,
    /// anything else it names, as it names it ("sound-capture")
    pub other: Vec<String>,
}

impl Uses {
    pub fn of(v: &Value) -> Uses {
        let mut u = Uses::default();
        for x in v.as_array().into_iter().flatten() {
            match x {
                Value::String(s) if s == "monitors" => u.monitors = true,
                Value::String(s) if s == "nvidia" => u.nvidia = true,
                Value::String(s) if s == "launch" => u.launch = true,
                Value::String(s) => u.other.push(s.clone()),
                Value::Object(o) => {
                    let to = |a: &str| o.get("to").and_then(Value::as_str).unwrap_or(a).to_string();
                    if let Some(a) = o.get("network").and_then(Value::as_str) {
                        u.network.push((a.to_string(), to(a)));
                    }
                    if let Some(h) = o.get("web").and_then(Value::as_str) {
                        u.web.push((h.to_lowercase(), to(h)));
                    }
                    // (a path as written, absolute and with no way up out of it)
                    if let Some(f) = o.get("read").and_then(Value::as_str).filter(|f| plain_path(f)) {
                        u.read.push(f.trim_end_matches('/').to_string());
                    }
                }
                _ => {}
            }
        }
        u
    }
}

/// The computer as a core may read it through Ectomatter, beyond its USB
/// devices: its monitors, as the system interface's `monitors` gives them;
/// its files (read only); NVIDIA's driver; and the web. Ectomatter checks
/// what a package's manifest allows before asking any of these.
pub trait Computer: Send + Sync {
    fn monitors(&self) -> Result<Value, String>;
    fn read(&self, path: &Path) -> Result<String, String>;
    fn entries(&self, path: &Path) -> Result<Vec<String>, String>;
    fn resolve(&self, path: &Path) -> Result<PathBuf, String>;
    fn nvidia(&self, pci: &str, what: &str) -> Result<Value, String>;
    fn fetch(&self, url: &str, timeout: Duration) -> Result<String, String>;
}

/// A path a manifest may name to read: absolute, with nothing that leads up.
fn plain_path(p: &str) -> bool {
    let p = Path::new(p);
    p.is_absolute() && !p.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::CurDir))
}

/// The host of a web address ("https://api.open-meteo.com/v1/forecast?..."
/// is "api.open-meteo.com"), or none where it isn't one Ectomatter asks.
fn host_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    // (no name and password before the host, and the port isn't the host's)
    (!host.contains('@')).then(|| host.split(':').next().unwrap_or(host).to_lowercase()).filter(|h| !h.is_empty())
}

/// A computer with no devices, for asking a core what it says of itself
/// without it reaching any.
pub struct NoDevices;

impl Usb for NoDevices {
    fn devices(&mut self) -> Vec<DeviceInfo> {
        Vec::new()
    }
    fn open(&mut self, _: &DeviceInfo) -> Result<Box<dyn Hid>, String> {
        Err("no device".into())
    }
}

/// What the core in `dir` says of itself (`device/describe`), asked with no
/// devices to reach and its files in a folder of its own, gone after: for a
/// window drawn offscreen, which never reaches a device.
pub fn describe(dir: &Path) -> Option<Value> {
    let data = std::env::temp_dir().join(format!("ectomatter-describe-{}", std::process::id()));
    let core = Core::start(dir, data.clone(), Box::new(NoDevices)).ok()?;
    let d = core.call("device/describe", &Value::Null).ok();
    let _ = core.call("shutdown", &Value::Null);
    let _ = std::fs::remove_dir_all(&data);
    d
}

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
    /// as evga-z10's own HidTransport reads one: the whole buffer, whatever
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
/// connection, what it may open, the timers it has asked for, and the calls
/// for it.
struct Run {
    wasi: WasiCtx,
    table: ResourceTable,
    usb: Box<dyn Usb>,
    matches: Vec<Match>,
    uses: Uses,
    computer: Option<std::sync::Arc<dyn Computer>>,
    timers: Vec<(Instant, u32)>,
    id: String,
    asked: Inbox,
    /// calls that came in during a long call, in order
    held: VecDeque<Ask>,
    /// calls the core took with take-waiting, until it answers them
    taken: Vec<(u32, Tell)>,
    next_id: u32,
    /// what the core has told unasked (device/added and the like), until read
    told: Told,
}

/// A core's notices, as its thread keeps them and its handle reads them, and
/// what to wake when one comes (Core::on_told): a control pressed can't wait
/// for its page to look.
#[derive(Clone, Default)]
struct Told(std::sync::Arc<std::sync::Mutex<Notices>>);

#[derive(Default)]
struct Notices {
    list: Vec<(String, Value)>,
    wake: Option<Box<dyn Fn() + Send>>,
}

type Tell = mpsc::Sender<Result<String, String>>;

/// The calls for a core, as its thread and (in a long call) the core reach them.
type Inbox = std::sync::Arc<std::sync::Mutex<mpsc::Receiver<Ask>>>;

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

impl ectomatter::core::usb::Host for Run {
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

impl ectomatter::core::usb::HostHidDevice for Run {
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

impl Run {
    /// A path its manifest lets it read: a file it names, or one in a folder it names.
    fn may_read(&self, path: &str) -> Result<PathBuf, String> {
        if !plain_path(path) {
            return Err(format!("{path} isn’t a path Ectomatter reads: it must be whole, from /"));
        }
        let p = Path::new(path);
        if self.uses.read.iter().any(|f| p.starts_with(f)) {
            Ok(p.to_path_buf())
        } else {
            Err(format!("{} may not read {path}: its manifest doesn’t name it", self.id))
        }
    }

    /// Where a path it may read really is, its links followed, and only if
    /// that too is somewhere it may read: no link takes it outside.
    fn real(&self, path: &str) -> Result<PathBuf, String> {
        let p = self.may_read(path)?;
        let real = self.reach("files")?.resolve(&p)?;
        self.may_read(&real.to_string_lossy()).map_err(|_| format!("{path} leads outside what {} may read", self.id))
    }

    fn reach(&self, what: &str) -> Result<&std::sync::Arc<dyn Computer>, String> {
        self.computer.as_ref().ok_or_else(|| format!("Ectomatter can’t reach this computer’s {what}"))
    }
}

impl ectomatter::core::system::Host for Run {
    fn monitors(&mut self) -> Result<String, String> {
        if !self.uses.monitors {
            return Err(format!("{} may not read the monitors: its manifest doesn’t name them", self.id));
        }
        self.reach("monitors")?.monitors().map(|v| v.to_string())
    }

    fn read(&mut self, path: String) -> Result<String, String> {
        let real = self.real(&path)?;
        self.reach("files")?.read(&real)
    }

    fn entries(&mut self, path: String) -> Result<Vec<String>, String> {
        let real = self.real(&path)?;
        self.reach("files")?.entries(&real)
    }

    fn resolve(&mut self, path: String) -> Result<String, String> {
        let p = self.may_read(&path)?;
        let to = self.reach("files")?.resolve(&p)?.to_string_lossy().to_string();
        // (a link may not lead it to what it may not read)
        self.may_read(&to).map(|_| to).map_err(|_| format!("{path} leads outside what {} may read", self.id))
    }

    fn nvidia(&mut self, pci: String, what: String) -> Result<String, String> {
        if !self.uses.nvidia {
            return Err(format!("{} may not ask NVIDIA’s driver: its manifest doesn’t name it", self.id));
        }
        self.reach("NVIDIA driver")?.nvidia(&pci, &what).map(|v| v.to_string())
    }
}

/// An address as a manifest names it, as one to connect to: a number's, or
/// this computer's by its name (no name is looked up anywhere else).
fn address_of(a: &str) -> Option<std::net::SocketAddr> {
    a.parse().ok().or_else(|| {
        let port = a.strip_prefix("localhost:")?.parse().ok()?;
        Some(std::net::SocketAddr::from(([127, 0, 0, 1], port)))
    })
}

impl ectomatter::core::net::Host for Run {
    fn connect(&mut self, address: String, timeout_ms: u32) -> Result<Resource<Stream>, String> {
        // only an address the manifest names
        if !self.uses.network.iter().any(|(a, _)| *a == address) {
            return Err(format!("{} may not connect to {address}", self.id));
        }
        let at = address_of(&address).ok_or_else(|| format!("{address} isn’t an address Ectomatter can connect to"))?;
        let s = std::net::TcpStream::connect_timeout(&at, Duration::from_millis(timeout_ms.max(1) as u64)).map_err(|e| e.to_string())?;
        let _ = s.set_nodelay(true);
        self.table.push(Stream(s)).map_err(|e| e.to_string())
    }

    fn fetch(&mut self, url: String, timeout_ms: u32) -> Result<String, String> {
        // only a host the manifest names
        let host = host_of(&url).ok_or_else(|| format!("{url} isn’t a web address Ectomatter asks"))?;
        if !self.uses.web.iter().any(|(h, _)| *h == host) {
            return Err(format!("{} may not ask {host}: its manifest doesn’t name it", self.id));
        }
        self.reach("web")?.fetch(&url, Duration::from_millis(timeout_ms.max(1) as u64))
    }
}

impl ectomatter::core::net::HostTcpStream for Run {
    fn write(&mut self, s: Resource<Stream>, bytes: Vec<u8>) -> Result<u32, String> {
        use std::io::Write;
        let s = &mut self.table.get_mut(&s).map_err(|e| e.to_string())?.0;
        s.write_all(&bytes).map_err(|e| e.to_string())?;
        Ok(bytes.len() as u32)
    }
    fn read(&mut self, s: Resource<Stream>, max: u32, timeout_ms: u32) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let s = &mut self.table.get_mut(&s).map_err(|e| e.to_string())?.0;
        let mut buf = vec![0u8; max.clamp(1, 1 << 20) as usize];
        let got = if timeout_ms == 0 {
            s.set_nonblocking(true).map_err(|e| e.to_string())?;
            let r = s.read(&mut buf);
            let _ = s.set_nonblocking(false);
            r
        } else {
            s.set_read_timeout(Some(Duration::from_millis(timeout_ms as u64))).map_err(|e| e.to_string())?;
            s.read(&mut buf)
        };
        match got {
            Ok(0) => Err("the connection has closed".into()),
            Ok(n) => {
                buf.truncate(n);
                Ok(buf)
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => Ok(Vec::new()),
            Err(e) => Err(e.to_string()),
        }
    }
    fn drop(&mut self, s: Resource<Stream>) -> wasmtime::Result<()> {
        self.table.delete(s)?;
        Ok(())
    }
}

impl ectomatter::core::host::Host for Run {
    fn notify(&mut self, method: String, params: String) {
        // (a control's every press isn't worth a line)
        if !method.starts_with("input/") {
            eprintln!("ectomatter: {} told {method}", self.id);
        }
        let p = serde_json::from_str(&params).unwrap_or(Value::Null);
        let mut n = self.told.0.lock().unwrap_or_else(|p| p.into_inner());
        n.list.push((method, p));
        if let Some(wake) = &n.wake {
            wake();
        }
    }
    fn after(&mut self, ms: u32, tag: u32) {
        self.timers.retain(|&(_, t)| t != tag);
        self.timers.push((Instant::now() + Duration::from_millis(ms as u64), tag));
    }
    fn log(&mut self, line: String) {
        eprintln!("ectomatter: {}: {line}", self.id);
    }
    fn take_waiting(&mut self, methods: Vec<String>) -> Vec<ectomatter::core::host::Waiting> {
        while let Ok(a) = self.asked.lock().unwrap_or_else(|p| p.into_inner()).try_recv() {
            self.held.push_back(a);
        }
        let mut out = Vec::new();
        let mut kept = VecDeque::new();
        for a in self.held.drain(..) {
            match a {
                Ask::Call(method, params, tell) if methods.contains(&method) => {
                    self.next_id = self.next_id.wrapping_add(1);
                    self.taken.push((self.next_id, tell));
                    out.push(ectomatter::core::host::Waiting { id: self.next_id, method, params });
                }
                other => kept.push_back(other),
            }
        }
        self.held = kept;
        out
    }
    fn answer(&mut self, id: u32, reply: Result<String, String>) {
        if let Some(i) = self.taken.iter().position(|(t, _)| *t == id) {
            let _ = self.taken.remove(i).1.send(reply);
        }
    }
}

impl Run {
    /// After each call into the core: a call it took and never answered
    /// hears so, rather than waiting forever.
    fn unanswered(&mut self) {
        for (_, tell) in self.taken.drain(..) {
            let _ = tell.send(Err(format!("the {} core took a call and never answered it", self.id)));
        }
    }
}

/// A core's manifest, as Ectomatter reads it (and the whole of it as written,
/// for what the rest of Ectomatter reads of it: its description, its glyph).
#[derive(Clone)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    module: PathBuf,
    matches: Vec<Match>,
    pub uses: Uses,
    pub all: Value,
}

impl Manifest {
    pub fn read(dir: &Path) -> Result<Manifest, String> {
        let path = dir.join("core.json");
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let m = ectomatter_draw::jsonc::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let s = |k: &str| m[k].as_str().map(str::to_string).ok_or_else(|| format!("{} names no {k}", path.display()));
        Ok(Manifest {
            id: s("id")?,
            name: s("name")?,
            version: s("version")?,
            module: dir.join(s("module")?),
            matches: m["matches"].as_array().map(|a| a.iter().filter_map(Match::of).collect()).unwrap_or_default(),
            uses: Uses::of(&m["uses"]),
            all: m,
        })
    }

    /// Whether it reaches any USB device: a device's core does; a plugin
    /// that adds to Ectomatter's pages may not.
    pub fn has_usb(&self) -> bool {
        !self.matches.is_empty()
    }

    /// The USB ids of the first device it reaches, for asking the system how
    /// that device is (devices.rs).
    pub fn usb_ids(&self) -> Option<(u16, u16)> {
        self.matches.first().and_then(|m| Some((m.vendor?, m.product?)))
    }
}

enum Ask {
    Call(String, String, mpsc::Sender<Result<String, String>>),
}

/// A running core: its calls go to its thread and wait there for the answer.
/// A copy reaches the same core, from another thread.
#[derive(Clone)]
pub struct Core {
    pub id: String,
    ask: mpsc::Sender<Ask>,
    told: Told,
}

impl Core {
    /// Start the core in `dir` (its manifest and module), its own files in
    /// `data`, its devices reached through `usb`. The module is made ready
    /// on the core's thread, so the first call waits for that.
    pub fn start(dir: &Path, data: PathBuf, usb: Box<dyn Usb>) -> Result<Core, String> {
        Core::start_with(dir, data, usb, None)
    }

    /// The same, the core also reaching the computer as its manifest's `uses`
    /// allows (its monitors, through `computer`).
    pub fn start_with(dir: &Path, data: PathBuf, usb: Box<dyn Usb>, computer: Option<std::sync::Arc<dyn Computer>>) -> Result<Core, String> {
        let manifest = Manifest::read(dir)?;
        let id = manifest.id.clone();
        let (ask, asked) = mpsc::channel::<Ask>();
        let told: Told = Default::default();
        let name = format!("core {id}");
        let t = told.clone();
        std::thread::Builder::new()
            .name(name)
            .spawn(move || run(manifest, data, usb, computer, asked, t))
            .map_err(|e| e.to_string())?;
        Ok(Core { id, ask, told })
    }

    /// What the core has told since this was last asked, oldest first.
    pub fn told(&self) -> Vec<(String, Value)> {
        std::mem::take(&mut self.told.0.lock().unwrap_or_else(|p| p.into_inner()).list)
    }

    /// Run `wake` as each notice comes, on the core's thread; whoever it wakes
    /// reads them with `told`.
    pub fn on_told(&self, wake: impl Fn() + Send + 'static) {
        self.told.0.lock().unwrap_or_else(|p| p.into_inner()).wake = Some(Box::new(wake));
    }

    /// Call `method` with `params` and go on without its answer: for what a
    /// page hands a plugin as it changes (its Settings values), which mustn't
    /// wait behind a plugin busy on the network.
    pub fn tell(&self, method: &str, params: &Value) {
        let (tell, _) = mpsc::channel();
        let params = if params.is_null() { String::new() } else { params.to_string() };
        let _ = self.ask.send(Ask::Call(method.to_string(), params, tell));
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

fn make(engine: &Engine, component: &Component, m: &Manifest, data: &Path, usb: Box<dyn Usb>, computer: Option<std::sync::Arc<dyn Computer>>, asked: Inbox, told: Told) -> Result<Live, String> {
    std::fs::create_dir_all(data).map_err(|e| format!("{}: {e}", data.display()))?;
    let wasi = WasiCtxBuilder::new()
        .inherit_stderr()
        .preopened_dir(data, "/data", FsPerms::ReadWrite)
        .map_err(|e| format!("{}: {e}", data.display()))?
        .build();
    let run = Run { wasi, table: ResourceTable::new(), usb, matches: m.matches.clone(), uses: m.uses.clone(), computer, timers: Vec::new(), id: m.id.clone(), asked, held: VecDeque::new(), taken: Vec::new(), next_id: 0, told };
    let mut linker = Linker::<Run>::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(|e| e.to_string())?;
    World::add_to_linker::<_, HasSelf<_>>(&mut linker, |r| r).map_err(|e| e.to_string())?;
    let mut store = Store::new(engine, run);
    let core = World::instantiate(&mut store, component, &linker).map_err(|e| format!("the {} core would not start: {e}", m.id))?;
    Ok(Live { store, core })
}

fn run(m: Manifest, data: PathBuf, usb: Box<dyn Usb>, computer: Option<std::sync::Arc<dyn Computer>>, asked: mpsc::Receiver<Ask>, told: Told) {
    let t0 = Instant::now();
    let engine = Engine::default();
    let inbox: Inbox = std::sync::Arc::new(std::sync::Mutex::new(asked));
    let made = Component::from_file(&engine, &m.module)
        .map_err(|e| format!("the {} core's module ({}) would not load: {e}", m.id, m.module.display()))
        .and_then(|c| make(&engine, &c, &m, &data, usb, computer, inbox.clone(), told));
    let asked = inbox;
    eprintln!("ectomatter: the {} {} ({} {}) made ready in {} ms", m.id, crate::packages::Kind::of(&m.all).word(), m.name, m.version, t0.elapsed().as_millis());
    let mut live = match made {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ectomatter: {e}");
            // every call hears why
            while let Ok(Ask::Call(_, _, tell)) = asked.lock().unwrap_or_else(|p| p.into_inner()).recv() {
                let _ = tell.send(Err(e.clone()));
            }
            return;
        }
    };
    loop {
        // a call held from during the last, or the next to come in before
        // the next timer due, if any
        let held = live.store.data_mut().held.pop_front();
        let a = match held {
            Some(a) => Some(a),
            None => {
                let due = live.store.data().timers.iter().map(|&(at, _)| at).min();
                let inbox = asked.lock().unwrap_or_else(|p| p.into_inner());
                match due {
                    Some(at) => match inbox.recv_timeout(at.saturating_duration_since(Instant::now())) {
                        Ok(a) => Some(a),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    },
                    None => match inbox.recv() {
                        Ok(a) => Some(a),
                        Err(_) => break,
                    },
                }
            }
        };
        if let Some(Ask::Call(method, params, tell)) = a {
            let t = Instant::now();
            let r = live.core.call_call(&mut live.store, &method, &params).unwrap_or_else(|trap| Err(format!("the {} core failed: {trap}", m.id)));
            live.store.data_mut().unanswered();
            slow(&m, &method, t);
            let _ = tell.send(r);
        }
        let now = Instant::now();
        while let Some(i) = live.store.data().timers.iter().position(|&(at, _)| at <= now) {
            let (_, tag) = live.store.data_mut().timers.remove(i);
            let t = Instant::now();
            if let Err(trap) = live.core.call_timer(&mut live.store, tag) {
                eprintln!("ectomatter: the {} {} failed on its timer: {trap}", m.id, crate::packages::Kind::of(&m.all).word());
            }
            live.store.data_mut().unanswered();
            slow(&m, &format!("timer {tag}"), t);
        }
    }
}

/// A line for the log when a call kept a package's thread past a few of the
/// preview's beats: everything else it does waited for it.
fn slow(m: &Manifest, what: &str, since: Instant) {
    let ms = since.elapsed().as_millis();
    if ms > 200 {
        eprintln!("ectomatter: the {} {} took {ms} ms on {what}", m.id, crate::packages::Kind::of(&m.all).word());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io::Read;
    use std::sync::{Arc, Mutex};

    /// What a manifest may name to reach: a web host by its name alone (no
    /// name and password before it, no other kind of address); a file or
    /// folder whole from /, with no way up out of it.
    #[test]
    fn a_package_reaches_only_what_its_manifest_names() {
        assert_eq!(host_of("https://api.open-meteo.com/v1/forecast?x=1").as_deref(), Some("api.open-meteo.com"));
        assert_eq!(host_of("https://API.Weather.gov:443/alerts").as_deref(), Some("api.weather.gov"));
        assert_eq!(host_of("https://someone@elsewhere.example/"), None);
        assert_eq!(host_of("ftp://elsewhere.example/"), None);
        assert!(plain_path("/sys") && plain_path("/proc/stat"));
        assert!(!plain_path("sys") && !plain_path("/sys/../etc"));
        let u = Uses::of(&serde_json::json!([{ "read": "/sys" }, { "read": "/proc/stat" }, { "read": "/etc/../shadow" }, { "web": "API.open-meteo.com", "to": "Open-Meteo" }, "nvidia"]));
        assert_eq!(u.read, vec!["/sys", "/proc/stat"], "a path with a way up isn't taken");
        assert_eq!(u.web, vec![("api.open-meteo.com".to_string(), "Open-Meteo".to_string())]);
        assert!(u.nvidia);
        // (a file named is that file alone, not every name it starts)
        assert!(Path::new("/proc/stat").starts_with("/proc/stat") && !Path::new("/proc/statm").starts_with("/proc/stat"));
    }

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

    /// A keyboard that replays a recorded exchange, as evga-z10's parity
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

    /// The original editor's exchange less the reads it repeated: it read
    /// each stored macro twice in a row (once for its settings, again for its
    /// steps), and the Z10's code reads each once. Every report written stays.
    fn reads_once(log: &Log) {
        let mut l = log.lock().unwrap();
        let mut out = VecDeque::new();
        let mut seen = std::collections::HashSet::new();
        while let Some((kind, b)) = l.pop_front() {
            if kind == "set" && b[0] == 9 {
                if b[3] != 1 || b[4] == 0 {
                    seen.clear();
                } else if !seen.insert(b[4]) {
                    l.pop_front(); // and its answer
                    continue;
                }
            }
            out.push_back((kind, b));
        }
        *l = out;
    }

    fn core_dir() -> PathBuf {
        let dir = crate::cores::dir().join("evga-z10");
        assert!(dir.join("core.wasm").exists(), "the Z10's module isn't built: run tools/build-cores.sh");
        dir
    }

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    /// A layer of the keyboard's keys, four bytes each in order, by the keys'
    /// ids as shared actions (a value that isn't four bytes kept as it is, for
    /// the core to refuse).
    fn shared_layer(keys: &Value) -> Value {
        let keys = keys.as_array().cloned().unwrap_or_default();
        let m: serde_json::Map<String, Value> = keys
            .iter()
            .enumerate()
            .take(evga_z10::protocol::NKEYS)
            .map(|(pos, d)| {
                let bytes: Option<Vec<u8>> = d.as_array().filter(|a| a.len() == 4).and_then(|a| a.iter().map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok())).collect());
                let v = bytes.map_or_else(|| d.clone(), |b| evga_z10::shared::action_of([b[0], b[1], b[2], b[3]]).write());
                (evga_z10::shared::control(pos).unwrap().to_string(), v)
            })
            .collect();
        Value::Object(m)
    }

    /// A profile the original editor read or saved, in the shared shapes.
    fn shared_profile(p: &Value) -> Value {
        evga_z10::shared::profile_of_view(p)
    }

    /// What the original editor saved, as `profiles/set` takes it: only what it gave.
    fn shared_save(n: &Value, body: &Value) -> Value {
        let mut out = serde_json::json!({ "profile": n });
        if let Some(name) = body.get("name") {
            out["name"] = name.clone();
        }
        if let Some(l) = body.get("lighting") {
            out["lighting"] = evga_z10::shared::lighting_of(l);
        }
        let mut keys = serde_json::Map::new();
        for (field, layer) in [("keys", "base"), ("fnkeys", "fn")] {
            if let Some(k) = body.get(field).filter(|k| !k.is_null()) {
                keys.insert(layer.into(), shared_layer(k));
            }
        }
        if !keys.is_empty() {
            out["keys"] = Value::Object(keys);
        }
        out
    }

    /// Every macro as the original editor read them, in the shared shapes.
    fn shared_macros(m: &Value) -> Value {
        Value::Object(m.as_object().unwrap().iter().map(|(i, v)| (i.clone(), evga_z10::shared::macro_of(v))).collect())
    }

    /// The original editor's whole read of the keyboard, as `device/read` gives it.
    fn shared_state(s: &Value) -> Value {
        evga_z10::shared::read_of_state(s)
    }

    /// Every editor action in evga-z10's parity vectors (the original
    /// editor's exchanges with the keyboard), run through the Z10's module in
    /// the core protocol's shared shapes: the same reports sent byte for byte,
    /// and the same answers given, in those shapes.
    #[test]
    fn the_z10_module_sends_what_the_original_editor_sent() {
        let root = crate::cores::dir().join("../parity");
        let mut files = vec![root.join("vectors.json.gz")];
        if let Ok(dir) = std::fs::read_dir(root.join("private")) {
            files.extend(dir.flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with(".json.gz")));
        }
        let data = std::env::temp_dir().join(format!("ectomatter-core-test-{}", std::process::id()));
        let mut cases = 0;
        for f in files {
            let mut text = String::new();
            flate2::read::GzDecoder::new(std::fs::File::open(&f).unwrap()).read_to_string(&mut text).unwrap();
            let v: Value = serde_json::from_str(&text).unwrap();
            for c in v["device"]["editor"].as_array().unwrap() {
                let name = format!("{}: {}", f.file_name().unwrap().to_string_lossy(), c["name"].as_str().unwrap());
                let log: Log = Arc::new(Mutex::new(c["log"].as_array().unwrap().iter().map(|e| (e[0].as_str().unwrap().to_string(), unhex(e[1].as_str().unwrap()))).collect()));
                reads_once(&log);
                let wrong = Arc::new(Mutex::new(None));
                let hid = Replay { log: log.clone(), wrong: wrong.clone() };
                let core = Core::start(&core_dir(), data.clone(), Box::new(One(Some(Box::new(hid))))).unwrap();
                core.call("initialize", &serde_json::json!({ "backups": false })).unwrap();
                let kind = c["name"].as_str().unwrap().split(' ').next().unwrap();
                let (got, want) = match kind {
                    "state" => (core.call("device/read", &Value::Null), shared_state(&c["result"])),
                    "save" => (core.call("profiles/set", &shared_save(&c["n"], &c["body"])), shared_profile(&c["result"])),
                    "write_macro" if c["body"].get("delete").is_some() => (core.call("macros/delete", &serde_json::json!({ "macro": c["index"] })), shared_macros(&c["result"])),
                    "write_macro" => {
                        let mut body = c["body"].clone();
                        if let Some(steps) = body["steps"].as_array() {
                            body["steps"] = Value::Array(evga_z10::shared::steps_of(steps));
                        }
                        body["macro"] = c["index"].clone();
                        (core.call("macros/set", &body), shared_macros(&c["result"]))
                    }
                    "activate" => (core.call("profiles/activate", &serde_json::json!({ "profile": c["n"] })), c["result"].clone()),
                    other => panic!("no replay for {other}"),
                };
                if c["error"] == true {
                    assert!(got.is_err(), "{name}: should fail");
                } else {
                    assert_eq!(*wrong.lock().unwrap(), None, "{name}");
                    assert_eq!(got.unwrap_or_else(|e| panic!("{name}: {e}")), want, "{name}: result");
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

    /// A fake Sensei 310: it keeps every report sent to it, with when, and
    /// answers the firmware and settings questions as Grant's mouse did
    /// (firmware 24 01, presets 800 and 1600 with the first in use, both
    /// lights #FF1000).
    struct FakeMouse {
        sent: Arc<Mutex<Vec<(Instant, bool, Vec<u8>)>>>,
        waiting: VecDeque<Vec<u8>>,
    }

    const MOUSE_SETTINGS: [u8; 12] = [0x01, 0x01, 0x07, 0x00, 0x0f, 0x00, 0xff, 0x10, 0x00, 0xff, 0x10, 0x00];

    impl Hid for FakeMouse {
        fn send_feature(&mut self, report: &[u8]) -> Result<(), String> {
            self.sent.lock().unwrap().push((Instant::now(), true, report.to_vec()));
            Ok(())
        }
        fn get_feature(&mut self, _: u8, _: usize) -> Result<Vec<u8>, String> {
            Err("the mouse is asked by output reports".into())
        }
        fn write(&mut self, report: &[u8]) -> Result<usize, String> {
            self.sent.lock().unwrap().push((Instant::now(), false, report.to_vec()));
            let mut answer = vec![0u8; 64];
            match report.get(1) {
                Some(0x90) => answer[..2].copy_from_slice(&[0x24, 0x01]),
                Some(0x92) => answer[..12].copy_from_slice(&MOUSE_SETTINGS),
                _ => return Ok(report.len()),
            }
            self.waiting.push_back(answer);
            Ok(report.len())
        }
        fn read(&mut self, _: u32) -> Result<Vec<u8>, String> {
            Ok(self.waiting.pop_front().unwrap_or_default())
        }
    }

    /// SteelSeries' core, started on a fake Sensei 310 with a folder of its own.
    fn mouse_core(name: &str) -> (Core, Arc<Mutex<Vec<(Instant, bool, Vec<u8>)>>>, PathBuf) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let fake = FakeMouse { sent: sent.clone(), waiting: VecDeque::new() };
        let data = std::env::temp_dir().join(format!("ectomatter-mouse-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let dir = crate::cores::dir().join(crate::mouse::CORE);
        assert!(dir.join("core.wasm").exists(), "the SteelSeries module isn't built: run tools/build-cores.sh");
        let core = Core::start(&dir, data.clone(), Box::new(One(Some(Box::new(fake))))).unwrap();
        core.call("initialize", &serde_json::json!({ "protocol": 3 })).unwrap();
        (core, sent, data)
    }

    /// The fake mouse's key reports: what it sends as a keyboard, each read once.
    struct FakeKeys(Arc<Mutex<VecDeque<Vec<u8>>>>);

    impl Hid for FakeKeys {
        fn send_feature(&mut self, _: &[u8]) -> Result<(), String> {
            Err("nothing is sent to the mouse's key reports".into())
        }
        fn get_feature(&mut self, _: u8, _: usize) -> Result<Vec<u8>, String> {
            Err("nothing is asked of the mouse's key reports".into())
        }
        fn write(&mut self, _: &[u8]) -> Result<usize, String> {
            Err("nothing is sent to the mouse's key reports".into())
        }
        fn read(&mut self, _: u32) -> Result<Vec<u8>, String> {
            Ok(self.0.lock().unwrap().pop_front().unwrap_or_default())
        }
    }

    /// A computer with the fake mouse's command channel and its key reports.
    struct MouseAndKeys(Option<Box<dyn Hid>>, Option<Box<dyn Hid>>);

    impl Usb for MouseAndKeys {
        fn devices(&mut self) -> Vec<DeviceInfo> {
            vec![
                DeviceInfo { vendor: 0x1038, product: 0x1722, interface: 0, usage_page: 0xFFC0, usage: 1, path: "mouse".into() },
                DeviceInfo { vendor: 0x1038, product: 0x1722, interface: 2, usage_page: 0x0001, usage: 0x0006, path: "mouse-keys".into() },
            ]
        }
        fn open(&mut self, d: &DeviceInfo) -> Result<Box<dyn Hid>, String> {
            let which = if d.path == "mouse" { &mut self.0 } else { &mut self.1 };
            which.take().ok_or_else(|| "opened twice".to_string())
        }
    }

    /// A button given one of Ectomatter's host slots sends that slot's reserved
    /// code, and the core hears it in the mouse's key reports: pressed, then
    /// let go, each told at once (waking whoever waits for it), by the
    /// button's id.
    #[test]
    fn a_host_slot_button_is_heard_pressed_and_let_go() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let reports = Arc::new(Mutex::new(VecDeque::new()));
        let usb = MouseAndKeys(Some(Box::new(FakeMouse { sent: sent.clone(), waiting: VecDeque::new() })), Some(Box::new(FakeKeys(reports.clone()))));
        let data = std::env::temp_dir().join(format!("ectomatter-mouse-host-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let core = Core::start(&crate::cores::dir().join(crate::mouse::CORE), data.clone(), Box::new(usb)).unwrap();
        core.call("initialize", &serde_json::json!({ "protocol": 3 })).unwrap();
        let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let w = woken.clone();
        core.on_told(move || {
            w.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        });
        core.call("keys/set", &serde_json::json!({ "device": "sensei310-1", "layer": "base", "control": "right-side-front", "action": { "host": 6 } })).unwrap();
        let buttons = sent.lock().unwrap().iter().rev().find(|(_, _, b)| b[..2] == [0x00, 0x31]).map(|(_, _, b)| b.clone()).expect("the buttons sent");
        assert_eq!(buttons[3 + 6 * 5..3 + 6 * 5 + 2], [0x51, 0xAB], "button 7 sends the slot's reserved code");
        let _ = core.told();
        let heard = |want: &str| {
            for _ in 0..50 {
                std::thread::sleep(Duration::from_millis(10));
                let _ = core.call("settings/get", &serde_json::json!({ "device": "sensei310-1" }));
                if let Some((_, p)) = core.told().into_iter().find(|(m, _)| m == want) {
                    return p;
                }
            }
            panic!("never heard {want}");
        };
        reports.lock().unwrap().push_back(vec![1, 0, 0, 0xAB, 0, 0, 0, 0, 0]);
        let down = heard("input/down");
        assert_eq!((down["control"].as_str(), down["host"].as_u64()), (Some("right-side-front"), Some(6)));
        reports.lock().unwrap().push_back(vec![1, 0, 0, 0, 0, 0, 0, 0, 0]);
        let up = heard("input/up");
        assert_eq!(up["control"], "right-side-front");
        assert!(woken.load(std::sync::atomic::Ordering::Relaxed) >= 2, "each told woke the page");
        // a key the button sends for the computer is no slot's
        reports.lock().unwrap().push_back(vec![1, 0, 0, 0x04, 0, 0, 0, 0, 0]);
        std::thread::sleep(Duration::from_millis(60));
        let _ = core.call("settings/get", &serde_json::json!({ "device": "sensei310-1" }));
        assert!(core.told().iter().all(|(m, _)| !m.starts_with("input/")));
        let _ = std::fs::remove_dir_all(&data);
    }

    /// A computer with the whole fake mouse as hidapi lists it: its command
    /// channel, its pointer's button reports and its key reports, the last
    /// two each giving the reports pushed to it, opened as often as asked.
    struct MouseWhole(Option<Box<dyn Hid>>, Arc<Mutex<VecDeque<Vec<u8>>>>, Arc<Mutex<VecDeque<Vec<u8>>>>);

    impl Usb for MouseWhole {
        fn devices(&mut self) -> Vec<DeviceInfo> {
            vec![
                DeviceInfo { vendor: 0x1038, product: 0x1722, interface: 0, usage_page: 0xFFC0, usage: 1, path: "mouse".into() },
                DeviceInfo { vendor: 0x1038, product: 0x1722, interface: 1, usage_page: 0x0001, usage: 0x0002, path: "mouse-pointer".into() },
                DeviceInfo { vendor: 0x1038, product: 0x1722, interface: 2, usage_page: 0x0001, usage: 0x0006, path: "mouse-keys".into() },
            ]
        }
        fn open(&mut self, d: &DeviceInfo) -> Result<Box<dyn Hid>, String> {
            match d.interface {
                0 => self.0.take().ok_or_else(|| "opened twice".to_string()),
                1 => Ok(Box::new(FakeKeys(self.1.clone()))),
                _ => Ok(Box::new(FakeKeys(self.2.clone()))),
            }
        }
    }

    /// Asked to watch every press, the mouse's core hears every button: a
    /// click in the pointer's reports, by the button whose job sends it, and
    /// a key a side button sends, in the key reports, each pressed and let go,
    /// told by the button's id and what it sent. Moving tells nothing, nor
    /// does a click once it's no longer watched, and nothing is sent to the
    /// mouse for any of it.
    #[test]
    fn the_mouse_core_hears_every_button_while_watched() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let (pointer, keys) = (Arc::new(Mutex::new(VecDeque::new())), Arc::new(Mutex::new(VecDeque::new())));
        let usb = MouseWhole(Some(Box::new(FakeMouse { sent: sent.clone(), waiting: VecDeque::new() })), pointer.clone(), keys.clone());
        let data = std::env::temp_dir().join(format!("ectomatter-mouse-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let core = Core::start(&crate::cores::dir().join(crate::mouse::CORE), data.clone(), Box::new(usb)).unwrap();
        core.call("initialize", &serde_json::json!({ "protocol": 3 })).unwrap();
        let before = sent.lock().unwrap().len();
        let heard = |n: usize| {
            let mut got: Vec<(String, Value)> = Vec::new();
            for _ in 0..100 {
                std::thread::sleep(Duration::from_millis(10));
                got.extend(core.told().into_iter().filter(|(m, _)| m.starts_with("input/")));
                if got.len() >= n {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(60));
            got.extend(core.told().into_iter().filter(|(m, _)| m.starts_with("input/")));
            got.into_iter().map(|(m, p)| (m, p["control"].as_str().unwrap_or_default().to_string(), p["sends"].clone())).collect::<Vec<_>>()
        };
        let ev = |m: &str, c: &str, v: Value| (m.to_string(), c.to_string(), v);
        let push = |q: &Arc<Mutex<VecDeque<Vec<u8>>>>, r: Vec<u8>| q.lock().unwrap().push_back(r);
        assert_eq!(core.call("input/watch", &serde_json::json!({ "device": "sensei310-1", "all": true })).unwrap()["listening"], true);
        // the left button, then the left side's back one with it held (the
        // mouse's buttons 1 and 4, from the factory), then both let go
        push(&pointer, vec![0x01, 0, 0, 0, 0, 0, 0, 0, 0]);
        push(&pointer, vec![0x09, 0, 0, 0, 0, 0, 0, 0, 0]);
        push(&pointer, vec![0x00, 0, 0, 0, 0, 0, 0, 0, 0]);
        let mouse = |n: u8| serde_json::json!({ "mouse": n });
        assert_eq!(heard(4), [ev("input/down", "left", mouse(1)), ev("input/down", "left-side-back", mouse(4)), ev("input/up", "left", mouse(1)), ev("input/up", "left-side-back", mouse(4))]);
        // the right side's back button, which sends Page Down from the factory
        push(&keys, vec![1, 0, 0, 0x4E, 0, 0, 0, 0, 0]);
        push(&keys, vec![1, 0, 0, 0, 0, 0, 0, 0, 0]);
        let page_down = serde_json::json!({ "key": "PageDown" });
        assert_eq!(heard(2), [ev("input/down", "right-side-back", page_down.clone()), ev("input/up", "right-side-back", page_down)]);
        // the pointer moved, no button: nothing
        push(&pointer, vec![0x00, 7, 0, 3, 0, 0, 0, 0, 0]);
        assert!(heard(1).is_empty());
        // no longer watched: a click is the computer's alone
        core.call("input/watch", &serde_json::json!({ "device": "sensei310-1", "all": false })).unwrap();
        push(&pointer, vec![0x02, 0, 0, 0, 0, 0, 0, 0, 0]);
        push(&keys, vec![1, 0, 0, 0x4E, 0, 0, 0, 0, 0]);
        assert!(heard(1).is_empty());
        assert_eq!(sent.lock().unwrap().len(), before, "nothing sent to the mouse to listen to it");
        let _ = std::fs::remove_dir_all(&data);
    }

    /// One of the fake Z10's interfaces as its key reports are read: each
    /// report once, and an error once the keyboard is unplugged.
    struct Ear(Arc<Mutex<VecDeque<Vec<u8>>>>, Arc<std::sync::atomic::AtomicBool>);

    impl Hid for Ear {
        fn send_feature(&mut self, _: &[u8]) -> Result<(), String> {
            Err("nothing is sent where the keys are read".into())
        }
        fn get_feature(&mut self, _: u8, _: usize) -> Result<Vec<u8>, String> {
            Err("nothing is asked where the keys are read".into())
        }
        fn write(&mut self, _: &[u8]) -> Result<usize, String> {
            Err("nothing is sent where the keys are read".into())
        }
        fn read(&mut self, _: u32) -> Result<Vec<u8>, String> {
            if self.1.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("unplugged".into());
            }
            Ok(self.0.lock().unwrap().pop_front().unwrap_or_default())
        }
    }

    /// A computer with a fake Z10 on all three of its interfaces, as hidapi
    /// lists them: one entry for each collection, an interface's collections
    /// sharing its path. The editor's collection keeps every report sent to
    /// it; each interface opened anew gives its key reports.
    struct Z10Whole {
        editor: Option<Box<dyn Hid>>,
        reports: [Arc<Mutex<VecDeque<Vec<u8>>>>; 3],
        gone: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Usb for Z10Whole {
        fn devices(&mut self) -> Vec<DeviceInfo> {
            if self.gone.load(std::sync::atomic::Ordering::Relaxed) {
                return Vec::new();
            }
            let d = |interface: i32, usage_page: u16, usage: u16| DeviceInfo { vendor: 0x3842, product: 0x2611, interface, usage_page, usage, path: format!("kb{interface}") };
            vec![d(0, 0x01, 0x06), d(1, 0x01, 0x02), d(1, 0x0C, 0x01), d(1, 0x08, 0x4B), d(2, 0x01, 0x06), d(2, 0x08, 0x4B)]
        }
        fn open(&mut self, d: &DeviceInfo) -> Result<Box<dyn Hid>, String> {
            if (d.interface, d.usage_page) == (1, 0x08) {
                return self.editor.take().ok_or_else(|| "the editor's collection opened twice".to_string());
            }
            if (d.interface, d.usage_page) == (2, 0x08) {
                return Err("the ten-key interface's indicators were opened for commands".into());
            }
            let i = d.interface as usize;
            Ok(Box::new(Ear(self.reports[i].clone(), self.gone.clone())))
        }
    }

    /// Asked to watch, the Z10's core hears the keyboard's own key reports: a
    /// key on a host slot, pressed and let go, by its slot; every other key,
    /// a modifier and a media key only while every press is watched, each by
    /// what it sent; and, unplugged, everything held let go. Nothing is ever
    /// sent to the keyboard for it.
    #[test]
    fn the_z10_core_hears_its_keys() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let editor = Sent(sent.clone(), Clock(Arc::new(Mutex::new(Vec::new())), Vec::new()), Arc::new(Mutex::new(Vec::new())));
        let reports: [Arc<Mutex<VecDeque<Vec<u8>>>>; 3] = Default::default();
        let gone = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let usb = Z10Whole { editor: Some(Box::new(editor)), reports: reports.clone(), gone: gone.clone() };
        let data = std::env::temp_dir().join(format!("ectomatter-core-keys-{}", std::process::id()));
        let core = Core::start(&core_dir(), data.clone(), Box::new(usb)).unwrap();
        core.call("initialize", &serde_json::json!({ "backups": false })).unwrap();
        assert_eq!(core.call("input/watch", &serde_json::json!({ "all": false })).unwrap()["listening"], true);
        let heard = |n: usize| {
            let mut got: Vec<(String, Value)> = Vec::new();
            for _ in 0..100 {
                std::thread::sleep(Duration::from_millis(10));
                got.extend(core.told().into_iter().filter(|(m, _)| m.starts_with("input/")));
                if got.len() >= n {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(60));
            got.extend(core.told().into_iter().filter(|(m, _)| m.starts_with("input/")));
            got.into_iter().map(|(m, p)| (m, p["sends"].clone())).collect::<Vec<_>>()
        };
        let push = |i: usize, r: Vec<u8>| reports[i].lock().unwrap().push_back(r);
        let ten = |keys: &[u8]| {
            let mut r = vec![0x10, 0, 0];
            r.extend(keys);
            r.resize(13, 0);
            r
        };
        let ev = |m: &str, v: Value| (m.to_string(), v);
        // a key on host slot 2 (the code 0xA7), with a letter beside it, then
        // both let go: the slot alone is told, the letter unwatched
        push(2, ten(&[0x04, 0xA7]));
        push(2, ten(&[0x04]));
        push(2, ten(&[]));
        assert_eq!(heard(2), [ev("input/down", serde_json::json!({ "host": 2 })), ev("input/up", serde_json::json!({ "host": 2 }))]);
        // every press, watched: Left Shift and A on the boot report, then a
        // media key; each in the order it came, and let go the same way
        core.call("input/watch", &serde_json::json!({ "all": true })).unwrap();
        push(0, vec![0x02, 0, 0x04, 0, 0, 0, 0, 0]);
        push(1, vec![0x02, 0xE9, 0x00]);
        assert_eq!(heard(3), [ev("input/down", serde_json::json!({ "key": "LeftShift" })), ev("input/down", serde_json::json!({ "key": "A" })), ev("input/down", serde_json::json!({ "media": "VolumeUp" }))]);
        push(0, vec![0; 8]);
        push(1, vec![0x02, 0, 0]);
        assert_eq!(heard(3), [ev("input/up", serde_json::json!({ "key": "LeftShift" })), ev("input/up", serde_json::json!({ "key": "A" })), ev("input/up", serde_json::json!({ "media": "VolumeUp" }))]);
        // a key pressed and let go between two listens is told both ways
        push(0, vec![0, 0, 0x05, 0, 0, 0, 0, 0]);
        push(0, vec![0; 8]);
        assert_eq!(heard(2), [ev("input/down", serde_json::json!({ "key": "B" })), ev("input/up", serde_json::json!({ "key": "B" }))]);
        // unplugged with a key held: it is let go
        push(2, ten(&[0x06]));
        assert_eq!(heard(1), [ev("input/down", serde_json::json!({ "key": "C" }))]);
        gone.store(true, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(heard(1), [ev("input/up", serde_json::json!({ "key": "C" }))]);
        assert!(sent.lock().unwrap().is_empty(), "nothing sent to the keyboard");
        let _ = std::fs::remove_dir_all(&data);
    }

    /// The display's rows go to the keyboard as the screen player sent them
    /// itself (report 10, byte for byte), and handing the screen back sends
    /// the player's own last frame (its fourth byte 0).
    #[test]
    fn the_z10_core_shows_what_the_player_sent() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let editor = Sent(sent.clone(), Clock(Arc::new(Mutex::new(Vec::new())), Vec::new()), Arc::new(Mutex::new(Vec::new())));
        let data = std::env::temp_dir().join(format!("ectomatter-core-lcd-{}", std::process::id()));
        let core = Core::start(&core_dir(), data.clone(), Box::new(One(Some(Box::new(editor))))).unwrap();
        core.call("initialize", &serde_json::json!({ "backups": false })).unwrap();
        let rows: Vec<String> = ["CPU  40\u{b0}C   5%", "GPU  38C   2%", "\u{2588}\u{2588}\u{2588} RAM 8.1/31 GB", "VRAM 2.0/31 GB      x"].map(String::from).to_vec();
        core.call("displays/show", &serde_json::json!({ "rows": rows })).unwrap();
        core.call("displays/hand-back", &Value::Null).unwrap();
        let _ = std::fs::remove_dir_all(&data);
        let s = sent.lock().unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0], crate::lcd_player::frame(&rows, 1).to_vec(), "the rows as the player sent them");
        assert_eq!(s[1], crate::lcd_player::frame(&[], 0).to_vec(), "handed back as the player did");
    }

    /// A key with modifiers held goes on a button as libratbag puts it there
    /// (its steelseries driver: 0x51, the modifiers' usages in order, then
    /// the key), so Grant's own desktop bindings, set by game-helper through
    /// ratbagctl, are the bytes Ectomatter sends for them too.
    #[test]
    fn a_shortcut_goes_on_a_button_as_libratbag_writes_it() {
        let (core, sent, data) = mouse_core("chord");
        let d = "sensei310-1";
        for (control, action) in [
            ("left-side-back", serde_json::json!({ "shortcut": ["LeftCtrl", "LeftShift", "Tab"] })),
            ("left-side-front", serde_json::json!({ "shortcut": ["LeftCtrl", "T"] })),
            ("right-side-back", serde_json::json!({ "key": "F5" })),
            ("right-side-front", serde_json::json!({ "shortcut": ["LeftCtrl", "W"] })),
            ("cpi", serde_json::json!({ "shortcut": ["LeftMeta", "W"] })),
        ] {
            core.call("keys/set", &serde_json::json!({ "device": d, "layer": "base", "control": control, "action": action })).unwrap();
        }
        let buttons = sent.lock().unwrap().iter().rev().find(|(_, _, b)| b[..2] == [0x00, 0x31]).map(|(_, _, b)| b.clone()).expect("the buttons sent");
        let want: [[u8; 5]; 8] = [
            [1, 0, 0, 0, 0],
            [2, 0, 0, 0, 0],
            [3, 0, 0, 0, 0],
            [0x51, 0xE0, 0xE1, 0x2B, 0],
            [0x51, 0xE0, 0x17, 0, 0],
            [0x51, 0x3E, 0, 0, 0],
            [0x51, 0xE0, 0x1A, 0, 0],
            [0x51, 0xE3, 0x1A, 0, 0],
        ];
        assert_eq!(buttons[3..3 + 40], want.concat(), "each button as libratbag writes it");
        // more than three modifiers, or two keys, the mouse can't hold
        assert!(core.call("keys/set", &serde_json::json!({ "device": d, "layer": "base", "control": "cpi", "action": { "shortcut": ["LeftCtrl", "LeftShift", "LeftAlt", "LeftMeta", "A"] } })).is_err());
        assert!(core.call("keys/set", &serde_json::json!({ "device": d, "layer": "base", "control": "cpi", "action": { "shortcut": ["A", "B"] } })).is_err());
        let _ = std::fs::remove_dir_all(&data);
    }

    /// The mouse as its page's thread hears it, from the core on the fake
    /// mouse, saved where MOUSE_STATE_OUT says (for snapshots of the page).
    #[test]
    fn the_mouse_as_its_page_hears_it() {
        let Ok(out) = std::env::var("MOUSE_STATE_OUT") else { return };
        let (core, _, data) = mouse_core("state");
        let device = core.told().into_iter().find(|(m, _)| m == "device/added").map(|(_, p)| p).unwrap();
        let d = serde_json::json!({ "device": "sensei310-1" });
        let state = serde_json::json!({
            "present": true,
            "device": device,
            "keys": core.call("keys/get", &d).unwrap(),
            "lighting": core.call("lighting/get", &d).unwrap(),
            "settings": core.call("settings/get", &d).unwrap(),
        });
        std::fs::write(out, serde_json::to_string_pretty(&state).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&data);
    }

    /// On the real mouse, when MOUSE_LIVE is set: the first preset set to 900
    /// CPI, read back by the settings read, then set back to what it was, which
    /// the mouse then keeps. (Leaves the mouse as it found it.)
    #[test]
    fn the_real_mouse_takes_a_change() {
        if std::env::var("MOUSE_LIVE").is_err() {
            return;
        }
        let data = std::env::temp_dir().join(format!("ectomatter-mouse-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let core = Core::start(&crate::cores::dir().join(crate::mouse::CORE), data.clone(), Box::new(Hidapi::new())).unwrap();
        core.call("initialize", &serde_json::json!({ "protocol": 3 })).unwrap();
        let d = serde_json::json!({ "device": "sensei310-1" });
        let was = core.call("settings/get", &d).unwrap()["values"]["cpi1"].as_u64().unwrap();
        eprintln!("the first preset was {was} CPI");
        let read = || -> u64 {
            // the mouse's own answer, through a core met anew (its record kept apart)
            let d2 = std::env::temp_dir().join(format!("ectomatter-mouse-read-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d2);
            let c = Core::start(&crate::cores::dir().join(crate::mouse::CORE), d2.clone(), Box::new(Hidapi::new())).unwrap();
            c.call("initialize", &serde_json::json!({ "protocol": 3 })).unwrap();
            let v = c.call("settings/get", &serde_json::json!({ "device": "sensei310-1" })).unwrap()["values"]["cpi1"].as_u64().unwrap();
            let _ = std::fs::remove_dir_all(&d2);
            v
        };
        core.call("settings/set", &serde_json::json!({ "device": "sensei310-1", "id": "cpi1", "value": 900 })).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let now = read();
        eprintln!("the mouse says {now} CPI after the change");
        core.call("settings/set", &serde_json::json!({ "device": "sensei310-1", "id": "cpi1", "value": was })).unwrap();
        std::thread::sleep(Duration::from_millis(1800));
        let _ = core.call("settings/get", &d);
        let back = read();
        eprintln!("and {back} CPI once set back");
        let _ = std::fs::remove_dir_all(&data);
        assert_eq!(now, 900);
        assert_eq!(back, was);
    }

    /// Meeting the mouse asks it two questions and writes nothing: the core
    /// knows its presets and lights' colors from its answer, and announces it.
    #[test]
    fn meeting_the_mouse_writes_nothing() {
        let (core, sent, data) = mouse_core("meet");
        let told = core.told();
        let added = told.iter().find(|(m, _)| m == "device/added").map(|(_, p)| p.clone()).expect("device/added");
        assert_eq!(added["firmware"]["version"], "36.1");
        assert_eq!(added["controls"].as_array().unwrap().len(), 8);
        // (what the frontend draws the mouse's settings from)
        assert_eq!(added["kind"], "mouse");
        assert_eq!(added["layout"]["device"][0]["show"], serde_json::json!(["cpi1", "cpi2"]));
        assert_eq!(added["capabilities"]["lighting"]["effects"][3]["start"]["colors"], serde_json::json!(["#FF0000", "#00FF00", "#0000FF"]));
        if let Ok(out) = std::env::var("ECTOMATTER_DESCRIPTION_OUT") {
            std::fs::write(out, serde_json::to_vec_pretty(&added).unwrap()).unwrap();
        }
        let s = core.call("settings/get", &serde_json::json!({ "device": "sensei310-1" })).unwrap();
        assert_eq!((s["values"]["cpi1"].as_u64(), s["values"]["cpi2"].as_u64()), (Some(800), Some(1600)));
        assert_eq!((s["known"]["cpi1"].as_bool(), s["known"]["polling"].as_bool()), (Some(true), Some(false)));
        let l = core.call("lighting/get", &serde_json::json!({ "device": "sensei310-1" })).unwrap();
        assert_eq!(l["logo"]["params"]["color"], "#FF1000");
        core.call("shutdown", &Value::Null).unwrap();
        let sent: Vec<Vec<u8>> = sent.lock().unwrap().iter().map(|(_, _, b)| b.clone()).collect();
        let _ = std::fs::remove_dir_all(&data);
        assert_eq!(sent.len(), 2, "only the two questions: {sent:02x?}");
        assert_eq!(sent[0], [0x00, 0x90, 0x00], "the firmware's version");
        assert_eq!(sent[1][..2], [0x00, 0x92], "the settings read");
    }

    /// The mouse says its lights' colors and nothing more of them: a light's
    /// effect is known once Ectomatter sets it, each light by itself. Met again
    /// showing another color than Ectomatter set (something else changed it),
    /// it is the color the mouse says, and only that is known again.
    #[test]
    fn a_light_is_known_once_ectomatter_sets_it() {
        let (core, _sent, data) = mouse_core("known");
        let d = serde_json::json!({ "device": "sensei310-1" });
        assert_eq!(core.call("lighting/get", &d).unwrap()["known"], serde_json::json!({ "logo": "colors", "wheel": "colors" }));
        core.call("lighting/set", &serde_json::json!({ "device": "sensei310-1", "light": "logo", "effect": "static", "params": { "color": "#00FF00" } })).unwrap();
        assert_eq!(core.call("lighting/get", &d).unwrap()["known"], serde_json::json!({ "logo": true, "wheel": "colors" }));
        core.call("shutdown", &Value::Null).unwrap();
        drop(core);
        // met again, the mouse saying #FF1000, not the green Ectomatter set
        let fake = FakeMouse { sent: Arc::new(Mutex::new(Vec::new())), waiting: VecDeque::new() };
        let core = Core::start(&crate::cores::dir().join(crate::mouse::CORE), data.clone(), Box::new(One(Some(Box::new(fake))))).unwrap();
        core.call("initialize", &serde_json::json!({ "protocol": 3 })).unwrap();
        let l = core.call("lighting/get", &d).unwrap();
        core.call("shutdown", &Value::Null).unwrap();
        let _ = std::fs::remove_dir_all(&data);
        assert_eq!((l["logo"]["params"]["color"].as_str(), l["known"]["logo"].as_str()), (Some("#FF1000"), Some("colors")));
        assert_eq!(l["known"]["wheel"], "colors");
    }

    /// A change goes to the mouse at once, as rivalcfg sends it, 50 ms after
    /// the command before at the soonest; once changes stop, it is kept in
    /// the mouse's memory, once.
    #[test]
    fn a_change_is_sent_then_kept_once() {
        let (core, sent, data) = mouse_core("change");
        let at = sent.lock().unwrap().len();
        let d = serde_json::json!({ "device": "sensei310-1" });
        core.call("settings/set", &serde_json::json!({ "device": "sensei310-1", "id": "cpi1", "value": 1200 })).unwrap();
        core.call("settings/set", &serde_json::json!({ "device": "sensei310-1", "id": "polling", "value": 500 })).unwrap();
        core.call("lighting/set", &serde_json::json!({ "device": "sensei310-1", "light": "logo", "effect": "static", "params": { "color": "#FF0000" } })).unwrap();
        core.call("keys/set", &serde_json::json!({ "device": "sensei310-1", "layer": "base", "control": "right-side-front", "action": { "key": "F13" } })).unwrap();
        let soon = sent.lock().unwrap().len();
        std::thread::sleep(Duration::from_millis(1800));
        // (a call, so the core's thread has run its timer)
        let _ = core.call("settings/get", &d);
        let all = sent.lock().unwrap()[at..].to_vec();
        let _ = std::fs::remove_dir_all(&data);
        let b: Vec<&Vec<u8>> = all.iter().map(|(_, _, b)| b).collect();
        assert_eq!(soon - at, 4, "each change sent at once, nothing kept yet: {b:02x?}");
        assert_eq!(*b[0], [0x00, 0x53, 0x00, 0x01, 0x0b, 0x00, 0x42], "preset 1 at 1200 CPI");
        assert_eq!(*b[1], [0x00, 0x54, 0x00, 0x02], "500 Hz");
        // the logo held red: a 26-byte header (light 0, a second's cycle, held, one stop), the color, the stop
        let mut light = vec![0x00, 0x5b, 0x00, 0x00, 0xe8, 0x03];
        light.extend([0u8; 14]);
        light.push(0x01);
        light.extend([0u8; 7]);
        light.push(0x01);
        light.extend([0xff, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00]);
        assert_eq!(*b[2], light, "the logo held red");
        assert!(all[2].1, "a light's effect is a feature report");
        assert_eq!(b[3][..3], [0x00, 0x31, 0x00], "the buttons");
        assert_eq!(b[3].len(), 3 + 8 * 5);
        assert_eq!(b[3][3 + 6 * 5..3 + 6 * 5 + 2], [0x51, 0x68], "button 7 sends F13");
        assert_eq!(b[3][3 + 5 * 5..3 + 5 * 5 + 2], [0x51, 0x4e], "button 6 still Page Down");
        assert_eq!(b[3][3 + 7 * 5], 0x30, "button 8 still switches sensitivity");
        assert_eq!(b.iter().filter(|r| r[..2] == [0x00, 0x59]).count(), 1, "kept once");
        assert_eq!(*b[4], [0x00, 0x59, 0x00], "kept after the changes");
        for w in all.windows(2) {
            assert!(w[1].0 - w[0].0 >= Duration::from_millis(49), "commands 50 ms apart");
        }
    }

    /// A whole profile goes to the mouse in order (its buttons in one report,
    /// each light, both sensitivities, the polling rate) and is kept in its
    /// memory once; GG's own Color shift plays red, green and blue a second
    /// around, evenly, back to red; and what the mouse ships as is GG's.
    #[test]
    fn a_profile_goes_to_the_mouse_whole_and_is_kept_once() {
        let (core, sent, data) = mouse_core("apply");
        let at = sent.lock().unwrap().len();
        let d = serde_json::json!({ "device": "sensei310-1" });
        let mut p = core.call("profiles/factory", &d).unwrap();
        assert_eq!(p["settings"], serde_json::json!({ "cpi1": 800, "cpi2": 1600, "polling": 1000 }));
        assert_eq!(p["keys"]["base"]["right-side-back"], serde_json::json!({ "key": "PageDown" }));
        assert_eq!(p["lighting"]["logo"]["params"]["colors"], serde_json::json!(["#FF0000", "#00FF00", "#0000FF"]));
        p["device"] = serde_json::json!("sensei310-1");
        p["settings"]["cpi1"] = serde_json::json!(1200);
        let answer = core.call("profiles/apply", &p).unwrap();
        let plays = &answer["plays"]["logo"];
        assert_eq!(plays["ms"], 1000);
        let stops: Vec<(u64, String)> = plays["stops"].as_array().unwrap().iter().map(|s| (s["at"].as_u64().unwrap(), s["color"].as_str().unwrap().to_string())).collect();
        assert_eq!(stops, [(0, "#FF0000".into()), (85, "#00FF00".into()), (170, "#0000FF".into()), (255, "#FF0000".into())]);
        let soon = sent.lock().unwrap().len();
        std::thread::sleep(Duration::from_millis(1800));
        let _ = core.call("settings/get", &d);
        let all: Vec<Vec<u8>> = sent.lock().unwrap()[at..].iter().map(|(_, _, b)| b.clone()).collect();
        let s = core.call("settings/get", &d).unwrap();
        let _ = std::fs::remove_dir_all(&data);
        assert_eq!(soon - at, 6, "buttons, two lights, two presets, polling: {all:02x?}");
        assert_eq!(all[0][..2], [0x00, 0x31], "the buttons first, in one report");
        assert_eq!(all[1][..4], [0x00, 0x5b, 0x00, 0x00], "then the logo");
        assert_eq!(all[2][..4], [0x00, 0x5b, 0x00, 0x01], "then the wheel");
        assert_eq!(all[3], [0x00, 0x53, 0x00, 0x01, 0x0b, 0x00, 0x42], "the first preset at 1200 CPI");
        assert_eq!(all[4][..4], [0x00, 0x53, 0x00, 0x02], "the second preset");
        assert_eq!(all[5], [0x00, 0x54, 0x00, 0x01], "1000 Hz");
        assert_eq!(all.iter().filter(|r| r[..2] == [0x00, 0x59]).count(), 1, "kept once");
        assert_eq!(s["values"]["cpi1"], 1200);
    }

    /// Colors streamed to the mouse's lights are shown and never kept in its
    /// memory: a change made meanwhile is kept only once the lights are
    /// handed back, which puts them as the mouse keeps them first.
    #[test]
    fn streamed_colors_are_never_kept() {
        let (core, sent, data) = mouse_core("direct");
        let d = serde_json::json!({ "device": "sensei310-1" });
        let at = sent.lock().unwrap().len();
        core.call("lighting/direct", &serde_json::json!({ "device": "sensei310-1", "colors": { "logo": "#00FF00", "wheel": "#0000FF" } })).unwrap();
        core.call("settings/set", &serde_json::json!({ "device": "sensei310-1", "id": "cpi1", "value": 1200 })).unwrap();
        std::thread::sleep(Duration::from_millis(1800));
        let _ = core.call("settings/get", &d);
        let during: Vec<Vec<u8>> = sent.lock().unwrap()[at..].iter().map(|(_, _, b)| b.clone()).collect();
        assert_eq!(during.iter().filter(|r| r[..2] == [0x00, 0x5b]).count(), 2, "both lights streamed: {during:02x?}");
        assert!(during.iter().all(|r| r[..2] != [0x00, 0x59]), "nothing kept while streaming");
        let back = core.call("lighting/hand-back", &d).unwrap();
        assert!(back["plays"]["logo"].is_object(), "what the lights play again");
        std::thread::sleep(Duration::from_millis(1800));
        let _ = core.call("settings/get", &d);
        let after: Vec<Vec<u8>> = sent.lock().unwrap()[at + during.len()..].iter().map(|(_, _, b)| b.clone()).collect();
        let _ = std::fs::remove_dir_all(&data);
        assert_eq!(after.iter().filter(|r| r[..2] == [0x00, 0x5b]).count(), 2, "the lights as the mouse keeps them: {after:02x?}");
        assert_eq!(after.iter().filter(|r| r[..2] == [0x00, 0x59]).count(), 1, "and the change kept, once");
    }

    /// A breath goes from dark up to its color and back down to dark, and a
    /// color shift comes back round to red, as each cycle ends on its first
    /// color: the mouse jumps from the last stop to the first.
    #[test]
    fn a_cycle_ends_where_it_starts() {
        let (core, sent, data) = mouse_core("cycle");
        let at = sent.lock().unwrap().len();
        let set = core.call("lighting/set", &serde_json::json!({ "device": "sensei310-1", "light": "logo", "effect": "breathing", "params": { "color": "#FF0000" } })).unwrap();
        core.call("lighting/set", &serde_json::json!({ "device": "sensei310-1", "light": "wheel", "effect": "colorshift", "params": {} })).unwrap();
        // what the light plays, as sent: told back for the page to play it on the drawing
        let told = serde_json::json!({ "ms": 3000, "held": false, "stops": [{ "at": 0, "color": "#000000" }, { "at": 127, "color": "#FF0000" }, { "at": 255, "color": "#000000" }] });
        assert_eq!(set["plays"], told, "lighting/set says what the light plays");
        let got = core.call("lighting/get", &serde_json::json!({ "device": "sensei310-1" })).unwrap();
        assert_eq!(got["logo"]["plays"], told, "and lighting/get");
        assert_eq!(got["wheel"]["plays"]["stops"].as_array().map(|a| a.len()), Some(7));
        let all = sent.lock().unwrap()[at..].to_vec();
        let _ = std::fs::remove_dir_all(&data);
        // (after the 3 bytes and the 26-byte header, the first color, then each stop's color and step)
        let stops = |r: &Vec<u8>| r[32..].chunks_exact(4).map(|c| ([c[0], c[1], c[2]], c[3])).collect::<Vec<_>>();
        let breath = &all[0].2;
        assert_eq!(breath[3 + 25], 3, "three stops");
        assert_eq!(stops(breath), [([0, 0, 0], 0), ([0xff, 0, 0], 127), ([0, 0, 0], 128)], "dark, red, dark again");
        let shift = &all[1].2;
        assert_eq!(stops(shift).len(), 7, "the wheel's six colors and red again");
        assert_eq!(stops(shift).last(), Some(&([0xff, 0, 0], 42)), "red again at the end");
    }

    /// A fake Z10 that answers as a recorded exchange did, taking `delay`
    /// over each answer as a keyboard takes time, and keeps the preview's
    /// frames (reports 6 and 7) with when each was sent.
    struct Recorded {
        answers: std::collections::HashMap<Vec<u8>, Vec<u8>>,
        pending: Option<Vec<u8>>,
        delay: Duration,
        frames: Arc<Mutex<Vec<(Instant, Vec<u8>)>>>,
    }

    impl Hid for Recorded {
        fn send_feature(&mut self, report: &[u8]) -> Result<(), String> {
            if report[0] == 6 || report[0] == 7 {
                self.frames.lock().unwrap().push((Instant::now(), report.to_vec()));
            } else {
                self.pending = Some(self.answers.get(report).cloned().ok_or("a report the recording never sent")?);
            }
            Ok(())
        }
        fn get_feature(&mut self, _: u8, _: usize) -> Result<Vec<u8>, String> {
            std::thread::sleep(self.delay);
            self.pending.clone().ok_or_else(|| "nothing asked".to_string())
        }
        fn write(&mut self, _: &[u8]) -> Result<usize, String> {
            Ok(0)
        }
        fn read(&mut self, _: u32) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
    }

    /// A preview asked for in the middle of a whole read (which takes a
    /// keyboard over half a second) reaches the keys between two of the
    /// read's exchanges, not after the read; it shows the moment of the
    /// effect the screen asked for; and the read gives what it gave alone.
    #[test]
    fn a_preview_goes_out_during_a_whole_read() {
        let root = crate::cores::dir().join("../parity");
        let mut text = String::new();
        flate2::read::GzDecoder::new(std::fs::File::open(root.join("vectors.json.gz")).unwrap()).read_to_string(&mut text).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        let case = v["device"]["editor"].as_array().unwrap().iter().find(|c| c["name"] == "state").unwrap().clone();
        let log: Log = Arc::new(Mutex::new(case["log"].as_array().unwrap().iter().map(|e| (e[0].as_str().unwrap().to_string(), unhex(e[1].as_str().unwrap()))).collect()));
        reads_once(&log);
        let entries: Vec<_> = log.lock().unwrap().iter().cloned().collect();
        let answers = entries.chunks(2).map(|p| (p[0].1.clone(), p[1].1.clone())).collect();
        let frames = Arc::new(Mutex::new(Vec::new()));
        let fake = Recorded { answers, pending: None, delay: Duration::from_millis(20), frames: frames.clone() };
        let data = std::env::temp_dir().join(format!("ectomatter-core-during-{}", std::process::id()));
        let core = Core::start(&core_dir(), data.clone(), Box::new(One(Some(Box::new(fake))))).unwrap();
        core.call("initialize", &serde_json::json!({ "backups": false })).unwrap();
        let reader = core.clone();
        let read = std::thread::spawn(move || {
            let t = Instant::now();
            (reader.call("device/read", &Value::Null), t.elapsed())
        });
        std::thread::sleep(Duration::from_millis(100));
        let asked = Instant::now();
        let wave = serde_json::json!({ "keys": { "effect": "wave", "effects": { "wave": { "cycle": 1000 } } } });
        core.call("lighting/preview", &serde_json::json!({ "lighting": wave, "tab": "test", "t": 5000 })).unwrap();
        let answered = asked.elapsed();
        let (state, took) = read.join().unwrap();
        core.call("lighting/preview/stop", &serde_json::json!({ "tab": "test" })).unwrap();
        let _ = std::fs::remove_dir_all(&data);
        let f = frames.lock().unwrap().clone();
        let first = f.iter().find(|(_, b)| b[0] == 7 && b[3] == 1).expect("a wave frame");
        let late = first.0 - asked;
        let phase = u16::from_le_bytes([first.1[7], first.1[8]]);
        eprintln!("the read took {took:?}; the preview was answered in {answered:?}, and its first frame went out {late:?} after it was asked, at {phase} ms into the cycle");
        assert!(took > Duration::from_millis(400), "the read took {took:?}, too quick to test with");
        assert!(first.0 < asked + took, "the first frame waited for the read");
        assert!(late < Duration::from_millis(60), "the first frame went out {late:?} after it was asked");
        assert!(answered < Duration::from_millis(60), "the preview was answered {answered:?} after it was asked");
        assert!(phase < 60, "the frame is {phase} ms into the cycle, where the screen was at 0");
        let mut state = state.unwrap();
        state["live"] = case["result"]["live"].clone();
        assert_eq!(state, shared_state(&case["result"]));
    }

    /// A fake Z10 that keeps every report sent to it, answering reads as
    /// `Clock` does.
    struct Sent(Arc<Mutex<Vec<Vec<u8>>>>, Clock, Arc<Mutex<Vec<Instant>>>);

    impl Hid for Sent {
        fn send_feature(&mut self, report: &[u8]) -> Result<(), String> {
            self.0.lock().unwrap().push(report.to_vec());
            self.2.lock().unwrap().push(Instant::now());
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
    /// lighting) is not sent again after the switch. The switch goes out at
    /// once, not at the next beat.
    #[test]
    fn off_previews_every_key_black() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let data = std::env::temp_dir().join(format!("ectomatter-core-off-{}", std::process::id()));
        let when = Arc::new(Mutex::new(Vec::new()));
        let fake = Sent(sent.clone(), Clock(Arc::new(Mutex::new(Vec::new())), Vec::new()), when.clone());
        let core = Core::start(&core_dir(), data.clone(), Box::new(One(Some(Box::new(fake))))).unwrap();
        core.call("initialize", &serde_json::json!({ "backups": false })).unwrap();
        let wave = serde_json::json!({ "keys": { "effect": "wave", "effects": { "wave": { "cycle": 1000 } } } });
        core.call("lighting/preview", &serde_json::json!({ "lighting": wave, "tab": "test" })).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        // just after a beat, so the next is most of a beat away
        let n = sent.lock().unwrap().len();
        while sent.lock().unwrap().len() == n {
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(3));
        let before = sent.lock().unwrap().len();
        let asked = Instant::now();
        let r = core.call("lighting/preview", &serde_json::json!({ "lighting": { "keys": { "effect": "off" } }, "tab": "test" })).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let after: Vec<Vec<u8>> = sent.lock().unwrap()[before..].to_vec();
        let switched = when.lock().unwrap()[before] - asked;
        let _ = std::fs::remove_dir_all(&data);
        assert_eq!(r["mode"], "keys");
        eprintln!("the switch to Off went out {switched:?} after it was asked");
        assert!(switched < Duration::from_millis(25), "the switch went out {switched:?} after it was asked");
        // the switch: leave the effect, then nothing but black keys
        assert_eq!(after[0][..4], [6, 0xEA, 0x02, 0], "stop the key stream first");
        assert_eq!(after[1][..4], [7, 0xEA, 0x02, 0], "leave the effect");
        let keys = &after[2..];
        assert!(keys.len() > 4, "{} frames of keys in 300 ms", keys.len());
        for k in keys {
            assert_eq!(k[..4], [6, 0xEA, 0x02, 1], "a key frame");
            assert_eq!(k.len(), 8 + 4 * evga_z10::protocol::NKEYS);
            assert!(k[8..].chunks(4).all(|c| c[1..] == [0, 0, 0]), "every key black");
        }
    }

    /// The preview's beat as the module keeps it, on Ectomatter's timer: a frame
    /// every 40 ms, as the keyboard code's own thread sends them. Prints the
    /// spacing; fails if the frames come more than 5 ms late on average.
    #[test]
    fn the_preview_keeps_its_beat() {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let data = std::env::temp_dir().join(format!("ectomatter-core-beat-{}", std::process::id()));
        let core = Core::start(&core_dir(), data.clone(), Box::new(One(Some(Box::new(Clock(frames.clone(), Vec::new())))))).unwrap();
        core.call("initialize", &serde_json::json!({ "backups": false })).unwrap();
        let lighting = serde_json::json!({ "mode": "breathing", "breathing": { "speed": 2, "colors": [{ "brightness": 255, "color": [255, 0, 0] }] } });
        core.call("lighting/preview", &serde_json::json!({ "lighting": evga_z10::shared::lighting_of(&lighting), "tab": "test" })).unwrap();
        std::thread::sleep(Duration::from_millis(2100));
        core.call("lighting/preview/stop", &serde_json::json!({ "tab": "test" })).unwrap();
        let spacing = |f: &[Instant]| {
            let gaps: Vec<f64> = f.windows(2).map(|w| (w[1] - w[0]).as_secs_f64() * 1000.0).collect();
            (gaps.iter().sum::<f64>() / gaps.len() as f64, gaps.iter().cloned().fold(0.0, f64::max), gaps.iter().cloned().fold(f64::MAX, f64::min))
        };
        let f = frames.lock().unwrap().clone();
        let (mean, worst, least) = spacing(&f);
        eprintln!("module: {} frames, {mean:.2} ms apart on average, {least:.2} to {worst:.2} ms", f.len());
        // the keyboard code's own thread, on the same fake keyboard, for comparison
        struct Kept(Clock);
        impl evga_z10::Transport for Kept {
            fn set_feature(&mut self, b: &[u8]) -> evga_z10::Result<()> {
                self.0.send_feature(b).map_err(evga_z10::Error::Link)
            }
            fn get_feature(&mut self, rid: u8, len: usize) -> evga_z10::Result<Vec<u8>> {
                self.0.get_feature(rid, len).map_err(evga_z10::Error::Link)
            }
            fn path(&self) -> String {
                "fake".into()
            }
        }
        let native = Arc::new(Mutex::new(Vec::new()));
        let n2 = native.clone();
        let opener: evga_z10::conn::Opener = Arc::new(move || Ok(evga_z10::Z10::new(Box::new(Kept(Clock(n2.clone(), Vec::new()))) as Box<dyn evga_z10::Transport>)));
        let kb = evga_z10::Keyboard::new(evga_z10::Conn::new(opener), None);
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
