//! Every sensor Ectodeck can read on this computer, for the Sensors box in Settings: found
//! under `root` (`/` but for tests), sorted into the parts of the computer the box has a tab
//! for (the processor, graphics, memory, the motherboard, a laptop's battery and its own
//! controller, drives, network), and named as people say them where Ectodeck knows what a
//! sensor reads (k10temp's Tccd1 is Chiplet 1, a drive's Composite is its Temperature), the
//! driver's own name kept beside it. Elsewhere the driver's name is used.
//!
//! A sensor is kept by what Linux calls it: its driver, the device it reads and its label
//! (`k10temp@0000:00:18.3/temp/Tctl`). The device is a PCI address, a platform device's
//! name, a memory stick's place on its bus or a drive's serial number; the number Linux gives
//! an hwmon at start-up can change between boots, so it is never part of one. The readings
//! Ectodeck works out itself have names of their own: `cpu/load`, `cpu/clock/average`,
//! `memory/used`, and a graphics card's `card@0000:01:00.0/temperature`.

use std::path::{Path, PathBuf};

use crate::sensors::{cards, Vendor};

// ------------------------------------------------------------------ what a sensor is

/// The parts of a computer, a tab each, in the tabs' order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Part {
    Processor,
    Graphics,
    Memory,
    Motherboard,
    Battery,
    Laptop,
    Drives,
    Network,
    Cooling,
    PowerSupply,
    Other,
}

impl Part {
    /// Its tab's name.
    pub fn name(self) -> &'static str {
        match self {
            Part::Processor => "Processor",
            Part::Graphics => "Graphics",
            Part::Memory => "Memory",
            Part::Motherboard => "Motherboard",
            Part::Battery => "Battery",
            Part::Laptop => "Laptop",
            Part::Drives => "Drives",
            Part::Network => "Network",
            Part::Cooling => "Cooling",
            Part::PowerSupply => "Power supply",
            Part::Other => "Other devices",
        }
    }

    /// Whether its tab's rows are its devices, as each has only a few readings, rather than
    /// the kinds of reading of its one device.
    pub fn by_device(self) -> bool {
        matches!(self, Part::Graphics | Part::Drives | Part::Network | Part::Cooling | Part::PowerSupply | Part::Other)
    }

    /// The word a row of the keyboard's screen says is unavailable when one of its readings
    /// can't be had and the row has no words of its own: "Board unavailable".
    pub fn short(self) -> &'static str {
        match self {
            Part::Processor => "CPU",
            Part::Graphics => "GPU",
            Part::Memory => "RAM",
            Part::Motherboard => "Board",
            Part::Battery => "Battery",
            Part::Laptop => "Laptop",
            Part::Drives => "Drive",
            Part::Network => "Network",
            Part::Cooling => "Cooling",
            Part::PowerSupply => "PSU",
            Part::Other => "Sensor",
        }
    }
}

/// What a sensor reads: the row it sits in on its tab, and how its reading is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum What {
    Temperature,
    Load,
    Clock,
    InUse,
    /// a fan's speed in RPM
    Fan,
    /// a fan's speed as a share of its fastest, as NVIDIA's driver gives it
    FanShare,
    Voltage,
    Power,
    Current,
    Charge,
}

impl What {
    /// The name of its row.
    pub fn row(self) -> &'static str {
        match self {
            What::Temperature => "Temperature",
            What::Load => "Load",
            What::Clock => "Clock",
            What::InUse => "In use",
            What::Fan | What::FanShare => "Fan",
            What::Voltage => "Voltage",
            What::Power => "Power",
            What::Current => "Current",
            What::Charge => "Charge",
        }
    }

    /// The word the screen editor adds to a sensor's name that doesn't say what it reads:
    /// "Chiplet 1 temperature".
    pub fn word(self) -> &'static str {
        match self {
            What::Temperature => "temperature",
            What::Load => "load",
            What::Clock => "clock",
            What::InUse => "in use",
            What::Fan | What::FanShare => "fan",
            What::Voltage => "voltage",
            What::Power => "power",
            What::Current => "current",
            What::Charge => "charge",
        }
    }
}

/// A run of like readings that its row keeps folded into one line until it is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Run {
    /// a reading for each of the processor's cores
    Cores,
    /// a card's temperatures after its main one
    Temps,
    Volts,
}

impl Run {
    pub fn name(self) -> &'static str {
        match self {
            Run::Cores => "Each core",
            Run::Temps => "More temperatures",
            Run::Volts => "Voltages",
        }
    }
}

/// What NVIDIA's driver gives for a card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Nv {
    Temp,
    Load,
    Clock,
    MemClock,
    Vram,
    Power,
    Fan,
}

/// How a sensor is read.
#[derive(Clone, Debug, PartialEq)]
pub enum How {
    /// a number in a file, divided by its scale (an hwmon's millidegrees, millivolts,
    /// microwatts and hertz; a fan's RPM as it is)
    File(PathBuf, f64),
    /// a count of microjoules, read twice: the watts between
    Energy(PathBuf),
    /// the processor's load: every thread's (none named), or these threads'
    Load(Vec<usize>),
    /// the processor's clock: these threads' average, or the fastest of them
    Clock(Vec<usize>, bool),
    /// the memory in use, and in all
    Memory,
    /// a card read through NVIDIA's driver, by its PCI address
    Nvml(String, Nv),
    /// an AMD card's load, from its driver's busy share
    Busy(PathBuf),
    /// an Intel card's load: how long its graphics slept since the last reading, in
    /// milliseconds (a count Intel's own tools read the same way)
    Asleep(PathBuf),
    /// an AMD card's video memory in use, and in all, in bytes
    Vram(PathBuf, PathBuf),
    /// a battery's charge
    Charge(PathBuf),
    /// the power going out of a battery: its own reading of it while it runs the computer
    Out(PathBuf),
}

/// One sensor, as the Sensors box lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Sensor {
    pub id: String,
    /// Ectodeck's name for it, or the driver's where Ectodeck doesn't know better
    pub name: String,
    /// the driver's own name for it, shown beside Ectodeck's ("Tctl")
    pub orig: Option<String>,
    pub what: What,
    pub run: Option<Run>,
    /// one of the readings the ready-made layouts show, checked from the start
    pub usual: bool,
    pub how: How,
}

/// A graphics card, as the main card is chosen among them.
#[derive(Clone, Debug, PartialEq)]
pub struct CardOf {
    pub pci: String,
    /// its name, as Linux's list of PCI devices gives it: "NVIDIA GeForce RTX 5090"
    pub name: String,
    pub vendor: Vendor,
    /// the graphics in the processor, rather than a card of its own
    pub integrated: bool,
    /// its video memory in all, GiB (0 when its driver doesn't say)
    pub vram: f64,
}

/// A device the sensors read: a tab's first line, or one of its rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    /// what Ectodeck keeps it by: `cpu`, `memory`, `board`, `card@0000:01:00.0`
    pub key: String,
    pub part: Part,
    /// its name as its row, or its tab's first line, says it: "RTX 5090"
    pub name: String,
    /// its whole name: "NVIDIA GeForce RTX 5090"
    pub full: String,
    /// where its readings come from: "From NVIDIA’s driver"
    pub src: String,
    /// a word under its name: "in the CPU", "Ethernet"
    pub inside: Option<String>,
    pub card: Option<CardOf>,
    pub sensors: Vec<Sensor>,
    /// a line under its row: what its driver doesn't report
    pub note: Option<String>,
}

/// Every sensor on the computer, by device; and each part whose readings can't come in,
/// with why (a motherboard whose sensor chip Linux has no driver for).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    pub devices: Vec<Device>,
    pub missing: Vec<(Part, String)>,
}

impl Catalog {
    pub fn sensor(&self, id: &str) -> Option<(&Device, &Sensor)> {
        self.devices.iter().find_map(|d| d.sensors.iter().find(|s| s.id == id).map(|s| (d, s)))
    }

    pub fn device(&self, key: &str) -> Option<&Device> {
        self.devices.iter().find(|d| d.key == key)
    }

    /// The parts that have a tab, in the tabs' order.
    pub fn parts(&self) -> Vec<Part> {
        let mut all: Vec<Part> = self.devices.iter().map(|d| d.part).chain(self.missing.iter().map(|(p, _)| *p)).collect();
        all.sort();
        all.dedup();
        all
    }

