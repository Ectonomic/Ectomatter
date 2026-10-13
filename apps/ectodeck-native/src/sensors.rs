//! Where the screen's readings come from where there is more than one source: the sensors
//! that read the processor's temperature, how its clock is counted, and the graphics cards,
//! found on this computer; and the choice of each, kept in Ectodeck's settings ("sensors")
//! and shared with every reader of the screen's readings.
//!
//! A CPU sensor is every temperature from the processor's own driver (k10temp on AMD,
//! coretemp on Intel), then the motherboard chip's temperatures labeled CPU, kept by driver
//! and label (an hwmon's number can change between boots). A card is kept by its PCI address
//! and name. Everything is read under `root`, which is `/` but for tests.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde_json::{json, Value};

/// A sensor that reads the processor's temperature.
#[derive(Clone, Debug, PartialEq)]
pub struct CpuSensor {
    /// the hwmon's name: k10temp, coretemp, or the motherboard chip's (nct6687)
    pub driver: String,
    pub label: String,
    /// from the processor's own driver, rather than the motherboard's
    pub own: bool,
    pub input: PathBuf,
}

impl CpuSensor {
    /// Its reading now, in °C.
    pub fn read(&self) -> Option<f64> {
        let v: i64 = std::fs::read_to_string(&self.input).ok()?.trim().parse().ok()?;
        Some(v as f64 / 1000.0)
    }
}

const OWN: [&str; 3] = ["k10temp", "zenpower", "coretemp"];

/// The hwmon devices under `root`, in their numbers' order: each one's directory and name.
fn hwmons(root: &Path) -> Vec<(PathBuf, String)> {
    let Ok(rd) = std::fs::read_dir(root.join("sys/class/hwmon")) else { return Vec::new() };
    let mut all: Vec<(u32, PathBuf)> = rd.flatten().filter_map(|e| Some((e.file_name().to_str()?.strip_prefix("hwmon")?.parse().ok()?, e.path()))).collect();
    all.sort();
    all.into_iter().filter_map(|(_, p)| Some((p.clone(), std::fs::read_to_string(p.join("name")).ok()?.trim().to_string()))).collect()
}

/// An hwmon's temperatures: each one's label (or `temp1` and so on, where it has none) and
/// the file it reads from, in their numbers' order.
fn temps(h: &Path) -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(h) else { return Vec::new() };
    let mut all: Vec<(u32, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_str()?.strip_prefix("temp")?.strip_suffix("_input")?.parse().ok()?;
            Some((n, e.path()))
        })
        .collect();
    all.sort();
    all.into_iter()
        .map(|(n, p)| (std::fs::read_to_string(h.join(format!("temp{n}_label"))).map_or_else(|_| format!("temp{n}"), |l| l.trim().to_string()), p))
        .collect()
}

/// Every sensor that reads the processor: its own driver's first, then the motherboard's.
pub fn cpu_sensors(root: &Path) -> Vec<CpuSensor> {
    let mut own = Vec::new();
    let mut board = Vec::new();
    for (h, name) in hwmons(root) {
        if OWN.contains(&name.as_str()) {
            own.extend(temps(&h).into_iter().map(|(label, input)| CpuSensor { driver: name.clone(), label, own: true, input }));
            continue;
        }
        // (a motherboard's sensor chip sits on the platform bus, as no card or drive does)
        let on_board = std::fs::canonicalize(h.join("device")).is_ok_and(|d| d.to_string_lossy().contains("/platform/")) && name != "acpitz";
        if on_board {
            board.extend(temps(&h).into_iter().filter(|(l, _)| l.to_uppercase().contains("CPU")).map(|(label, input)| CpuSensor { driver: name.clone(), label, own: false, input }));
        }
    }
    own.extend(board);
    own
}

