//! Which of the computer's sensors (see [crate::sensor_list]) the screen's layouts can show,
//! as Ectomatter's settings keep it ("sensors"): the sensors checked in Settings, the names
//! given them, and the main card, the card the ready-made layouts' GPU readings come from.
//! Until a sensor is first checked or unchecked, the usual ones are: those the ready-made
//! layouts show. Every reader of the screen's readings goes by the same choices, and knows
//! each sensor a layout shows by a number of its own ([Source::Sensor]), as this keeps it.
//!
//! Here too: the processor's, the motherboard's and the graphics cards' names, found on
//! this computer under `root` (`/` but for tests).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

use crate::lcd_page::Source;
use crate::sensor_list::{Catalog, Device, Part, What};

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
pub fn short_vendor(v: &str) -> String {
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
}

/// The graphics cards Ectomatter can read, in their PCI addresses' order.
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
            Some(Card { pci, name: format!("{maker} {model}"), vendor, integrated })
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

// ------------------------------------------------------------------ the choices

/// Which sensors the screen's layouts can show.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Setup {
    /// the sensors checked, by id; None until one is first checked or unchecked, when the
    /// usual ones are, and those carried over from the Sensors card before (`also`)
    pub checked: Option<BTreeSet<String>>,
    pub also: BTreeSet<String>,
    /// the names given in Settings, by id
    pub names: BTreeMap<String, String>,
    /// the card made main (or chosen on the Sensors card before), by its PCI address and
    /// name; while it isn't in the computer, or none was made main, the card by the rule
    pub main: Option<(String, String)>,
    /// the devices listed so far, so a card that comes in as the main card has its usual
    /// sensors checked
    pub seen: BTreeSet<String>,
}

/// The Sensors card's choices before there was a library: the CPU sensor by its driver and
/// label, whether the clock was the fastest core's, and the card by its PCI address and name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Before {
    pub cpu: Option<(String, String)>,
    pub fastest: bool,
    pub card: Option<(String, String)>,
}

impl Setup {
    /// The choices as the settings keep them, and the Sensors card's choices where they are
    /// still kept as it kept them.
    pub fn of(v: &Value) -> (Setup, Option<Before>) {
        let ids = |v: &Value| -> BTreeSet<String> { v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default() };
        let pair = |o: &Value, a: &str, b: &str| Some((o[a].as_str()?.to_string(), o[b].as_str()?.to_string()));
        // (the Sensors card always kept how the clock was counted)
        if v.get("cpu_clock").is_some() {
            let before = Before { cpu: pair(&v["cpu_temp"], "driver", "label"), fastest: v["cpu_clock"].as_str() == Some("fastest"), card: pair(&v["card"], "pci", "name") };
            return (Setup::default(), Some(before));
        }
        let names = v["names"].as_object().map(|m| m.iter().filter_map(|(k, n)| Some((k.clone(), n.as_str()?.to_string()))).collect()).unwrap_or_default();
        let setup = Setup { checked: v["checked"].is_array().then(|| ids(&v["checked"])), also: ids(&v["also"]), names, main: pair(&v["main"], "pci", "name"), seen: ids(&v["seen"]) };
        (setup, None)
    }

    pub fn to_json(&self) -> Value {
        let mut v = json!({ "names": self.names, "seen": self.seen });
        if let Some(c) = &self.checked {
            v["checked"] = json!(c);
        }
        if !self.also.is_empty() {
            v["also"] = json!(self.also);
        }
        if let Some((p, n)) = &self.main {
            v["main"] = json!({ "pci": p, "name": n });
        }
        v
    }

    /// The Sensors card's choices carried over, on a computer listed as `cat`: the CPU
    /// sensor chosen and the clock chosen are checked, and the card chosen is the main card.
    /// What the layouts showing the CPU temperature or clock read from where those weren't
    /// the usual ones, to show the same: each with the sensor it now shows.
    pub fn carry(&mut self, b: &Before, cat: &Catalog) -> Vec<(Source, String)> {
        let mut shows = Vec::new();
        if let Some((driver, label)) = &b.cpu {
            let id = cat.devices.iter().flat_map(|d| d.sensors.iter()).find(|s| s.id.starts_with(&format!("{driver}@")) && s.id.ends_with(&format!("/temp/{label}"))).map(|s| s.id.clone());
            if let Some(id) = id.filter(|id| cat.cpu_temp().is_none_or(|u| u.id != *id)) {
                self.also.insert(id.clone());
                shows.push((Source::CpuTemp, id));
            }
        }
        if b.fastest {
            self.also.insert("cpu/clock/fastest".into());
            shows.push((Source::CpuClock, "cpu/clock/fastest".into()));
        }
        self.main = b.card.clone();
        shows
    }