    /// The processor's own temperature that the ready-made layouts show: Tctl, or on Intel
    /// Package id 0, else its first.
    pub fn cpu_temp(&self) -> Option<&Sensor> {
        self.device("cpu")?.sensors.iter().find(|s| s.what == What::Temperature && s.usual)
    }

    /// The cards, and the main card by the rule: the card of its own with the most video
    /// memory (an NVIDIA card first where none says), else the graphics in the processor.
    pub fn cards(&self) -> Vec<&Device> {
        self.devices.iter().filter(|d| d.card.is_some()).collect()
    }

    pub fn main_by_rule(&self) -> Option<&Device> {
        let mut all = self.cards();
        all.sort_by(|a, b| {
            let (x, y) = (a.card.as_ref().unwrap(), b.card.as_ref().unwrap());
            x.integrated
                .cmp(&y.integrated)
                .then(y.vram.partial_cmp(&x.vram).unwrap_or(std::cmp::Ordering::Equal))
                .then((x.vendor != Vendor::Nvidia).cmp(&(y.vendor != Vendor::Nvidia)))
                .then(x.pci.cmp(&y.pci))
        });
        all.first().copied()
    }
}

/// What NVIDIA's driver says of a card: what it reports, and its video memory in all (GiB).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Probe {
    pub has: Vec<Nv>,
    pub vram: f64,
}

/// A reading: a number, or a share of a whole (memory in use, and in all, GiB).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Val {
    One(f64),
    Of(f64, f64),
}

impl Val {
    pub fn one(self) -> f64 {
        match self {
            Val::One(v) => v,
            Val::Of(u, _) => u,
        }
    }
}

/// NVIDIA's driver: what it says a card reports (none: the driver can't be reached), and
/// a card's reading now.
pub trait Nvidia: Send {
    fn probe(&mut self, pci: &str) -> Option<Probe>;
    fn read(&mut self, pci: &str, what: Nv) -> Option<Val>;
}

/// No NVIDIA driver to ask, for a list made from Linux's own files alone.
pub struct NoNvidia;

impl Nvidia for NoNvidia {
    fn probe(&mut self, _: &str) -> Option<Probe> {
        None
    }
    fn read(&mut self, _: &str, _: Nv) -> Option<Val> {
        None
    }
}

// ------------------------------------------------------------------ reading Linux's files

fn num(p: &Path) -> Option<f64> {
    std::fs::read_to_string(p).ok()?.trim().parse::<f64>().ok()
}

fn words(p: &Path) -> Option<String> {
    Some(std::fs::read_to_string(p).ok()?.trim().to_string()).filter(|s| !s.is_empty())
}

/// An hwmon's inputs of one kind ("temp", "fan", "in", "power", "energy", "freq", "curr"):
/// each one's number, its label if it has one, and its file, in their numbers' order. One
/// that can't be read now, or that its chip has turned off, is left out. (A power reading is
/// the average on drivers that give only that.)
fn inputs(h: &Path, kind: &str) -> Vec<(u32, Option<String>, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(h) else { return Vec::new() };
    let mut all: Vec<(u32, PathBuf)> = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix(kind) else { continue };
        let n = rest.strip_suffix("_input").or_else(|| (kind == "power").then(|| rest.strip_suffix("_average")).flatten());
        if let Some(n) = n.and_then(|n| n.parse::<u32>().ok()) {
            // (an input before an average of the same number)
            if !all.iter().any(|(m, _)| *m == n) || name.ends_with("_input") {
                all.retain(|(m, _)| *m != n);
                all.push((n, e.path()));
            }
        }
    }
    all.sort();
    all.into_iter()
        .filter(|(n, p)| num(p).is_some() && num(&h.join(format!("{kind}{n}_enable"))) != Some(0.0))
        .map(|(n, p)| (n, words(&h.join(format!("{kind}{n}_label"))), p))
        .collect()
}

/// The hwmon devices under `root`: each one's directory, its driver's name, and the device
/// it reads (none for some made by the kernel itself), in the order of the devices' places
/// in the computer, which, unlike the hwmons' numbers, stays from one boot to the next.
fn hwmons(root: &Path) -> Vec<(PathBuf, String, Option<PathBuf>)> {
    let Ok(rd) = std::fs::read_dir(root.join("sys/class/hwmon")) else { return Vec::new() };
    let mut all: Vec<(Option<PathBuf>, u32, PathBuf, String)> = rd
        .flatten()
        .filter_map(|e| {
            let n: u32 = e.file_name().to_str()?.strip_prefix("hwmon")?.parse().ok()?;
            let p = e.path();
            Some((std::fs::canonicalize(p.join("device")).ok(), n, p.clone(), words(&p.join("name"))?))
        })
        .collect();
    all.sort();
    all.into_iter().map(|(d, _, p, name)| (p, name, d)).collect()
}

/// 0000:01:00.0
fn is_pci(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 12 && b[4] == b':' && b[7] == b':' && b[10] == b'.' && s.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
}

/// A device on an I2C bus by its place: 1-0050
fn is_i2c(s: &str) -> bool {
    s.split_once('-').is_some_and(|(b, a)| !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit()) && a.len() == 4 && a.bytes().all(|c| c.is_ascii_hexdigit()))
}

/// A USB device by its port: 3-2, 1-1.4
fn is_usb_port(s: &str) -> bool {
    s.split_once('-').is_some_and(|(b, a)| !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit()) && !a.is_empty() && a.bytes().all(|c| c.is_ascii_digit() || c == b'.'))
}

/// What Ectodeck keeps a sensor's device by, from where Linux puts it: a drive by its
/// serial number, a memory stick by its place on its bus, a USB device by its port, a PCI
/// device by its address, anything else by its own name (nct6687.2592).
fn device_key(dev: &Path) -> String {
    let s = dev.to_string_lossy();
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    let base = parts.last().copied().unwrap_or("");
    if parts.len() >= 2 && parts[parts.len() - 2] == "nvme" {
        if let Some(serial) = words(&dev.join("serial")) {
            return serial;
        }
    }
    if is_i2c(base) {
        return base.to_string();
    }
    if s.contains("/usb") {
        if let Some(port) = parts.iter().rev().find(|p| is_usb_port(p)) {
            return port.to_string();
        }
    }
    parts.iter().rev().find(|p| is_pci(p)).copied().unwrap_or(base).to_string()
}

fn hex(p: &Path) -> Option<u32> {
    u32::from_str_radix(std::fs::read_to_string(p).ok()?.trim().trim_start_matches("0x"), 16).ok()
}

/// The nearest PCI device on a device's path: its address, its directory and its class.
fn pci_of(dev: &Path) -> Option<(String, PathBuf, u32)> {
    let mut p = dev.to_path_buf();
    loop {
        let name = p.file_name()?.to_str()?.to_string();
        if is_pci(&name) {
            let class = hex(&p.join("class"))?;
            return Some((name, p, class));
        }
        if !p.pop() {
            return None;
        }
    }
}

/// The line of the PCI ID list that names a device, as it stands: "GB202 [GeForce RTX 5090]".
fn pci_line(ids: &str, vendor: u32, device: u32) -> Option<String> {
    let (v, d) = (format!("{vendor:04x}"), format!("\t{device:04x}"));
    let mut lines = ids.lines().skip_while(|l| !l.starts_with(&v));
    lines.next()?;
    let line = lines.take_while(|l| !l.starts_with(|c: char| c.is_ascii_hexdigit())).find(|l| l.starts_with(&d))?;
    Some(line[d.len()..].trim().to_string())
}

/// The maker's line of the PCI ID list: "Aquantia Corp.".
fn pci_vendor(ids: &str, vendor: u32) -> Option<String> {
    let v = format!("{vendor:04x}");
    ids.lines().find(|l| l.starts_with(&v)).map(|l| l[v.len()..].trim().to_string())
}