/// The processor's name, as its sensors' heading: "AMD Ryzen 9 9950X3D".
pub fn cpu_name(root: &Path) -> String {
    let info = std::fs::read_to_string(root.join("proc/cpuinfo")).unwrap_or_default();
    let model = info.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.trim() == "model name").map(|(_, v)| v.trim().to_string())).unwrap_or_else(|| "The processor".into());
    let mut name = model.replace("(R)", "").replace("(TM)", "").replace("(tm)", "");
    for cut in [" with Radeon Graphics", " Processor", " CPU"] {
        if let Some(i) = name.find(cut) {
            name.truncate(i);
        }
    }
    // ("16-Core", "Eight-Core" and the clock Intel adds: "@ 3.60GHz")
    let words: Vec<&str> = name.split_whitespace().filter(|w| !w.ends_with("-Core") && !w.starts_with('@') && !w.ends_with("GHz")).collect();
    words.join(" ")
}

/// How many cores the processor has (not threads), as Linux counts them.
pub fn cpu_cores(root: &Path) -> Option<u32> {
    let info = std::fs::read_to_string(root.join("proc/cpuinfo")).ok()?;
    info.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.trim() == "cpu cores").and_then(|(_, v)| v.trim().parse().ok()))
}

/// The motherboard's name, as its sensors' heading: "MSI MEG X670E ACE".
pub fn board_name(root: &Path) -> Option<String> {
    let read = |f: &str| std::fs::read_to_string(root.join("sys/class/dmi/id").join(f)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let board = read("board_name")?;
    // (the maker's model code in brackets, "(MS-7D69)", left off)
    let board = board.split(" (").next().unwrap_or(&board).trim().to_string();
    let vendor = read("board_vendor").map(|v| short_vendor(&v)).unwrap_or_default();
    Some(if vendor.is_empty() || board.starts_with(&vendor) { board } else { format!("{vendor} {board}") })
}

/// Names in a sentence: "A", "A and B", "A, B and C".
pub fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A maker as people say it: Micro-Star International Co., Ltd. is MSI.
fn short_vendor(v: &str) -> String {
    let l = v.to_lowercase();
    for (k, s) in [("micro-star", "MSI"), ("asustek", "ASUS"), ("gigabyte", "Gigabyte"), ("asrock", "ASRock"), ("nvidia", "NVIDIA"), ("advanced micro devices", "AMD"), ("intel", "Intel"), ("dell", "Dell"), ("lenovo", "Lenovo"), ("hewlett", "HP"), ("biostar", "Biostar")] {
        if l.contains(k) {
            return s.to_string();
        }
    }
    v.split_whitespace().next().unwrap_or("").trim_end_matches(',').to_string()
}

// ------------------------------------------------------------------ graphics cards

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
}

/// A graphics card in this computer.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    /// its PCI address, 0000:01:00.0
    pub pci: String,
    pub name: String,
    pub vendor: Vendor,
    /// the graphics in the processor, rather than a card of its own
    pub integrated: bool,
    dir: PathBuf,
}

impl Card {
    /// Its name as its menu lists it: the graphics in a processor say so.
    pub fn shown(&self) -> String {
        if self.integrated { format!("{}, in the processor", self.name) } else { self.name.clone() }
    }
}

/// The graphics cards Ectodeck can read, in their PCI addresses' order.
pub fn cards(root: &Path) -> Vec<Card> {
    let Ok(rd) = std::fs::read_dir(root.join("sys/bus/pci/devices")) else { return Vec::new() };
    let hex = |p: &Path, f: &str| std::fs::read_to_string(p.join(f)).ok().and_then(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok());
    let ids = std::fs::read_to_string(root.join("usr/share/hwdata/pci.ids")).unwrap_or_default();
    let mut all: Vec<Card> = rd
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let pci = e.file_name().to_str()?.to_string();
            (hex(&dir, "class")? >> 16 == 0x03).then_some(())?;
            let (v, d) = (hex(&dir, "vendor")?, hex(&dir, "device")?);
            let vendor = match v {
                0x10de => Vendor::Nvidia,
                0x1002 => Vendor::Amd,
                0x8086 => Vendor::Intel,
                _ => return None,
            };
            let maker = match vendor {
                Vendor::Nvidia => "NVIDIA",
                Vendor::Amd => "AMD",
                Vendor::Intel => "Intel",
            };
            let model = pci_name(&ids, v, d).unwrap_or_else(|| format!("graphics {d:04x}"));
            // (behind the processor's own bridge: AMD's at 00:08, Intel's at 00:02.0 itself)
            let parent = std::fs::canonicalize(&dir).ok().and_then(|p| p.parent().and_then(|q| q.file_name()).map(|n| n.to_string_lossy().into_owned())).unwrap_or_default();
            let integrated = match vendor {
                Vendor::Amd => parent.ends_with(":00:08.1") || parent.ends_with(":00:08.2") || parent.ends_with(":00:08.3"),
                Vendor::Intel => pci.ends_with(":00:02.0"),
                Vendor::Nvidia => false,
            };
            Some(Card { pci, name: format!("{maker} {model}"), vendor, integrated, dir })
        })
        .collect();
    all.sort_by(|a, b| a.pci.cmp(&b.pci));
    all
}