    /// The main card on a computer listed as `cat`: the one made main while it is in it (by its
    /// address, or by its name where its address changed), else the card by the rule.
    pub fn main_card<'a>(&self, cat: &'a Catalog) -> Option<&'a Device> {
        let cards = cat.cards();
        // (by its place first: a card's name changes when Linux's list of devices is updated)
        let made = self.main.as_ref().and_then(|(p, n)| {
            let at = |d: &&&Device| d.card.as_ref().is_some_and(|c| c.pci == *p);
            let named = |d: &&&Device| d.card.as_ref().is_some_and(|c| c.name == *n);
            cards.iter().find(at).or(cards.iter().find(named)).copied()
        });
        made.or(cat.main_by_rule())
    }

    /// The sensors the ready-made layouts show, on a computer listed as `cat`: the
    /// processor's temperature, load and average clock, the memory in use, and the main
    /// card's temperature, load, clock and video memory.
    pub fn usual(&self, cat: &Catalog) -> Vec<String> {
        let mut all: Vec<String> = cat.devices.iter().filter(|d| d.card.is_none()).flat_map(|d| d.sensors.iter()).filter(|s| s.usual).map(|s| s.id.clone()).collect();
        if let Some(m) = self.main_card(cat) {
            all.extend(m.sensors.iter().filter(|s| s.usual).map(|s| s.id.clone()));
        }
        all
    }

    /// The sensors checked now, on a computer listed as `cat`.
    pub fn checked_in(&self, cat: &Catalog) -> BTreeSet<String> {
        match &self.checked {
            Some(c) => c.clone(),
            None => self.usual(cat).into_iter().chain(self.also.iter().cloned()).collect(),
        }
    }

    /// A computer listed: a card that has come in as the main card has its usual sensors
    /// checked, and each device is seen. Whether that changed anything to keep.
    pub fn settle(&mut self, cat: &Catalog) -> bool {
        let mut changed = false;
        if let Some(m) = self.main_card(cat).filter(|m| !self.seen.contains(&m.key)) {
            let usual: Vec<String> = m.sensors.iter().filter(|s| s.usual).map(|s| s.id.clone()).collect();
            if let Some(c) = &mut self.checked {
                c.extend(usual);
                changed = true;
            }
        }
        for d in &cat.devices {
            changed |= self.seen.insert(d.key.clone());
        }
        changed
    }

    /// A sensor checked or unchecked in Settings.
    pub fn check(&mut self, cat: &Catalog, id: &str, on: bool) {
        let mut c = self.checked_in(cat);
        if on {
            c.insert(id.to_string());
        } else {
            c.remove(id);
        }
        self.checked = Some(c);
    }

    /// A card made the main card, its usual sensors checked (the card main before keeps its own).
    pub fn make_main(&mut self, cat: &Catalog, key: &str) {
        let Some(d) = cat.device(key) else { return };
        let Some(c) = &d.card else { return };
        let mut checked = self.checked_in(cat);
        checked.extend(d.sensors.iter().filter(|s| s.usual).map(|s| s.id.clone()));
        self.checked = Some(checked);
        self.main = Some((c.pci.clone(), c.name.clone()));
    }

    /// A sensor's name given in Settings: none (an empty name, or the sensor's own) takes it back.
    pub fn rename(&mut self, cat: &Catalog, id: &str, name: &str) {
        let name = name.trim();
        let own = cat.sensor(id).map(|(_, s)| s.name.as_str());
        if name.is_empty() || Some(name) == own {
            self.names.remove(id);
        } else {
            self.names.insert(id.to_string(), name.to_string());
        }
    }
}