/// A network chip's model, from its line of the PCI ID list: its first word with a number
/// in it that isn't a standard ("AQtion AQC113CS NBase-T/IEEE 802.3an Ethernet Controller" is
/// the AQC113CS, "MT7922 802.11ax PCI Express Wireless Network Adapter" the MT7922).
fn model_code(line: &str) -> Option<String> {
    let before = line.split('[').next().unwrap_or(line);
    before
        .split_whitespace()
        .map(|w| w.split('/').next().unwrap_or(w).trim_matches(|c: char| matches!(c, '*' | '(' | ')' | ',')))
        .find(|w| w.len() >= 3 && w.chars().any(|c| c.is_ascii_digit()) && !w.starts_with(|c: char| c.is_ascii_digit()) && !w.contains("802."))
        .map(String::from)
}

/// A maker as people say it.
fn maker(v: &str) -> String {
    let l = v.to_lowercase();
    for (k, s) in [("mediatek", "MediaTek"), ("aquantia", "Aquantia"), ("realtek", "Realtek"), ("broadcom", "Broadcom"), ("qualcomm", "Qualcomm"), ("marvell", "Marvell"), ("mellanox", "Mellanox")] {
        if l.contains(k) {
            return s.to_string();
        }
    }
    crate::sensors::short_vendor(v)
}

/// A motherboard's sensor chip as its maker names it, from its driver's name for it.
fn chip_name(driver: &str) -> String {
    let d = driver.to_lowercase();
    if let Some(n) = d.strip_prefix("nct") {
        return format!("Nuvoton NCT{}", n.to_uppercase());
    }
    for (p, m) in [("it8", "ITE"), ("f71", "Fintek"), ("f81", "Fintek"), ("w83", "Winbond")] {
        if d.starts_with(p) {
            return format!("{m} {}", d.to_uppercase());
        }
    }
    driver.to_string()
}

/// A count in words, for a device's line: "Four sticks".
fn count_of(n: usize, one: &str, many: &str) -> String {
    let w = ["One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten", "Eleven", "Twelve"];
    match w.get(n.wrapping_sub(1)) {
        Some(x) if n == 1 => format!("{x} {one}"),
        Some(x) => format!("{x} {many}"),
        None => format!("{n} {many}"),
    }
}

/// A card's name as its row says it, its maker and GeForce left off: "RTX 5090".
fn card_short(full: &str) -> String {
    let mut s = full;
    for p in ["NVIDIA ", "AMD ", "Intel "] {
        if let Some(r) = s.strip_prefix(p) {
            s = r;
            break;
        }
    }
    let s = s.strip_prefix("GeForce ").unwrap_or(s);
    s.strip_suffix(" GPU").unwrap_or(s).to_string()
}

/// Whether this computer is a laptop, by what its maker says it is.
fn laptop(root: &Path) -> bool {
    num(&root.join("sys/class/dmi/id/chassis_type")).is_some_and(|t| [8.0, 9.0, 10.0, 11.0, 14.0, 30.0, 31.0, 32.0].contains(&t))
}

/// A laptop's name: "Lenovo ThinkPad P1 Gen 7" (Lenovo puts its model's name in its
/// version, its name being a code).
fn laptop_name(root: &Path) -> String {
    let read = |f: &str| words(&root.join("sys/class/dmi/id").join(f));
    let vendor = read("sys_vendor").map(|v| crate::sensors::short_vendor(&v)).unwrap_or_default();
    let model = if vendor == "Lenovo" { read("product_version").or_else(|| read("product_name")) } else { read("product_name") }.unwrap_or_else(|| "This laptop".into());
    if vendor.is_empty() || model.starts_with(&vendor) { model } else { format!("{vendor} {model}") }
}

// ------------------------------------------------------------------ names

/// A label-less input's name, by its kind and its place among its kind on its device:
/// "Temperature", or "Fan 2" where there are more.
fn unlabeled(what: What, k: usize, of: usize) -> String {
    let w = match what {
        What::Temperature => "Temperature",
        What::Fan | What::FanShare => "Fan",
        What::Voltage => "Voltage",
        What::Power => "Power",
        What::Current => "Current",
        _ => what.row(),
    };
    if of > 1 { format!("{w} {}", k + 1) } else { w.to_string() }
}

/// What the processor's own driver calls a temperature, named as people say it.
fn cpu_temp_name(label: &str) -> (String, Option<String>) {
    match label {
        "Tctl" | "Package id 0" => ("Temperature".into(), Some(label.into())),
        "Tdie" => ("Die".into(), Some(label.into())),
        l => match l.strip_prefix("Tccd").and_then(|n| n.parse::<u32>().ok()) {
            Some(n) => (format!("Chiplet {n}"), Some(l.into())),
            None => (l.into(), None),
        },
    }
}

/// What a card's driver calls a temperature or a voltage, named as people say it.
fn card_input_name(label: &str) -> (String, Option<String>) {
    let plain = match label {
        "junction" | "hotspot" => Some("Hot spot".to_string()),
        "mem" | "vram" => Some("Video memory".into()),
        "mctrl" => Some("Memory controller".into()),
        "pcie" => Some("PCIe".into()),
        "vddgfx" => Some("Graphics".into()),
        "vddnb" => Some("Northbridge".into()),
        l => l.strip_prefix("vram_ch_").and_then(|n| n.parse::<u32>().ok()).map(|n| format!("Channel {}", n + 1)),
    };
    match plain {
        Some(p) => (p, Some(label.into())),
        None => (label.into(), None),
    }
}

/// An hwmon input's own readings, as the plain sensors of a device: each temperature, fan,
/// voltage, power and current, named by its label (with its kind's word left off where the
/// row says it: the Aquantia's "PHY Temperature" is its PHY), or by its kind and number.
fn plain_inputs(h: &Path, driver: &str, dev: &str) -> Vec<Sensor> {
    let mut out = Vec::new();
    for (kind, what, scale) in [("temp", What::Temperature, 1000.0), ("fan", What::Fan, 1.0), ("in", What::Voltage, 1000.0), ("power", What::Power, 1e6), ("curr", What::Current, 1000.0)] {
        let all = inputs(h, kind);
        for (k, (n, label, path)) in all.iter().enumerate() {
            let name = match label {
                Some(l) if what == What::Temperature => l.strip_suffix(" Temperature").or_else(|| l.strip_suffix(" Temp")).unwrap_or(l).to_string(),
                Some(l) => l.clone(),
                None => unlabeled(what, k, all.len()),
            };
            let tag = label.clone().unwrap_or_else(|| format!("{kind}{n}"));
            out.push(Sensor {
                id: format!("{driver}@{dev}/{kind}/{tag}"),
                name,
                orig: None,
                what,
                run: (what == What::Voltage).then_some(Run::Volts),
                usual: false,
                how: How::File(path.clone(), scale),
            });
        }
    }
    out
}

// ------------------------------------------------------------------ finding them

/// The threads Linux runs on the processor, by core: each core's threads, in the order of
/// its first (a core's threads share its clock; each counts its own load).
fn cores(root: &Path) -> Vec<Vec<usize>> {
    let Ok(rd) = std::fs::read_dir(root.join("sys/devices/system/cpu")) else { return Vec::new() };
    let mut threads: Vec<(usize, (i64, i64))> = rd
        .flatten()
        .filter_map(|e| {
            let n: usize = e.file_name().to_str()?.strip_prefix("cpu")?.parse().ok()?;
            let t = e.path().join("topology");
            // (a thread whose place isn't known is a core of its own)
            let place = match (num(&t.join("physical_package_id")), num(&t.join("core_id"))) {
                (Some(p), Some(c)) => (p as i64, c as i64),
                _ => (-1, n as i64),
            };
            Some((n, place))
        })
        .collect();
    threads.sort();
    let mut out: Vec<((i64, i64), Vec<usize>)> = Vec::new();
    for (n, place) in threads {
        match out.iter_mut().find(|(p, _)| *p == place) {
            Some((_, ts)) => ts.push(n),
            None => out.push((place, vec![n])),
        }
    }
    out.into_iter().map(|(_, ts)| ts).collect()
}