/// A card's model, from the PCI ID list: the name in its brackets where it has one
/// ("GB202 [GeForce RTX 5090]" is GeForce RTX 5090).
fn pci_name(ids: &str, vendor: u32, device: u32) -> Option<String> {
    let (v, d) = (format!("{vendor:04x}"), format!("\t{device:04x}"));
    let mut lines = ids.lines().skip_while(|l| !l.starts_with(&v));
    lines.next()?;
    // (the vendor's devices run to the next vendor, comments among them)
    let line = lines.take_while(|l| !l.starts_with(|c: char| c.is_ascii_hexdigit())).find(|l| l.starts_with(&d))?;
    let name = line[d.len()..].trim();
    Some(match (name.find('['), name.rfind(']')) {
        (Some(a), Some(b)) if b > a => name[a + 1..b].to_string(),
        _ => name.to_string(),
    })
}

/// What a card's readings come from, for its note: everything but video memory from
/// Intel's driver, which doesn't report it.
pub fn reports(c: &Card) -> &'static str {
    match c.vendor {
        Vendor::Nvidia => "Temperature, load, clock and video memory, from NVIDIA’s driver.",
        Vendor::Amd => "Temperature, load, clock and video memory, from Linux’s amdgpu driver.",
        Vendor::Intel => "Temperature, load and clock, from Linux’s xe driver, which doesn’t report video memory.",
    }
}

/// An AMD or Intel card read from Linux's own files (an NVIDIA card is NVML's): its
/// temperature, load and clock, and its video memory (none from Intel's driver, given as
/// 0 of 0). Intel's load is worked out from how long it was idle since the last reading,
/// so the first reading's is 0.
#[derive(Default)]
pub struct Sysfs {
    idle: Option<(u64, Instant)>,
}

impl Sysfs {
    pub fn read(&mut self, c: &Card) -> Option<crate::lcd_stats::GpuReading> {
        let num = |f: &Path| std::fs::read_to_string(f).ok()?.trim().parse::<u64>().ok();
        let hw = std::fs::read_dir(c.dir.join("hwmon")).ok()?.flatten().map(|e| e.path()).next()?;
        let t = temps(&hw);
        let temp_of = |want: &str| t.iter().find(|(l, _)| l == want).or(t.first()).and_then(|(_, p)| num(p)).map(|v| (v / 1000) as u32);
        let gib = (1u64 << 30) as f64;
        match c.vendor {
            Vendor::Amd => {
                let temp = temp_of("edge")?;
                let load = num(&c.dir.join("gpu_busy_percent")).unwrap_or(0) as u32;
                let clock = num(&hw.join("freq1_input")).map_or(0, |hz| (hz / 1_000_000) as u32);
                let (used, total) = (num(&c.dir.join("mem_info_vram_used")).unwrap_or(0), num(&c.dir.join("mem_info_vram_total")).unwrap_or(0));
                Some((temp, load, clock, used as f64 / gib, total as f64 / gib))
            }
            Vendor::Intel => {
                let temp = temp_of("pkg")?;
                let gt = c.dir.join("tile0/gt0");
                let clock = num(&gt.join("freq0/act_freq")).unwrap_or(0) as u32;
                let idle = num(&gt.join("gtidle/idle_residency_ms"));
                let load = match (idle, self.idle) {
                    (Some(now), Some((was, at))) => {
                        let ms = at.elapsed().as_secs_f64() * 1000.0;
                        if ms > 0.0 { (100.0 * (1.0 - (now.saturating_sub(was)) as f64 / ms)).clamp(0.0, 100.0).round() as u32 } else { 0 }
                    }
                    _ => 0,
                };
                self.idle = idle.map(|v| (v, Instant::now()));
                Some((temp, load, clock, 0.0, 0.0))
            }
            Vendor::Nvidia => None,
        }
    }
}

