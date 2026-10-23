//! The monitors, as plugins read them (the system interface's `monitors`)
//! and as Ectomatter changes them for a game (putback.rs). On KDE, through
//! KDE's own output protocols, as System Settings and kscreen-doctor use
//! them: kde-output-device-v2 to read each monitor, kde-output-management-v2
//! to change it; and its brightness through PowerDevil's ScreenBrightness on
//! D-Bus, so the system's brightness slider and keys agree with it. Each
//! operation makes a connection of its own and closes it.
//!
//! A monitor is known by an id made from its EDID (its maker's code, its
//! product, serial and week of making), never by the port it's on, which
//! changes between starts; KDE's own name for it holds the port too, where
//! its EDID has no serial, as both of Grant's monitors don't.
//!
//! What KDE does with each change, from its source (KWin 6.7.5): a change
//! names only the monitor it changes, and the others are left as they are;
//! HDR goes on and off with wide color, as System Settings sets them; the
//! main monitor is the one first in priority, so every monitor on is given
//! its place; turning one off is disabling it, and turning it on again
//! brings it back with its last mode, place and scale. KDE keeps every
//! change it's given (its own file), which is why Ectomatter's record of what
//! it changed is written first.
//!
//! Elsewhere there's no connection yet, and the Monitor Switcher says
//! Ectomatter can't reach this computer's monitors.

use serde_json::{json, Value};

/// A monitor's mode: its size in pixels and its refresh rate, in millihertz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh: u32,
}

impl Mode {
    pub fn to_value(self) -> Value {
        json!({ "width": self.width, "height": self.height, "refresh": self.refresh })
    }

    pub fn of(v: &Value) -> Option<Mode> {
        Some(Mode { width: v["width"].as_u64()? as u32, height: v["height"].as_u64()? as u32, refresh: v["refresh"].as_u64()? as u32 })
    }
}

/// A monitor as Ectomatter reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Monitor {
    pub id: String,
    /// its own name (from its EDID), its maker's code, and the maker's name as KDE writes it
    pub name: String,
    pub maker: String,
    pub make: String,
    /// the port it's on now, only to show
    pub port: String,
    pub enabled: bool,
    pub priority: u32,
    pub modes: Vec<Mode>,
    pub mode: Option<Mode>,
    /// what it can do, and how it is: none where it can't
    pub hdr: Option<bool>,
    pub wcg: bool,
    pub vrr: Option<u32>,
    pub brightness: Option<u32>,
    pub ddc: bool,
}

/// The capabilities KDE says a monitor has (kde_output_device_v2's).
const CAN_VRR: u32 = 0x2;
const CAN_HDR: u32 = 0x8;
const CAN_BRIGHTNESS: u32 = 0x80;

impl Monitor {
    pub fn to_value(&self, main: bool) -> Value {
        let mut v = json!({
            "id": self.id, "name": self.name, "maker": self.maker, "port": self.port, "enabled": self.enabled, "main": main,
            "modes": self.modes.iter().map(|m| m.to_value()).collect::<Vec<_>>(), "ddc": self.ddc,
        });
        if let Some(m) = self.mode {
            v["mode"] = m.to_value();
        }
        if let Some(h) = self.hdr {
            v["hdr"] = json!(h);
        }
        if let Some(p) = self.vrr {
            v["vrr"] = json!(vrr_word(p));
        }
        if let Some(b) = self.brightness {
            v["brightness"] = json!(b);
        }
        v
    }

    /// Its name as PowerDevil labels it: the maker's name, then its own.
    fn label(&self) -> String {
        if self.make.is_empty() || self.make == self.maker && self.name.starts_with(&self.maker) { self.name.clone() } else { format!("{} {}", self.make, self.name) }
    }
}

pub fn vrr_word(p: u32) -> &'static str {
    match p {
        1 => "always",
        2 => "automatic",
        _ => "never",
    }
}

fn vrr_policy(w: &str) -> Option<u32> {
    match w {
        "never" => Some(0),
        "always" => Some(1),
        "automatic" => Some(2),
        _ => None,
    }
}

/// The main monitor among several: the one on that's first in priority.
pub fn main_of(list: &[Monitor]) -> Option<&Monitor> {
    list.iter().filter(|m| m.enabled).min_by_key(|m| m.priority)
}