/// The processor: its own driver's temperatures, its load and its clocks.
fn processor(root: &Path, temps: Vec<Sensor>) -> Device {
    let mut sensors = temps;
    let cores = cores(root);
    let has_temps = !sensors.is_empty();
    // the usual temperature: Tctl, on Intel Package id 0, else the first
    let usual = sensors.iter().position(|s| matches!(s.orig.as_deref(), Some("Tctl" | "Package id 0"))).or((!sensors.is_empty()).then_some(0));
    if let Some(i) = usual {
        sensors[i].usual = true;
    }
    sensors.push(Sensor { id: "cpu/load".into(), name: "All cores".into(), orig: None, what: What::Load, run: None, usual: true, how: How::Load(Vec::new()) });
    if cores.len() > 1 {
        for (i, ts) in cores.iter().enumerate() {
            sensors.push(Sensor { id: format!("cpu/load/core{}", i + 1), name: format!("Core {}", i + 1), orig: None, what: What::Load, run: Some(Run::Cores), usual: false, how: How::Load(ts.clone()) });
        }
    }
    let all: Vec<usize> = cores.iter().flatten().copied().collect();
    if !all.is_empty() {
        sensors.push(Sensor { id: "cpu/clock/average".into(), name: "Average".into(), orig: None, what: What::Clock, run: None, usual: true, how: How::Clock(all.clone(), false) });
        sensors.push(Sensor { id: "cpu/clock/fastest".into(), name: "Fastest core".into(), orig: None, what: What::Clock, run: None, usual: false, how: How::Clock(all, true) });
        if cores.len() > 1 {
            for (i, ts) in cores.iter().enumerate() {
                sensors.push(Sensor { id: format!("cpu/clock/core{}", i + 1), name: format!("Core {}", i + 1), orig: None, what: What::Clock, run: Some(Run::Cores), usual: false, how: How::Clock(ts.clone(), true) });
            }
        }
    }
    let name = crate::sensors::cpu_name(root);
    let src = if has_temps { "Temperatures from the processor’s own sensor, load and clocks from Linux" } else { "Load and clocks from Linux" };
    Device { key: "cpu".into(), part: Part::Processor, full: name.clone(), name, src: src.into(), inside: None, card: None, sensors, note: None }
}