// ------------------------------------------------------------------ the choices

/// The choices, as Ectodeck's settings keep them: the CPU sensor by its driver and label,
/// the clock as the average of all cores or the fastest core's, the card by its PCI address
/// and name. None is the default: what Ectodeck read before there was a choice.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choice {
    pub cpu: Option<(String, String)>,
    pub fastest: bool,
    pub card: Option<(String, String)>,
}

impl Choice {
    pub fn of(v: &Value) -> Choice {
        let pair = |o: &Value, a: &str, b: &str| Some((o[a].as_str()?.to_string(), o[b].as_str()?.to_string()));
        Choice { cpu: pair(&v["cpu_temp"], "driver", "label"), fastest: v["cpu_clock"].as_str() == Some("fastest"), card: pair(&v["card"], "pci", "name") }
    }

    pub fn to_json(&self) -> Value {
        let mut v = json!({ "cpu_clock": if self.fastest { "fastest" } else { "average" } });
        if let Some((d, l)) = &self.cpu {
            v["cpu_temp"] = json!({ "driver": d, "label": l });
        }
        if let Some((p, n)) = &self.card {
            v["card"] = json!({ "pci": p, "name": n });
        }
        v
    }

    /// The sensor the CPU temperature comes from: the one chosen, while it is there (one
    /// gone shows as unavailable until another is chosen); or by default Tctl, on Intel
    /// Package id 0, else the processor's first.
    pub fn cpu_sensor<'a>(&self, all: &'a [CpuSensor]) -> Option<&'a CpuSensor> {
        match &self.cpu {
            Some((d, l)) => all.iter().find(|s| s.driver == *d && s.label == *l),
            None => all.iter().find(|s| s.own && (s.label == "Tctl" || s.label == "Package id 0")).or(all.iter().find(|s| s.own)).or(all.first()),
        }
    }

    /// The card the GPU readings come from: the one chosen (by its address, or its name
    /// where its address changed), while it is there; or by default the first NVIDIA card,
    /// else the first other.
    pub fn card<'a>(&self, all: &'a [Card]) -> Option<&'a Card> {
        match &self.card {
            Some((p, n)) => all.iter().find(|c| c.pci == *p && c.name == *n).or(all.iter().find(|c| c.name == *n)),
            None => all.iter().find(|c| c.vendor == Vendor::Nvidia).or(all.first()),
        }
    }
}

static CHOICE: OnceLock<Mutex<Choice>> = OnceLock::new();

/// The choices every reader of the screen's readings goes by now.
pub fn choice() -> Choice {
    CHOICE.get_or_init(Default::default).lock().unwrap().clone()
}