pub fn to_json(list: &[Monitor]) -> Value {
    let main = main_of(list).map(|m| m.id.clone());
    Value::Array(list.iter().map(|m| m.to_value(main.as_deref() == Some(&m.id))).collect())
}

// ---------------------------------------------------------------- the EDID

/// What a monitor's EDID says of it: its maker's three-letter code, its
/// product code, its serial number, its week and year of making, and its
/// name and serial as words, where it gives them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edid {
    pub maker: String,
    pub product: u16,
    pub serial: u32,
    pub week: u8,
    pub year: u16,
    pub name: Option<String>,
    pub serial_text: Option<String>,
}

pub fn edid(b: &[u8]) -> Option<Edid> {
    if b.len() < 128 || b[..8] != [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00] {
        return None;
    }
    let m = u16::from_be_bytes([b[8], b[9]]);
    let letter = |n: u16| char::from(b'A' - 1 + (n & 0x1F) as u8);
    let maker: String = [letter(m >> 10), letter(m >> 5), letter(m)].iter().collect();
    let mut e = Edid { maker, product: u16::from_le_bytes([b[10], b[11]]), serial: u32::from_le_bytes([b[12], b[13], b[14], b[15]]), week: b[16], year: 1990 + b[17] as u16, ..Default::default() };
    for d in [54usize, 72, 90, 108] {
        let block = &b[d..d + 18];
        if block[..3] != [0, 0, 0] {
            continue;
        }
        let text = String::from_utf8_lossy(&block[5..18]).split('\n').next().unwrap_or("").trim().to_string();
        match block[3] {
            0xFC if !text.is_empty() => e.name = Some(text),
            0xFF if !text.is_empty() => e.serial_text = Some(text),
            _ => {}
        }
    }
    Some(e)
}

/// A monitor's id: its maker, product, serial and the week it was made,
/// as its EDID says them, so the same monitor has the same id on any port.
pub fn id_of(e: &Edid) -> String {
    let serial = e.serial_text.as_deref().map(|s| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>()).filter(|s| !s.is_empty()).unwrap_or_else(|| format!("{:08X}", e.serial));
    format!("edid:{}-{:04X}-{serial}-{}w{:02}", e.maker, e.product, e.year, e.week)
}