/// A graphics card: its usual readings first, as the ready-made layouts show them, then its
/// driver's other temperatures and its voltages, folded.
fn card(root: &Path, c: &crate::sensors::Card, hw: Option<&(PathBuf, String)>, nv: &mut dyn Nvidia) -> Device {
    let key = format!("card@{}", c.pci);
    let id = |k: &str| format!("{key}/{k}");
    let mut sensors = Vec::new();
    let mut vram = 0.0;
    let mut note = None;
    let push = |sensors: &mut Vec<Sensor>, k: &str, name: &str, orig: Option<&str>, what: What, usual: bool, how: How| {
        sensors.push(Sensor { id: id(k), name: name.into(), orig: orig.map(String::from), what, run: None, usual, how });
    };
    let dir = root.join("sys/bus/pci/devices").join(&c.pci);
    // (a card no driver runs, as one given over to a virtual machine, has nothing to read)
    let driven = dir.join("driver").exists();
    let undriven = || Some("No Linux driver is running this card, so it can’t be read.".to_string());
    match c.vendor {
        _ if !driven => note = undriven(),
        Vendor::Nvidia => match nv.probe(&c.pci) {
            Some(p) => {
                vram = p.vram;
                for (k, name, what, usual, n) in [
                    ("temperature", "Temperature", What::Temperature, true, Nv::Temp),
                    ("load", "Load", What::Load, true, Nv::Load),
                    ("clock", "Clock", What::Clock, true, Nv::Clock),
                    ("video-memory", "Video memory", What::InUse, true, Nv::Vram),
                    ("memory-clock", "Memory clock", What::Clock, false, Nv::MemClock),
                    ("power", "Power", What::Power, false, Nv::Power),
                    ("fan", "Fan", What::FanShare, false, Nv::Fan),
                ] {
                    if p.has.contains(&n) {
                        push(&mut sensors, k, name, None, what, usual, How::Nvml(c.pci.clone(), n));
                    }
                }
            }
            None => note = Some("NVIDIA’s driver isn’t answering, so this card can’t be read until it loads.".to_string()),
        },
        Vendor::Amd | Vendor::Intel => {
            let (h, driver) = match hw {
                Some((h, d)) => (h.clone(), d.clone()),
                None => (PathBuf::new(), String::new()),
            };
            let temps = inputs(&h, "temp");
            // the main temperature: the edge of AMD's chip, the package of Intel's
            let main = temps.iter().position(|(_, l, _)| matches!(l.as_deref(), Some("edge" | "pkg"))).or((!temps.is_empty()).then_some(0));
            if let Some(i) = main {
                let (_, l, p) = &temps[i];
                push(&mut sensors, "temperature", "Temperature", l.as_deref(), What::Temperature, true, How::File(p.clone(), 1000.0));
            }
            let freqs = inputs(&h, "freq");
            let freq = |want: &str| freqs.iter().find(|(_, l, _)| l.as_deref() == Some(want)).map(|(_, _, p)| p.clone());
            if c.vendor == Vendor::Amd {
                if num(&dir.join("gpu_busy_percent")).is_some() {
                    push(&mut sensors, "load", "Load", None, What::Load, true, How::Busy(dir.join("gpu_busy_percent")));
                }
                if let Some(p) = freq("sclk") {
                    push(&mut sensors, "clock", "Clock", None, What::Clock, true, How::File(p, 1e6));
                }
                let (used, total) = (dir.join("mem_info_vram_used"), dir.join("mem_info_vram_total"));
                if let (Some(_), Some(t)) = (num(&used), num(&total)) {
                    vram = t / (1u64 << 30) as f64;
                    push(&mut sensors, "video-memory", "Video memory", None, What::InUse, true, How::Vram(used, total));
                }
                if let Some(p) = freq("mclk") {
                    push(&mut sensors, "memory-clock", "Memory clock", None, What::Clock, false, How::File(p, 1e6));
                }
                if let Some((_, l, p)) = inputs(&h, "power").into_iter().next() {
                    push(&mut sensors, "power", "Power", l.as_deref(), What::Power, false, How::File(p, 1e6));
                }
            } else {
                // Intel's: its graphics' sleep, and its clock now (0 while it sleeps), from its first tile
                let gt = dir.join("tile0/gt0");
                if num(&gt.join("gtidle/idle_residency_ms")).is_some() {
                    push(&mut sensors, "load", "Load", None, What::Load, true, How::Asleep(gt.join("gtidle/idle_residency_ms")));
                }
                if num(&gt.join("freq0/act_freq")).is_some() {
                    push(&mut sensors, "clock", "Clock", None, What::Clock, true, How::File(gt.join("freq0/act_freq"), 1.0));
                }
                let energy = inputs(&h, "energy");
                for (_, l, p) in &energy {
                    match l.as_deref() {
                        Some("card") => push(&mut sensors, "power", "Power", Some("card"), What::Power, false, How::Energy(p.clone())),
                        Some("pkg") => push(&mut sensors, "chip-power", "Chip power", Some("pkg"), What::Power, false, How::Energy(p.clone())),
                        _ => {}
                    }
                }
            }
            for (k, (n, _, p)) in inputs(&h, "fan").iter().enumerate() {
                let key = if k == 0 { "fan".to_string() } else { format!("fan{n}") };
                let name = if k == 0 { "Fan".to_string() } else { format!("Fan {}", k + 1) };
                push(&mut sensors, &key, &name, None, What::Fan, false, How::File(p.clone(), 1.0));
            }
            // the driver's other temperatures and its voltages, folded under the card's row
            for (i, (n, l, p)) in temps.iter().enumerate() {
                if Some(i) == main {
                    continue;
                }
                let tag = l.clone().unwrap_or_else(|| format!("temp{n}"));
                let (name, orig) = card_input_name(&tag);
                sensors.push(Sensor { id: format!("{driver}@{}/temp/{tag}", c.pci), name, orig, what: What::Temperature, run: Some(Run::Temps), usual: false, how: How::File(p.clone(), 1000.0) });
            }
            for (n, l, p) in inputs(&h, "in") {
                let tag = l.clone().unwrap_or_else(|| format!("in{n}"));
                let (name, orig) = card_input_name(&tag);
                sensors.push(Sensor { id: format!("{driver}@{}/in/{tag}", c.pci), name, orig, what: What::Voltage, run: Some(Run::Volts), usual: false, how: How::File(p, 1000.0) });
            }
            // what the ready-made layouts show that its driver doesn't report
            let lacks: Vec<&str> = [("temperature", "temperature"), ("load", "load"), ("clock", "clock"), ("video-memory", "video memory")]
                .into_iter()
                .filter(|(k, _)| !sensors.iter().any(|s| s.id == id(k)))
                .map(|(_, w)| w)
                .collect();
            if !lacks.is_empty() {
                let these = if c.integrated { "these graphics" } else { "this card" };
                let said = match lacks.as_slice() {
                    [one] => one.to_string(),
                    [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
                    [] => String::new(),
                };
                note = Some(format!("Linux’s driver for {these} reports no {said}."));
            }
        }
    }
    let src = match c.vendor {
        Vendor::Nvidia => "From NVIDIA’s driver",
        Vendor::Amd => "From Linux’s AMD graphics driver",
        Vendor::Intel => "From Linux’s Intel graphics driver",
    };
    Device {
        key,
        part: Part::Graphics,
        name: card_short(&c.name),
        full: if c.integrated { format!("{}, in the processor", c.name) } else { c.name.clone() },
        src: src.into(),
        inside: c.integrated.then(|| "in the CPU".to_string()),
        card: Some(CardOf { pci: c.pci.clone(), name: c.name.clone(), vendor: c.vendor, integrated: c.integrated, vram }),
        sensors,
        note,
    }
}

/// A battery's charge and the power going out of it. A battery Linux says powers a device of
/// its own rather than the computer (a game controller's, a mouse's: its scope is Device) is
/// that device's, named as the device names itself.
fn battery(dir: &Path) -> Option<Device> {
    if words(&dir.join("type")).as_deref() != Some("Battery") {
        return None;
    }
    let name = dir.file_name()?.to_string_lossy().into_owned();
    let key = format!("battery@{name}");
    let mut sensors = Vec::new();
    if num(&dir.join("capacity")).is_some() {
        sensors.push(Sensor { id: format!("{key}/charge"), name: "Charge".into(), orig: None, what: What::Charge, run: None, usual: false, how: How::Charge(dir.join("capacity")) });
    }
    if words(&dir.join("scope")).as_deref() == Some("Device") {
        // (the device's own name: the nearest "product" up its path, as USB gives it)
        let mut at = std::fs::canonicalize(dir.join("device")).ok();
        let mut product = words(&dir.join("model_name"));
        while product.is_none() {
            let Some(p) = at.as_mut() else { break };
            if !p.pop() {
                break;
            }
            product = words(&p.join("product"));
        }
        let name = product.unwrap_or_else(|| "A device’s battery".into());
        return (!sensors.is_empty()).then(|| Device { key, part: Part::Other, full: name.clone(), name, src: "From the device’s own battery".into(), inside: None, card: None, sensors, note: None });
    }
    if num(&dir.join("power_now")).is_some() || num(&dir.join("current_now")).is_some() {
        sensors.push(Sensor { id: format!("{key}/power"), name: "Going out".into(), orig: None, what: What::Power, run: None, usual: false, how: How::Out(dir.to_path_buf()) });
    }
    let wh = num(&dir.join("energy_full_design")).map(|uwh| uwh / 1e6).filter(|w| *w > 0.0);
    Some(Device {
        key,
        part: Part::Battery,
        name: wh.map_or("Battery".into(), |w| format!("{w:.0} Wh battery")),
        full: "The battery".into(),
        src: "From the battery".into(),
        inside: None,
        card: None,
        sensors,
        note: None,
    })
}

/// Every sensor on the computer under `root`, by device, the devices in their tabs' order
/// (the cards in their PCI addresses' order, the main card's place being the Settings' to give).
pub fn catalog(root: &Path, nv: &mut dyn Nvidia) -> Catalog {
    let ids = std::fs::read_to_string(root.join("usr/share/hwdata/pci.ids")).unwrap_or_default();
    let on_laptop = laptop(root);
    let mut devices: Vec<Device> = Vec::new();
    let mut cpu_temps: Vec<Sensor> = Vec::new();
    let mut sticks: Vec<(String, String, PathBuf)> = Vec::new();
    let mut board: Vec<(String, Vec<Sensor>)> = Vec::new();
    let mut card_hw: Vec<(String, PathBuf, String)> = Vec::new();
    let mut odd: Vec<Device> = Vec::new();
    for (h, driver, dev) in hwmons(root) {
        let Some(dev) = dev else { continue };
        let key = device_key(&dev);
        let path = dev.to_string_lossy().into_owned();
        // (a battery's own hwmon is read with the battery)
        if path.contains("/power_supply/") {
            continue;
        }
        match driver.as_str() {
            "k10temp" | "zenpower" | "coretemp" => {
                for (n, l, p) in inputs(&h, "temp") {
                    let tag = l.clone().unwrap_or_else(|| format!("temp{n}"));
                    let (name, orig) = cpu_temp_name(&tag);
                    let run = (driver == "coretemp" && tag.starts_with("Core ")).then_some(Run::Cores);
                    cpu_temps.push(Sensor { id: format!("{driver}@{key}/temp/{tag}"), name, orig, what: What::Temperature, run, usual: false, how: How::File(p, 1000.0) });
                }
                continue;
            }
            "spd5118" | "jc42" => {
                if let Some((_, _, p)) = inputs(&h, "temp").into_iter().next() {
                    sticks.push((key, driver.clone(), p));
                }
                continue;
            }
            _ => {}
        }
        let pci = pci_of(&dev);
        match pci.as_ref().map(|(_, _, class)| class >> 16) {
            // a graphics card's own sensors are read with the card
            Some(0x03) => {
                card_hw.push((pci.unwrap().0, h, driver));
                continue;
            }
            Some(0x01) | Some(0x02) => {
                let (addr, pdir, class) = pci.unwrap();
                let drive = class >> 16 == 0x01;
                let mut sensors = plain_inputs(&h, &driver, &key);
                let name = if drive {
                    // (a drive by its model, "SSD" left off: Samsung 9100 PRO 4TB)
                    let model = words(&dev.join("model")).or_else(|| words(&dev.join("device/model"))).unwrap_or_else(|| "Drive".into());
                    model.split_whitespace().filter(|w| *w != "SSD").collect::<Vec<_>>().join(" ")
                } else {
                    let (v, d) = (hex(&pdir.join("vendor")).unwrap_or(0), hex(&pdir.join("device")).unwrap_or(0));
                    let model = pci_line(&ids, v, d).and_then(|l| model_code(&l)).unwrap_or_else(|| driver.clone());
                    let m = pci_vendor(&ids, v).map(|v| maker(&v)).unwrap_or_default();
                    if m.is_empty() { model } else { format!("{m} {model}") }
                };
                // a drive's Composite is its own temperature, the one its maker gives
                for s in &mut sensors {
                    if s.name == "Composite" {
                        s.name = "Temperature".into();
                        s.orig = Some("Composite".into());
                    }
                }
                let inside = (!drive).then(|| if class >> 8 == 0x0280 { "Wi-Fi".to_string() } else { "Ethernet".to_string() });
                let dkey = if drive { format!("drive@{key}") } else { format!("net@{addr}") };
                let (part, src) = if drive { (Part::Drives, "From the drive") } else { (Part::Network, "From the card’s driver") };
                devices.push(Device { key: dkey, part, full: name.clone(), name, src: src.into(), inside, card: None, sensors, note: None });
                continue;
            }
            _ => {}
        }
        let on_board = (path.contains("/platform/") || driver == "acpitz") && !path.contains("/usb");
        if on_board {
            let mut sensors = plain_inputs(&h, &driver, &key);
            if driver == "acpitz" {
                for s in &mut sensors {
                    s.name = "Inside".into();
                    s.orig = Some("acpitz".into());
                }
            }
            board.push((driver.clone(), sensors));
            continue;
        }
        // anything else: a cooler or a power supply on USB, or a device of its own
        let part = match driver.as_str() {
            "nzxt-kraken2" | "nzxt-kraken3" | "nzxt-smart2" | "corsair-cpro" | "aquacomputer_d5next" | "d5next" | "quadro" | "octo" | "farbwerk" | "farbwerk360" | "highflownext" | "asus_rog_ryujin" => Part::Cooling,
            "corsairpsu" | "nzxt-psu" => Part::PowerSupply,
            _ => Part::Other,
        };
        let name = words(&dev.join("../product")).or_else(|| words(&dev.join("product"))).unwrap_or_else(|| driver.clone());
        odd.push(Device { key: format!("{driver}@{key}"), part, full: name.clone(), name, src: "From its driver".into(), inside: None, card: None, sensors: plain_inputs(&h, &driver, &key), note: None });
    }
    // the processor first
    devices.insert(0, processor(root, cpu_temps));
    // the cards, each with its driver's own sensors
    for c in cards(root) {
        let hw = card_hw.iter().find(|(p, _, _)| *p == c.pci).map(|(_, h, d)| (h.clone(), d.clone()));
        devices.push(card(root, &c, hw.as_ref(), nv));
    }
    // the memory: in use, and each stick's temperature, by its place on its bus
    sticks.sort();
    let gib = |kb: f64| kb * 1024.0 / (1u64 << 30) as f64;
    let total = std::fs::read_to_string(root.join("proc/meminfo")).ok().and_then(|s| s.lines().find(|l| l.starts_with("MemTotal:")).and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok())).map(gib);
    let mut mem = vec![Sensor { id: "memory/used".into(), name: "Memory".into(), orig: None, what: What::InUse, run: None, usual: true, how: How::Memory }];
    for (i, (key, driver, p)) in sticks.iter().enumerate() {
        mem.push(Sensor { id: format!("{driver}@{key}/temp/temp1"), name: format!("Stick {}", i + 1), orig: None, what: What::Temperature, run: None, usual: false, how: How::File(p.clone(), 1000.0) });
    }
    let (mem_name, mem_src) = if sticks.is_empty() {
        (total.map_or("Memory".into(), |t| format!("{t:.0} GB of memory")), "From Linux")
    } else {
        let kind = if sticks.iter().all(|(_, d, _)| d == "spd5118") { "DDR5" } else { "memory" };
        (format!("{} of {kind}", count_of(sticks.len(), "stick", "sticks")), "In use from Linux, temperatures from each stick")
    };
    devices.push(Device { key: "memory".into(), part: Part::Memory, full: mem_name.clone(), name: mem_name, src: mem_src.into(), inside: None, card: None, sensors: mem, note: None });
    // the motherboard's sensor chips, or a laptop's own controller
    let mut missing = Vec::new();
    let chips: Vec<String> = board.iter().filter(|(d, _)| d != "acpitz").map(|(d, _)| chip_name(d)).collect();
    if on_laptop {
        let sensors: Vec<Sensor> = board.into_iter().flat_map(|(_, s)| s).collect();
        if !sensors.is_empty() {
            devices.push(Device { key: "laptop".into(), part: Part::Laptop, name: laptop_name(root), full: laptop_name(root), src: "From the laptop’s own controller".into(), inside: None, card: None, sensors, note: None });
        }
    } else {
        let name = crate::sensors::board_name(root).unwrap_or_else(|| "The motherboard".into());
        let sensors: Vec<Sensor> = board.into_iter().flat_map(|(_, s)| s).collect();
        if chips.is_empty() {
            // (MSI's boards of the last years carry Nuvoton's NCT6687, which Linux reads only with its own driver)
            let msi = name.starts_with("MSI");
            missing.push((
                Part::Motherboard,
                if msi {
                    "Linux has no driver for this board’s sensor chip, a Nuvoton NCT6687, so its temperatures, fans and voltages can’t be read. The nct6687 driver adds them.".to_string()
                } else {
                    "Linux has no driver for this board’s sensor chip, so its temperatures, fans and voltages can’t be read.".to_string()
                },
            ));
        }
        if !sensors.is_empty() || chips.is_empty() {
            let src = match chips.len() {
                0 => String::new(),
                1 => format!("From its sensor chip, a {}", chips[0]),
                _ => format!("From its sensor chips, {}", crate::sensors::and_list(&chips.iter().map(|c| format!("a {c}")).collect::<Vec<_>>())),
            };
            devices.push(Device { key: "board".into(), part: Part::Motherboard, full: name.clone(), name, src, inside: None, card: None, sensors, note: None });
        }
    }
    // a laptop's batteries
    if let Ok(rd) = std::fs::read_dir(root.join("sys/class/power_supply")) {
        let mut all: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        all.sort();
        devices.extend(all.iter().filter_map(|d| battery(d)));
    }
    devices.extend(odd);
    // every device with something to read, in its tab's order; drives by their PCI addresses
    devices.retain(|d| !d.sensors.is_empty() || d.card.is_some() || d.part == Part::Motherboard);
    devices.sort_by_key(|d| d.part);
    Catalog { devices, missing }
}