/// The choices from now on, from Ectodeck's settings.
pub fn choose(c: Choice) {
    *CHOICE.get_or_init(Default::default).lock().unwrap() = c;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up computer: an AMD processor's k10temp, a motherboard chip, a drive, and two
    /// cards (an NVIDIA card, and the graphics in the processor).
    fn computer(dir: &Path) {
        let hw = |n: u32, name: &str, dev: &str, t: &[(&str, i64)]| {
            let h = dir.join(format!("sys/class/hwmon/hwmon{n}"));
            std::fs::create_dir_all(&h).unwrap();
            std::fs::write(h.join("name"), format!("{name}\n")).unwrap();
            let d = dir.join(dev);
            std::fs::create_dir_all(&d).unwrap();
            std::os::unix::fs::symlink(&d, h.join("device")).unwrap();
            for (k, (label, v)) in t.iter().enumerate() {
                std::fs::write(h.join(format!("temp{}_input", k + 1)), format!("{v}\n")).unwrap();
                std::fs::write(h.join(format!("temp{}_label", k + 1)), format!("{label}\n")).unwrap();
            }
        };
        hw(4, "k10temp", "sys/devices/pci0000:00/0000:00:18.3", &[("Tctl", 63000), ("Tccd1", 54000), ("Tccd2", 57000)]);
        hw(12, "nct6687", "sys/devices/platform/nct6687.2592", &[("CPU", 63500), ("System", 40000), ("CPU Socket", 46000)]);
        hw(0, "nvme", "sys/devices/pci0000:00/0000:00:01.2/0000:02:00.0/nvme/nvme0", &[("Composite", 48000)]);
        std::fs::create_dir_all(dir.join("proc")).unwrap();
        std::fs::write(dir.join("proc/cpuinfo"), "processor\t: 0\nmodel name\t: AMD Ryzen 9 9950X3D 16-Core Processor\ncpu cores\t: 16\n").unwrap();
        std::fs::create_dir_all(dir.join("sys/class/dmi/id")).unwrap();
        std::fs::write(dir.join("sys/class/dmi/id/board_vendor"), "Micro-Star International Co., Ltd.\n").unwrap();
        std::fs::write(dir.join("sys/class/dmi/id/board_name"), "MEG X670E ACE (MS-7D69)\n").unwrap();
        let card = |path: &str, v: &str, d: &str| {
            let real = dir.join("sys/devices").join(path);
            std::fs::create_dir_all(&real).unwrap();
            for (f, s) in [("class", "0x030000"), ("vendor", v), ("device", d)] {
                std::fs::write(real.join(f), format!("{s}\n")).unwrap();
            }
            let at = dir.join("sys/bus/pci/devices");
            std::fs::create_dir_all(&at).unwrap();
            std::os::unix::fs::symlink(&real, at.join(path.rsplit('/').next().unwrap())).unwrap();
        };
        card("pci0000:00/0000:00:01.1/0000:01:00.0", "0x10de", "0x2b85");
        card("pci0000:00/0000:00:08.1/0000:18:00.0", "0x1002", "0x13c0");
        std::fs::create_dir_all(dir.join("usr/share/hwdata")).unwrap();
        std::fs::write(dir.join("usr/share/hwdata/pci.ids"), "1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t13c0  Granite Ridge [Radeon Graphics]\n10de  NVIDIA Corporation\n# Overclocked\n\t2b85  GB202 [GeForce RTX 5090]\n").unwrap();
    }

    #[test]
    fn the_processor_s_sensors_its_own_first_then_the_motherboard_s() {
        let dir = std::env::temp_dir().join(format!("ectodeck-sensors-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        computer(&dir);
        let all = cpu_sensors(&dir);
        let names: Vec<(&str, &str, bool)> = all.iter().map(|s| (s.driver.as_str(), s.label.as_str(), s.own)).collect();
        assert_eq!(names, [("k10temp", "Tctl", true), ("k10temp", "Tccd1", true), ("k10temp", "Tccd2", true), ("nct6687", "CPU", false), ("nct6687", "CPU Socket", false)]);
        assert_eq!(all[3].read(), Some(63.5));
        assert_eq!((cpu_name(&dir), cpu_cores(&dir), board_name(&dir)), ("AMD Ryzen 9 9950X3D".into(), Some(16), Some("MSI MEG X670E ACE".into())));
        // by default Tctl; chosen, kept by driver and label; gone, none until another is chosen
        let mut c = Choice::default();
        assert_eq!(c.cpu_sensor(&all).map(|s| s.label.as_str()), Some("Tctl"));
        c.cpu = Some(("nct6687".into(), "CPU Socket".into()));
        assert_eq!(c.cpu_sensor(&all).map(|s| s.read()), Some(Some(46.0)));
        assert_eq!(Choice::of(&c.to_json()), c);
        c.cpu = Some(("it87".into(), "CPU".into()));
        assert!(c.cpu_sensor(&all).is_none());
        // the cards, named as people say them; the NVIDIA card by default
        let cards = cards(&dir);
        let shown: Vec<String> = cards.iter().map(|c| c.shown()).collect();
        assert_eq!(shown, ["NVIDIA GeForce RTX 5090", "AMD Radeon Graphics, in the processor"]);
        assert_eq!(Choice::default().card(&cards).map(|c| c.pci.as_str()), Some("0000:01:00.0"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