fn unbase64(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes().filter_map(val) {
        acc = (acc << 6) | c as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

// ---------------------------------------------------------------- KDE

#[cfg(target_os = "linux")]
mod kde {
    use super::*;
    use crate::kde_output::client::kde_output_device_mode_v2::{self, KdeOutputDeviceModeV2};
    use crate::kde_output::client::kde_output_device_registry_v2::{self, KdeOutputDeviceRegistryV2};
    use crate::kde_output::client::kde_output_device_v2::{self, KdeOutputDeviceV2};
    use crate::kde_output::management::kde_output_configuration_v2::{self, KdeOutputConfigurationV2};
    use crate::kde_output::management::kde_output_management_v2::KdeOutputManagementV2;
    use wayland_client::protocol::wl_registry;
    use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, WEnum};

    /// A monitor as KDE tells it, while it's being told.
    #[derive(Default)]
    pub struct Dev {
        pub obj: Option<KdeOutputDeviceV2>,
        pub port: String,
        pub make: String,
        pub edid: Vec<u8>,
        pub enabled: bool,
        pub priority: u32,
        pub caps: u32,
        pub hdr: bool,
        pub wcg: bool,
        pub vrr: u32,
        pub brightness: u32,
        pub ddc: bool,
        pub x: i32,
        pub y: i32,
        pub scale: f64,
        pub current: Option<KdeOutputDeviceModeV2>,
        pub modes: Vec<(KdeOutputDeviceModeV2, Mode)>,
        pub done: bool,
        pub gone: bool,
    }

    #[derive(Default)]
    pub struct App {
        pub devs: Vec<Dev>,
        pub management: Option<KdeOutputManagementV2>,
        /// how the change applied: none yet, then whether it did and why not
        pub applied: Option<Result<(), String>>,
        pub why: String,
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for App {
        fn event(app: &mut Self, reg: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
            if let wl_registry::Event::Global { name, interface, version } = event {
                match interface.as_str() {
                    "kde_output_device_registry_v2" if version >= 21 => {
                        let _: KdeOutputDeviceRegistryV2 = reg.bind(name, version.min(23), qh, ());
                    }
                    "kde_output_management_v2" if version >= 9 => {
                        app.management = Some(reg.bind(name, version.min(21), qh, ()));
                    }
                    _ => {}
                }
            }
        }
    }

    impl Dispatch<KdeOutputDeviceRegistryV2, ()> for App {
        fn event(app: &mut Self, _: &KdeOutputDeviceRegistryV2, e: kde_output_device_registry_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            if let kde_output_device_registry_v2::Event::Output { output } = e {
                app.devs.push(Dev { obj: Some(output), scale: 1.0, ..Default::default() });
            }
        }
        wayland_client::event_created_child!(App, KdeOutputDeviceRegistryV2, [
            kde_output_device_registry_v2::EVT_OUTPUT_OPCODE => (KdeOutputDeviceV2, ()),
        ]);
    }

    impl Dispatch<KdeOutputDeviceV2, ()> for App {
        fn event(app: &mut Self, dev: &KdeOutputDeviceV2, e: kde_output_device_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            use kde_output_device_v2::Event as E;
            let Some(d) = app.devs.iter_mut().find(|d| d.obj.as_ref() == Some(dev)) else { return };
            match e {
                E::Geometry { x, y, make, .. } => {
                    d.x = x;
                    d.y = y;
                    d.make = make;
                }
                E::CurrentMode { mode } => d.current = Some(mode),
                E::Mode { mode } => d.modes.push((mode, Mode { width: 0, height: 0, refresh: 0 })),
                E::Done => d.done = true,
                E::Scale { factor } => d.scale = factor,
                E::Edid { raw } => d.edid = unbase64(&raw),
                E::Enabled { enabled } => d.enabled = enabled != 0,
                E::Capabilities { flags } => d.caps = match flags {
                    WEnum::Value(f) => f.bits(),
                    WEnum::Unknown(u) => u,
                },
                E::VrrPolicy { vrr_policy } => d.vrr = match vrr_policy {
                    WEnum::Value(p) => p as u32,
                    WEnum::Unknown(u) => u,
                },
                E::Name { name } => d.port = name,
                E::HighDynamicRange { hdr_enabled } => d.hdr = hdr_enabled != 0,
                E::WideColorGamut { wcg_enabled } => d.wcg = wcg_enabled != 0,
                E::Brightness { brightness } => d.brightness = brightness,
                E::DdcCiAllowed { allowed } => d.ddc = allowed != 0,
                E::Priority { priority } => d.priority = priority,
                E::Removed => d.gone = true,
                _ => {}
            }
        }
        wayland_client::event_created_child!(App, KdeOutputDeviceV2, [
            kde_output_device_v2::EVT_MODE_OPCODE => (KdeOutputDeviceModeV2, ()),
        ]);
    }

    impl Dispatch<KdeOutputDeviceModeV2, ()> for App {
        fn event(app: &mut Self, mode: &KdeOutputDeviceModeV2, e: kde_output_device_mode_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            use kde_output_device_mode_v2::Event as E;
            for d in &mut app.devs {
                if let Some(i) = d.modes.iter().position(|(m, _)| m == mode) {
                    match e {
                        E::Size { width, height } => {
                            d.modes[i].1.width = width.max(0) as u32;
                            d.modes[i].1.height = height.max(0) as u32;
                        }
                        E::Refresh { refresh } => d.modes[i].1.refresh = refresh.max(0) as u32,
                        E::Removed => {
                            d.modes.remove(i);
                        }
                        _ => {}
                    }
                    return;
                }
            }
        }
    }

    impl Dispatch<KdeOutputManagementV2, ()> for App {
        fn event(_: &mut Self, _: &KdeOutputManagementV2, _: <KdeOutputManagementV2 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
    }

    impl Dispatch<KdeOutputConfigurationV2, ()> for App {
        fn event(app: &mut Self, _: &KdeOutputConfigurationV2, e: kde_output_configuration_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            use kde_output_configuration_v2::Event as E;
            match e {
                E::Applied => app.applied = Some(Ok(())),
                E::FailureReason { reason } => app.why = reason,
                E::Failed => app.applied = Some(Err(if app.why.is_empty() { "KDE didn’t take the change.".into() } else { format!("KDE didn’t take the change: {}", app.why) })),
            }
        }
    }

    /// A connection to KDE with every monitor told whole (the connection kept
    /// for as long as the session is, its objects living on it).
    pub struct Session {
        pub _conn: Connection,
        pub queue: EventQueue<App>,
        pub app: App,
    }

    impl Session {
        pub fn open() -> Result<Session, String> {
            let conn = Connection::connect_to_env().map_err(|_| "Ectomatter can’t reach this computer’s monitors.".to_string())?;
            let mut queue = conn.new_event_queue();
            let qh = queue.handle();
            conn.display().get_registry(&qh, ());
            let mut app = App::default();
            queue.roundtrip(&mut app).map_err(|e| e.to_string())?;
            for _ in 0..20 {
                queue.roundtrip(&mut app).map_err(|e| e.to_string())?;
                if !app.devs.is_empty() && app.devs.iter().all(|d| d.done) {
                    break;
                }
            }
            if app.devs.is_empty() {
                return Err("Ectomatter can’t reach this computer’s monitors.".into());
            }
            Ok(Session { _conn: conn, queue, app })
        }

        /// The monitors as told, each with its id (one EDID twice made unique by its port).
        pub fn monitors(&self) -> Vec<(Monitor, usize)> {
            let mut out: Vec<(Monitor, usize)> = Vec::new();
            for (i, d) in self.app.devs.iter().enumerate().filter(|(_, d)| !d.gone) {
                let e = edid(&d.edid).unwrap_or_default();
                let mut id = if d.edid.is_empty() { format!("port:{}", d.port) } else { id_of(&e) };
                if out.iter().any(|(m, _)| m.id == id) {
                    id = format!("{id}@{}", d.port);
                }
                let mut modes: Vec<Mode> = d.modes.iter().map(|(_, m)| *m).filter(|m| m.width > 0).collect();
                modes.dedup();
                let mode = d.current.as_ref().and_then(|c| d.modes.iter().find(|(m, _)| m == c)).map(|(_, m)| *m);
                out.push((
                    Monitor {
                        id,
                        name: e.name.clone().unwrap_or_else(|| d.port.clone()),
                        maker: e.maker.clone(),
                        make: d.make.clone(),
                        port: d.port.clone(),
                        enabled: d.enabled,
                        priority: d.priority,
                        modes,
                        mode,
                        hdr: (d.caps & CAN_HDR != 0).then_some(d.hdr),
                        wcg: d.wcg,
                        vrr: (d.caps & CAN_VRR != 0).then_some(d.vrr),
                        brightness: (d.caps & CAN_BRIGHTNESS != 0).then_some(d.brightness),
                        ddc: d.ddc,
                    },
                    i,
                ));
            }
            out
        }

        /// One change made, as one configuration KDE applies whole, and read back.
        pub fn apply(&mut self, fill: impl FnOnce(&KdeOutputConfigurationV2, &App)) -> Result<(), String> {
            let qh = self.queue.handle();
            let m = self.app.management.clone().ok_or("KDE doesn’t let Ectomatter change its monitors here.")?;
            let cfg = m.create_configuration(&qh, ());
            fill(&cfg, &self.app);
            self.app.applied = None;
            self.app.why.clear();
            cfg.apply();
            let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while self.app.applied.is_none() && std::time::Instant::now() < until {
                self.queue.roundtrip(&mut self.app).map_err(|e| e.to_string())?;
                if self.app.applied.is_none() {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
            cfg.destroy();
            let r = self.app.applied.take().unwrap_or_else(|| Err("KDE didn’t answer the change.".into()));
            // (KDE says it applied before its monitors say how they are now)
            let _ = self.queue.roundtrip(&mut self.app);
            r
        }
    }
}

// ---------------------------------------------------------------- PowerDevil's brightness

/// A display PowerDevil knows: its D-Bus name, its label and its brightness (0 to 10000).
#[cfg(target_os = "linux")]
fn powerdevil() -> Result<(zbus::blocking::Connection, Vec<(String, String, i32)>), String> {
    let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
    let root = zbus::blocking::Proxy::new(&conn, "org.kde.ScreenBrightness", "/org/kde/ScreenBrightness", "org.kde.ScreenBrightness").map_err(|e| e.to_string())?;
    let names: Vec<String> = root.get_property("DisplaysDBusNames").map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for n in names {
        let d = zbus::blocking::Proxy::new(&conn, "org.kde.ScreenBrightness", format!("/org/kde/ScreenBrightness/{n}"), "org.kde.ScreenBrightness.Display").map_err(|e| e.to_string())?;
        let label: String = d.get_property("Label").unwrap_or_default();
        let b: i32 = d.get_property("Brightness").unwrap_or(-1);
        out.push((n, label, b));
    }
    Ok((conn, out))
}

/// Which of PowerDevil's displays a monitor is: by its label (PowerDevil
/// lists them in KDE's order, so two alike go by that order).
#[cfg(target_os = "linux")]
fn display_of(list: &[(String, String, i32)], m: &Monitor, same_before: usize) -> Option<usize> {
    let want = m.label();
    let matching: Vec<usize> = list.iter().enumerate().filter(|(_, (_, l, _))| *l == want || l.ends_with(&m.name)).map(|(i, _)| i).collect();
    matching.get(same_before).copied()
}

// ---------------------------------------------------------------- reading and changing

/// The monitors connected now.
pub fn list() -> Result<Vec<Monitor>, String> {
    #[cfg(target_os = "linux")]
    {
        let s = kde::Session::open()?;
        let mut list: Vec<Monitor> = s.monitors().into_iter().map(|(m, _)| m).collect();
        // (PowerDevil's brightness is the exact one, where it says)
        if let Ok((_, displays)) = powerdevil() {
            let all = list.clone();
            for (k, m) in list.iter_mut().enumerate() {
                let before = all[..k].iter().filter(|x| x.label() == m.label()).count();
                if let (Some(i), Some(_)) = (display_of(&displays, m, before), m.brightness) {
                    if displays[i].2 >= 0 {
                        m.brightness = Some(displays[i].2 as u32);
                    }
                }
            }
        }
        Ok(list)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("Ectomatter can’t reach this computer’s monitors yet.".into())
    }
}

/// Ectomatter's hands on the monitors, for the changes it makes for a game.
pub struct Hands;

impl crate::putback::Screens for Hands {
    fn now(&mut self, monitor: &str, set: &str) -> Result<Value, String> {
        let list = list()?;
        if set == "main" {
            return Ok(main_of(&list).map_or(Value::Null, |m| json!(m.id)));
        }
        let m = list.iter().find(|m| m.id == monitor).ok_or("That monitor isn’t connected now.")?;
        Ok(match set {
            "brightness" => json!(m.brightness.ok_or("Its brightness can’t be set.")?),
            "mode" => m.mode.ok_or("Its mode isn’t known.")?.to_value(),
            "hdr" => json!({ "hdr": m.hdr.ok_or("It has no HDR.")?, "wcg": m.wcg }),
            "vrr" => json!(vrr_word(m.vrr.ok_or("It has no variable refresh.")?)),
            "enabled" => json!(m.enabled),
            other => return Err(format!("Ectomatter doesn’t know how to change a monitor’s {other}.")),
        })
    }

    fn set(&mut self, monitor: &str, set: &str, to: &Value) -> Result<(), String> {
        set_one(monitor, set, to)
    }
}

#[cfg(target_os = "linux")]
fn set_one(monitor: &str, set: &str, to: &Value) -> Result<(), String> {
    let mut s = kde::Session::open()?;
    let all = s.monitors();
    let (m, i) = all.iter().find(|(m, _)| m.id == monitor).cloned().ok_or("That monitor isn’t connected now.")?;
    if set == "brightness" {
        let level = to.as_u64().ok_or("Brightness is a number.")?.min(10000) as i32;
        // through PowerDevil, so its slider and keys agree; KDE's own way where it isn't running
        if let Ok((conn, displays)) = powerdevil() {
            let k = all.iter().position(|(x, _)| x.id == m.id).unwrap_or(0);
            let before = all[..k].iter().filter(|(x, _)| x.label() == m.label()).count();
            if let Some(d) = display_of(&displays, &m, before) {
                let p = zbus::blocking::Proxy::new(&conn, "org.kde.ScreenBrightness", format!("/org/kde/ScreenBrightness/{}", displays[d].0), "org.kde.ScreenBrightness.Display").map_err(|e| e.to_string())?;
                let r: zbus::Result<()> = p.call("SetBrightnessWithContext", &(level, 1u32, "ectomatter"));
                return r.map_err(|e| e.to_string());
            }
        }
        return s.apply(|cfg, app| {
            if let Some(dev) = app.devs[i].obj.as_ref() {
                cfg.set_brightness(dev, level as u32);
            }
        });
    }
    let enabled_others: Vec<usize> = all.iter().filter(|(x, _)| x.enabled && x.id != m.id).map(|(_, j)| *j).collect();
    s.apply(|cfg, app| {
        let Some(dev) = app.devs[i].obj.as_ref() else { return };
        match set {
            "mode" => {
                if let Some(want) = Mode::of(to) {
                    // that mode, or the same size at the nearest rate
                    let pick = app.devs[i].modes.iter().filter(|(_, x)| x.width == want.width && x.height == want.height).min_by_key(|(_, x)| x.refresh.abs_diff(want.refresh));
                    if let Some((obj, _)) = pick {
                        cfg.mode(dev, obj);
                    }
                }
            }
            "hdr" => {
                let (hdr, wcg) = match to {
                    Value::Bool(b) => (*b, *b),
                    v => (v["hdr"].as_bool().unwrap_or(false), v["wcg"].as_bool().unwrap_or(false)),
                };
                cfg.set_high_dynamic_range(dev, hdr as u32);
                cfg.set_wide_color_gamut(dev, wcg as u32);
            }
            "vrr" => {
                if let Some(p) = to.as_str().and_then(vrr_policy) {
                    cfg.set_vrr_policy(dev, match p {
                        1 => crate::kde_output::management::kde_output_configuration_v2::VrrPolicy::Always,
                        2 => crate::kde_output::management::kde_output_configuration_v2::VrrPolicy::Automatic,
                        _ => crate::kde_output::management::kde_output_configuration_v2::VrrPolicy::Never,
                    });
                }
            }
            "enabled" => cfg.enable(dev, to.as_bool().unwrap_or(true) as i32),
            "main" => {
                // first in priority, and every other monitor on after it in its own order
                cfg.set_priority(dev, 1);
                let mut rest: Vec<usize> = enabled_others.clone();
                rest.sort_by_key(|j| app.devs[*j].priority);
                for (n, j) in rest.iter().enumerate() {
                    if let Some(o) = app.devs[*j].obj.as_ref() {
                        cfg.set_priority(o, n as u32 + 2);
                    }
                }
            }
            _ => {}
        }
    })
}

#[cfg(not(target_os = "linux"))]
fn set_one(_monitor: &str, _set: &str, _to: &Value) -> Result<(), String> {
    Err("Ectomatter can’t reach this computer’s monitors yet.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An EDID's base block with a maker, product, serial and name.
    fn sample() -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        // "MSI": M=13, S=19, I=9
        let m: u16 = (13 << 10) | (19 << 5) | 9;
        b[8..10].copy_from_slice(&m.to_be_bytes());
        b[10..12].copy_from_slice(&0x4DD0u16.to_le_bytes());
        b[16] = 51;
        b[17] = 33;
        b[54..59].copy_from_slice(&[0, 0, 0, 0xFC, 0]);
        b[59..72].copy_from_slice(b"Wide 340 OLED");
        b
    }

    #[test]
    fn a_monitor_is_known_by_its_edid() {
        let e = edid(&sample()).unwrap();
        assert_eq!((e.maker.as_str(), e.product, e.year, e.week), ("MSI", 0x4DD0, 2023, 51));
        assert_eq!(e.name.as_deref(), Some("Wide 340 OLED"));
        assert_eq!(id_of(&e), "edid:MSI-4DD0-00000000-2023w51");
    }

    #[test]
    fn base64_is_read() {
        assert_eq!(unbase64("AP///////wA="), vec![0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
    }

    /// Needs a running KDE session; reads only: `cargo test monitors_live -- --ignored --nocapture`
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn monitors_live() {
        let l = list().unwrap();
        println!("{}", serde_json::to_string_pretty(&to_json(&l)).unwrap());
        assert!(!l.is_empty());
    }
}