// ------------------------------------------------------------------ for the editor

impl Sensor {
    /// Its name in the screen editor's menu, under its device: its own, with what it reads
    /// after it where its name doesn't say ("Chiplet 1 temperature"); a name given it, as given.
    pub fn menu_name(&self, given: Option<&str>) -> String {
        if let Some(g) = given {
            return g.to_string();
        }
        let n = self.name.to_lowercase();
        let says = match self.what {
            What::Fan | What::FanShare => n.contains("fan") || n.contains("pump"),
            What::Voltage => n.contains("volt") || n.ends_with('v') || n.starts_with('+') || n.starts_with("vdd") || n.contains("vcore"),
            w => n.contains(w.word()) || (w == What::Clock && n.contains("clock")),
        };
        if says || self.name == "Temperature" { self.name.clone() } else { format!("{} {}", self.name, self.what.word()) }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A made-up NVIDIA driver: the cards it has, each with what it reports.
    pub struct FakeNvidia(pub Vec<(String, Probe)>);

    impl Nvidia for FakeNvidia {
        fn probe(&mut self, pci: &str) -> Option<Probe> {
            self.0.iter().find(|(p, _)| p == pci).map(|(_, p)| p.clone())
        }
        // (the RTX 5090 as it read on October 16)
        fn read(&mut self, pci: &str, what: Nv) -> Option<Val> {
            self.probe(pci).filter(|p| p.has.contains(&what))?;
            Some(match what {
                Nv::Temp => Val::One(43.0),
                Nv::Load => Val::One(6.0),
                Nv::Clock => Val::One(1882.0),
                Nv::MemClock => Val::One(7001.0),
                Nv::Vram => Val::Of(4.8, 31.8),
                Nv::Power => Val::One(73.4),
                Nv::Fan => Val::One(0.0),
            })
        }
    }

    /// A made-up computer like Grant's: an AMD processor with two chiplets and four cores of
    /// two threads, an MSI board's NCT6687 (a temperature reading 193°, fans, voltages), two
    /// sticks of DDR5, two drives, an NVIDIA card, an Intel Arc card and the processor's own
    /// Radeon graphics, an Aquantia Ethernet chip and a MediaTek Wi-Fi card.
    pub fn computer(dir: &Path) {
        let w = |p: &str, s: &str| {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, format!("{s}\n")).unwrap();
        };
        let n = std::cell::Cell::new(0);
        let hw = |name: &str, dev: &str, files: &[(&str, &str)]| {
            let h = dir.join(format!("sys/class/hwmon/hwmon{}", n.get()));
            n.set(n.get() + 1);
            std::fs::create_dir_all(&h).unwrap();
            std::fs::write(h.join("name"), format!("{name}\n")).unwrap();
            let d = dir.join(dev);
            std::fs::create_dir_all(&d).unwrap();
            std::os::unix::fs::symlink(&d, h.join("device")).unwrap();
            for (f, v) in files {
                std::fs::write(h.join(f), format!("{v}\n")).unwrap();
            }
        };
        hw("k10temp", "sys/devices/pci0000:00/0000:00:18.3", &[("temp1_input", "66000"), ("temp1_label", "Tctl"), ("temp3_input", "58000"), ("temp3_label", "Tccd1"), ("temp4_input", "54000"), ("temp4_label", "Tccd2")]);
        hw("nct6687", "sys/devices/platform/nct6687.2592", &[
            ("temp1_input", "65000"), ("temp1_label", "CPU"), ("temp2_input", "50000"), ("temp2_label", "System"), ("temp7_input", "193000"), ("temp7_label", "M2_1"),
            ("fan1_input", "734"), ("fan1_label", "CPU Fan"), ("fan2_input", "2836"), ("fan2_label", "Pump Fan"),
            ("in0_input", "12000"), ("in0_label", "+12V"), ("in1_input", "4980"), ("in1_label", "+5V"), ("in10_input", "1550"), ("in10_label", "Voltage #2"),
        ]);
        hw("spd5118", "sys/devices/pci0000:00/0000:00:14.0/i2c-1/1-0051", &[("temp1_input", "49000"), ("temp1_enable", "1")]);
        hw("spd5118", "sys/devices/pci0000:00/0000:00:14.0/i2c-1/1-0050", &[("temp1_input", "47000"), ("temp1_enable", "1")]);
        w("sys/devices/pci0000:00/0000:00:01.2/0000:02:00.0/class", "0x010802");
        w("sys/devices/pci0000:00/0000:00:01.2/0000:02:00.0/nvme/nvme0/model", "Samsung SSD 9100 PRO 4TB          ");
        w("sys/devices/pci0000:00/0000:00:01.2/0000:02:00.0/nvme/nvme0/serial", "S0EXAMPLE00001M     ");
        hw("nvme", "sys/devices/pci0000:00/0000:00:01.2/0000:02:00.0/nvme/nvme0", &[("temp1_input", "47850"), ("temp1_label", "Composite"), ("temp2_input", "53850"), ("temp2_label", "Sensor 1")]);
        w("sys/devices/pci0000:00/0000:00:02.1/0000:0f:00.0/class", "0x010802");
        w("sys/devices/pci0000:00/0000:00:02.1/0000:0f:00.0/nvme/nvme3/model", "WDS200T1X0E-00AFY0");
        w("sys/devices/pci0000:00/0000:00:02.1/0000:0f:00.0/nvme/nvme3/serial", "100000000002");
        hw("nvme", "sys/devices/pci0000:00/0000:00:02.1/0000:0f:00.0/nvme/nvme3", &[("temp1_input", "42850"), ("temp1_label", "Composite")]);
        // the Ethernet chip names its hwmon after its network: enp6s0
        for (f, v) in [("class", "0x020000"), ("vendor", "0x1d6a"), ("device", "0x94c0")] {
            w(&format!("sys/devices/pci0000:00/0000:00:02.1/0000:06:00.0/{f}"), v);
        }
        hw("enp6s0", "sys/devices/pci0000:00/0000:00:02.1/0000:06:00.0", &[("temp1_input", "50000"), ("temp1_label", "PHY Temperature"), ("temp2_input", "50000"), ("temp2_label", "MAC Temperature")]);
        for (f, v) in [("class", "0x028000"), ("vendor", "0x14c3"), ("device", "0x0616")] {
            w(&format!("sys/devices/pci0000:00/0000:00:02.1/0000:0b:00.0/{f}"), v);
        }
        hw("mt7921_phy0", "sys/devices/pci0000:00/0000:00:02.1/0000:0b:00.0/ieee80211/phy0", &[("temp1_input", "48000")]);
        // the cards
        let card = |path: &str, v: &str, d: &str| {
            for (f, s) in [("class", "0x030000"), ("vendor", v), ("device", d)] {
                w(&format!("sys/devices/{path}/{f}"), s);
            }
            let at = dir.join("sys/bus/pci/devices");
            std::fs::create_dir_all(&at).unwrap();
            std::os::unix::fs::symlink(dir.join("sys/devices").join(path), at.join(path.rsplit('/').next().unwrap())).unwrap();
            // (a driver running it)
            std::fs::create_dir_all(dir.join("sys/bus/pci/drivers/gpu")).unwrap();
            std::os::unix::fs::symlink(dir.join("sys/bus/pci/drivers/gpu"), dir.join("sys/devices").join(path).join("driver")).unwrap();
        };
        card("pci0000:00/0000:00:01.1/0000:01:00.0", "0x10de", "0x2b85");
        let arc = "pci0000:00/0000:00:02.2/0000:16:00.0";
        card(arc, "0x8086", "0xe212");
        w(&format!("sys/devices/{arc}/tile0/gt0/gtidle/idle_residency_ms"), "28005947");
        w(&format!("sys/devices/{arc}/tile0/gt0/freq0/act_freq"), "1200");
        hw("xe", &format!("sys/devices/{arc}"), &[
            ("temp2_input", "49000"), ("temp2_label", "pkg"), ("temp3_input", "48000"), ("temp3_label", "vram"), ("temp6_input", "48000"), ("temp6_label", "vram_ch_0"),
            ("fan1_input", "1255"), ("energy1_input", "1000000"), ("energy1_label", "card"), ("energy2_input", "500000"), ("energy2_label", "pkg"),
        ]);
        let radeon = "pci0000:00/0000:00:08.1/0000:18:00.0";
        card(radeon, "0x1002", "0x13c0");
        w(&format!("sys/devices/{radeon}/gpu_busy_percent"), "6");
        w(&format!("sys/devices/{radeon}/mem_info_vram_used"), "75743232");
        w(&format!("sys/devices/{radeon}/mem_info_vram_total"), "2147483648");
        hw("amdgpu", &format!("sys/devices/{radeon}"), &[
            ("temp1_input", "54000"), ("temp1_label", "edge"), ("freq1_input", "2200000000"), ("freq1_label", "sclk"), ("power1_input", "100000"), ("power1_label", "PPT"),
            ("in0_input", "1340"), ("in0_label", "vddgfx"), ("in1_input", "1200"), ("in1_label", "vddnb"),
        ]);
        w("usr/share/hwdata/pci.ids", "1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t13c0  Granite Ridge [Radeon Graphics]\n10de  NVIDIA Corporation\n\t2b85  GB202 [GeForce RTX 5090]\n14c3  MEDIATEK Corp.\n\t0616  MT7922 802.11ax PCI Express Wireless Network Adapter\n1d6a  Aquantia Corp.\n\t94c0  AQtion AQC113CS NBase-T/IEEE 802.3an Ethernet Controller [Antigua 10G]\n8086  Intel Corporation\n\te212  Battlemage G21 [Arc Pro B50]");
        // the processor: four cores of two threads (0 and 4 share the first)
        for t in 0..8 {
            w(&format!("sys/devices/system/cpu/cpu{t}/topology/physical_package_id"), "0");
            w(&format!("sys/devices/system/cpu/cpu{t}/topology/core_id"), &(t % 4).to_string());
            w(&format!("sys/devices/system/cpu/cpu{t}/cpufreq/scaling_cur_freq"), &(if t < 4 { 5_400_000 + t * 100_000 } else { 624_194 }).to_string());
        }
        w("proc/cpuinfo", "processor\t: 0\nmodel name\t: AMD Ryzen 9 9950X3D 16-Core Processor\ncpu cores\t: 4");
        w("proc/meminfo", "MemTotal:       63119752 kB\nMemFree:        1 kB\nMemAvailable:   28100000 kB");
        w("sys/class/dmi/id/board_vendor", "Micro-Star International Co., Ltd.");
        w("sys/class/dmi/id/board_name", "MEG X670E ACE (MS-7D69)");
        w("sys/class/dmi/id/chassis_type", "3");
    }