static SETUP: OnceLock<Mutex<Setup>> = OnceLock::new();

/// The choices every reader of the screen's readings goes by now.
pub fn setup() -> Setup {
    SETUP.get_or_init(Default::default).lock().unwrap().clone()
}

/// The choices from now on.
pub fn set_up(s: Setup) {
    *SETUP.get_or_init(Default::default).lock().unwrap() = s;
}

// ------------------------------------------------------------------ the sensors layouts show

/// What a layout knows of a sensor it shows, from the last list of the computer's sensors:
/// what it reads, the part it is in, and its name as the editor says it.
#[derive(Clone, Debug, PartialEq)]
pub struct Known {
    pub what: What,
    pub part: Part,
    pub name: String,
    /// its device's name, and its device's key
    pub device: String,
    pub key: String,
}

#[derive(Default)]
struct Book {
    ids: Vec<String>,
    known: HashMap<u16, Known>,
}

static BOOK: OnceLock<Mutex<Book>> = OnceLock::new();

fn book() -> std::sync::MutexGuard<'static, Book> {
    BOOK.get_or_init(Default::default).lock().unwrap()
}

/// The number a layout knows a sensor by, for as long as Ectomatter runs.
pub fn number(id: &str) -> u16 {
    let mut b = book();
    match b.ids.iter().position(|x| x == id) {
        Some(i) => i as u16,
        None => {
            b.ids.push(id.to_string());
            (b.ids.len() - 1) as u16
        }
    }
}

/// The sensor a layout knows by this number.
pub fn id_of(n: u16) -> String {
    book().ids.get(n as usize).cloned().unwrap_or_default()
}

/// What is known of the sensor a layout knows by this number (nothing for a sensor not on
/// this computer, or before its sensors are first listed).
pub fn known(n: u16) -> Option<Known> {
    book().known.get(&n).cloned()
}