    pub fn nvidia() -> FakeNvidia {
        FakeNvidia(vec![("0000:01:00.0".into(), Probe { has: vec![Nv::Temp, Nv::Load, Nv::Clock, Nv::Vram, Nv::MemClock, Nv::Power, Nv::Fan], vram: 31.8 })])
    }

    pub fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ectodeck-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn names(d: &Device) -> Vec<(String, Option<String>)> {
        d.sensors.iter().filter(|s| s.run.is_none()).map(|s| (s.name.clone(), s.orig.clone())).collect()
    }

    #[test]
    fn every_sensor_found_named_and_sorted_into_its_part() {
        let dir = temp_dir("list");
        computer(&dir);
        let cat = catalog(&dir, &mut nvidia());
        let parts: Vec<&str> = cat.parts().iter().map(|p| p.name()).collect();
        assert_eq!(parts, ["Processor", "Graphics", "Memory", "Motherboard", "Drives", "Network"]);
        // the processor: Tctl is its temperature, the chiplets by number; its load and clocks, each core folded
        let cpu = cat.device("cpu").unwrap();
        assert_eq!(cpu.name, "AMD Ryzen 9 9950X3D");
        let n = |s: &str, o: Option<&str>| (s.to_string(), o.map(String::from));
        assert_eq!(names(cpu), [n("Temperature", Some("Tctl")), n("Chiplet 1", Some("Tccd1")), n("Chiplet 2", Some("Tccd2")), n("All cores", None), n("Average", None), n("Fastest core", None)]);
        assert_eq!(cat.cpu_temp().map(|s| s.id.as_str()), Some("k10temp@0000:00:18.3/temp/Tctl"));
        let each: Vec<(&str, &How)> = cpu.sensors.iter().filter(|s| s.run == Some(Run::Cores) && s.what == What::Clock).map(|s| (s.name.as_str(), &s.how)).collect();
        assert_eq!(each.len(), 4);
        assert_eq!(each[0], ("Core 1", &How::Clock(vec![0, 4], true)));
        // the cards in their PCI order, each named as its row says it
        let cards: Vec<(&str, &str, Option<&str>)> = cat.cards().iter().map(|d| (d.name.as_str(), d.full.as_str(), d.inside.as_deref())).collect();
        assert_eq!(cards, [("RTX 5090", "NVIDIA GeForce RTX 5090", None), ("Arc Pro B50", "Intel Arc Pro B50", None), ("Radeon Graphics", "AMD Radeon Graphics, in the processor", Some("in the CPU"))]);
        assert_eq!(cat.main_by_rule().map(|d| d.key.as_str()), Some("card@0000:01:00.0"));
        let nv: Vec<&str> = cat.device("card@0000:01:00.0").unwrap().sensors.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(nv, ["Temperature", "Load", "Clock", "Video memory", "Memory clock", "Power", "Fan"]);
        // Intel's: its load from its sleep, its power from its energy; no video memory, said so
        let arc = cat.device("card@0000:16:00.0").unwrap();
        assert_eq!(names(arc), [n("Temperature", Some("pkg")), n("Load", None), n("Clock", None), n("Power", Some("card")), n("Chip power", Some("pkg")), n("Fan", None)]);
        let more: Vec<(&str, Option<&str>, Option<Run>)> = arc.sensors.iter().filter(|s| s.run.is_some()).map(|s| (s.name.as_str(), s.orig.as_deref(), s.run)).collect();
        assert_eq!(more, [("Video memory", Some("vram"), Some(Run::Temps)), ("Channel 1", Some("vram_ch_0"), Some(Run::Temps))]);
        assert_eq!(arc.note.as_deref(), Some("Linux’s driver for this card reports no video memory."));
        let ig = cat.device("card@0000:18:00.0").unwrap();
        assert_eq!(names(ig), [n("Temperature", Some("edge")), n("Load", None), n("Clock", None), n("Video memory", None), n("Power", Some("PPT"))]);
        assert_eq!(ig.sensors.iter().filter(|s| s.run == Some(Run::Volts)).map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Graphics", "Northbridge"]);
        assert_eq!(ig.card.as_ref().map(|c| c.vram), Some(2.0));
        // the memory: in use, then each stick by its place on the bus
        let mem = cat.device("memory").unwrap();
        assert_eq!((mem.name.as_str(), names(mem)), ("Two sticks of DDR5", vec![n("Memory", None), n("Stick 1", None), n("Stick 2", None)]));
        assert_eq!(mem.sensors[1].id, "spd5118@1-0050/temp/temp1");
        // the board: its chip's own names, the voltages folded
        let board = cat.device("board").unwrap();
        assert_eq!((board.name.as_str(), board.src.as_str()), ("MSI MEG X670E ACE", "From its sensor chip, a Nuvoton NCT6687"));
        assert_eq!(names(board), [n("CPU", None), n("System", None), n("M2_1", None), n("CPU Fan", None), n("Pump Fan", None)]);
        let volts: Vec<&str> = board.sensors.iter().filter(|s| s.run == Some(Run::Volts)).map(|s| s.name.as_str()).collect();
        assert_eq!(volts, ["+12V", "+5V", "Voltage #2"]);
        assert_eq!(board.sensors[4].id, "nct6687@nct6687.2592/fan/Pump Fan");
        // the drives by their models, kept by their serial numbers; network chips by their models
        let rows: Vec<(&str, Option<&str>, &str)> = cat.devices.iter().filter(|d| d.part == Part::Drives || d.part == Part::Network).map(|d| (d.name.as_str(), d.inside.as_deref(), d.key.as_str())).collect();
        assert_eq!(rows, [("Samsung 9100 PRO 4TB", None, "drive@S0EXAMPLE00001M"), ("WDS200T1X0E-00AFY0", None, "drive@100000000002"), ("Aquantia AQC113CS", Some("Ethernet"), "net@0000:06:00.0"), ("MediaTek MT7922", Some("Wi-Fi"), "net@0000:0b:00.0")]);
        assert_eq!(names(cat.device("drive@S0EXAMPLE00001M").unwrap()), [n("Temperature", Some("Composite")), n("Sensor 1", None)]);
        assert_eq!(names(cat.device("net@0000:06:00.0").unwrap()), [n("PHY", None), n("MAC", None)]);
        assert_eq!(names(cat.device("net@0000:0b:00.0").unwrap()), [n("Temperature", None)]);
        // the editor's names
        let (_, c1) = cat.sensor("k10temp@0000:00:18.3/temp/Tccd1").unwrap();
        assert_eq!((c1.menu_name(None), c1.menu_name(Some("Hot die"))), ("Chiplet 1 temperature".to_string(), "Hot die".to_string()));
        let (_, pump) = cat.sensor("nct6687@nct6687.2592/fan/Pump Fan").unwrap();
        assert_eq!(pump.menu_name(None), "Pump Fan");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_controller_s_battery_is_its_own_and_a_card_no_driver_runs_says_so() {
        let dir = temp_dir("list-controller");
        computer(&dir);
        // a game controller's battery, which powers the controller, not the computer
        let usb = dir.join("sys/devices/pci0000:00/0000:00:08.1/0000:18:00.3/usb5/5-2");
        std::fs::create_dir_all(usb.join("5-2:1.3/0003:054C:0CE6.004D")).unwrap();
        std::fs::write(usb.join("product"), "DualSense Wireless Controller\n").unwrap();
        let bat = dir.join("sys/class/power_supply/ps-controller-battery-48:18:8d:ca:8e:0c");
        std::fs::create_dir_all(&bat).unwrap();
        for (f, v) in [("type", "Battery"), ("scope", "Device"), ("capacity", "85"), ("status", "Discharging")] {
            std::fs::write(bat.join(f), format!("{v}\n")).unwrap();
        }
        std::os::unix::fs::symlink(usb.join("5-2:1.3/0003:054C:0CE6.004D"), bat.join("device")).unwrap();
        // and the Arc given over to a virtual machine: no driver of Linux's runs it
        std::fs::remove_file(dir.join("sys/devices/pci0000:00/0000:00:02.2/0000:16:00.0/driver")).unwrap();
        let cat = catalog(&dir, &mut nvidia());
        assert!(!cat.parts().contains(&Part::Battery));
        let pad = cat.devices.iter().find(|d| d.key.starts_with("battery@")).unwrap();
        assert_eq!((pad.part, pad.name.as_str(), names(pad)), (Part::Other, "DualSense Wireless Controller", vec![("Charge".to_string(), None)]));
        let arc = cat.device("card@0000:16:00.0").unwrap();
        assert!(arc.sensors.is_empty());
        assert_eq!(arc.note.as_deref(), Some("No Linux driver is running this card, so it can’t be read."));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_board_whose_chip_linux_has_no_driver_for_says_so() {
        let dir = temp_dir("list-bad");
        computer(&dir);
        std::fs::remove_dir_all(dir.join("sys/class/hwmon/hwmon1")).unwrap();
        // and NVIDIA's driver not loaded
        let cat = catalog(&dir, &mut FakeNvidia(Vec::new()));
        assert!(cat.missing.iter().any(|(p, why)| *p == Part::Motherboard && why.contains("Nuvoton NCT6687")));
        let board = cat.device("board").unwrap();
        assert!(board.sensors.is_empty());
        let nv = cat.device("card@0000:01:00.0").unwrap();
        assert!(nv.sensors.is_empty() && nv.note.is_some());
        // with no card's video memory known, the NVIDIA card is still the main card
        assert_eq!(cat.main_by_rule().map(|d| d.key.as_str()), Some("card@0000:01:00.0"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