/// The computer's sensors listed: each one's name, as given or its own, and what it reads.
pub fn remember(cat: &Catalog, setup: &Setup) {
    for d in &cat.devices {
        for s in &d.sensors {
            let n = number(&s.id);
            let k = Known { what: s.what, part: d.part, name: s.menu_name(setup.names.get(&s.id).map(String::as_str)), device: d.name.clone(), key: d.key.clone() };
            book().known.insert(n, k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensor_list::tests::{computer, nvidia, temp_dir};
    use crate::sensor_list::catalog;

    #[test]
    fn the_usual_sensors_checked_until_one_is_and_the_main_card_by_the_rule() {
        let dir = temp_dir("setup");
        computer(&dir);
        let cat = catalog(&dir, &mut nvidia());
        let (mut s, before) = Setup::of(&Value::Null);
        assert!(before.is_none());
        let usual: Vec<&str> = ["k10temp@0000:00:18.3/temp/Tctl", "cpu/load", "cpu/clock/average", "memory/used", "card@0000:01:00.0/temperature", "card@0000:01:00.0/load", "card@0000:01:00.0/clock", "card@0000:01:00.0/video-memory"].into();
        assert_eq!(s.checked_in(&cat), usual.iter().map(|x| x.to_string()).collect());
        assert_eq!(s.main_card(&cat).map(|d| d.name.as_str()), Some("RTX 5090"));
        // a sensor checked: from then on, the ones checked
        s.check(&cat, "nct6687@nct6687.2592/fan/Pump Fan", true);
        s.rename(&cat, "nct6687@nct6687.2592/fan/Pump Fan", " Radiator pump ");
        assert!(s.checked_in(&cat).contains("nct6687@nct6687.2592/fan/Pump Fan"));
        assert_eq!(s.names.get("nct6687@nct6687.2592/fan/Pump Fan").map(String::as_str), Some("Radiator pump"));
        // the Arc made main: its usual sensors checked, the RTX 5090 keeps its own
        s.make_main(&cat, "card@0000:16:00.0");
        let c = s.checked_in(&cat);
        assert!(c.contains("card@0000:16:00.0/temperature") && c.contains("card@0000:16:00.0/load") && c.contains("card@0000:01:00.0/temperature"));
        assert_eq!(s.main_card(&cat).map(|d| d.name.as_str()), Some("Arc Pro B50"));
        // still the main card when Linux's list of devices names it anew
        let kept = s.main.clone();
        s.main = Some(("0000:16:00.0".into(), "Intel Battlemage G21".into()));
        assert_eq!(s.main_card(&cat).map(|d| d.name.as_str()), Some("Arc Pro B50"));
        s.main = kept;
        // kept and read back as it was
        assert_eq!(Setup::of(&s.to_json()), (s.clone(), None));
        // a name taken back by giving it nothing
        s.rename(&cat, "nct6687@nct6687.2592/fan/Pump Fan", "");
        assert!(s.names.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_card_that_comes_in_as_the_main_card_has_its_usual_sensors_checked() {
        let dir = temp_dir("setup-new-card");
        computer(&dir);
        // the computer before the RTX 5090 was in it: the Arc is the main card
        std::fs::remove_file(dir.join("sys/bus/pci/devices/0000:01:00.0")).unwrap();
        let before = catalog(&dir, &mut nvidia());
        let mut s = Setup::default();
        s.settle(&before);
        s.check(&before, "nct6687@nct6687.2592/fan/Pump Fan", true);
        assert_eq!(s.main_card(&before).map(|d| d.name.as_str()), Some("Arc Pro B50"));
        // then the RTX 5090 put in: main by the rule, its usual sensors checked; nothing else new checked
        std::os::unix::fs::symlink(dir.join("sys/devices/pci0000:00/0000:00:01.1/0000:01:00.0"), dir.join("sys/bus/pci/devices/0000:01:00.0")).unwrap();
        let after = catalog(&dir, &mut nvidia());
        assert!(s.settle(&after));
        let c = s.checked_in(&after);
        assert!(c.contains("card@0000:01:00.0/temperature") && c.contains("card@0000:01:00.0/video-memory"));
        assert!(!c.contains("card@0000:01:00.0/power"));
        // and a second look changes nothing
        assert!(!s.settle(&after));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sensors_card_s_choices_carried_over() {
        let dir = temp_dir("setup-before");
        computer(&dir);
        let cat = catalog(&dir, &mut nvidia());
        let old = json!({ "cpu_temp": { "driver": "nct6687", "label": "CPU" }, "cpu_clock": "fastest", "card": { "pci": "0000:18:00.0", "name": "AMD Radeon Graphics" } });
        let (mut s, before) = Setup::of(&old);
        let shows = s.carry(&before.unwrap(), &cat);
        assert_eq!(shows, [(Source::CpuTemp, "nct6687@nct6687.2592/temp/CPU".to_string()), (Source::CpuClock, "cpu/clock/fastest".to_string())]);
        let c = s.checked_in(&cat);
        assert!(c.contains("nct6687@nct6687.2592/temp/CPU") && c.contains("cpu/clock/fastest") && c.contains("k10temp@0000:00:18.3/temp/Tctl"));
        assert_eq!(s.main_card(&cat).map(|d| d.name.as_str()), Some("Radeon Graphics"));
        // Tctl and the average, as they were by default: nothing for a layout to change
        let (mut s, before) = Setup::of(&json!({ "cpu_temp": { "driver": "k10temp", "label": "Tctl" }, "cpu_clock": "average" }));
        assert!(s.carry(&before.unwrap(), &cat).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_layout_knows_a_sensor_by_its_number() {
        let dir = temp_dir("setup-known");
        computer(&dir);
        let cat = catalog(&dir, &mut nvidia());
        let n = number("k10temp@0000:00:18.3/temp/Tccd1");
        assert_eq!(number("k10temp@0000:00:18.3/temp/Tccd1"), n);
        assert_eq!(id_of(n), "k10temp@0000:00:18.3/temp/Tccd1");
        let mut s = Setup::default();
        s.names.insert("k10temp@0000:00:18.3/temp/Tccd1".into(), "Hot die".into());
        remember(&cat, &s);
        let k = known(n).unwrap();
        assert_eq!((k.name.as_str(), k.what, k.part, k.device.as_str()), ("Hot die", What::Temperature, Part::Processor, "AMD Ryzen 9 9950X3D"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
