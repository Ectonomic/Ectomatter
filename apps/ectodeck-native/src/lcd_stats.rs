//! The Stats page: the computer's processor, graphics card and memory, in
//! the four rows Grant laid out for the z10-lcd service, read and written as
//! z10lcd.py reads and writes them, so the keyboard shows the same
//! characters; and every other reading a page shows of the computer. The
//! readings are the sensors checked in Settings (see [crate::sensors]): the
//! processor's own temperature (Tctl by default), its load and its average
//! clock, the memory in use, the main card's temperature, load, clock and
//! video memory, and each other sensor checked, each read as
//! [crate::sensor_read] reads it. A reading unchecked can't be had, as one the
//! computer doesn't have. Everything is read under `root`, which is `/` but
//! for tests, which hand in a made-up computer.

use std::path::PathBuf;
#[cfg(test)]
use std::path::Path;
use std::time::Instant;

use crate::sensor_list::{Catalog, Nvidia, Sensor, Val};

/// One reading of the main card: its temperature, load, core clock, and its video memory
/// used and in all (GiB), each where its driver reports it and it is checked.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gpu {
    pub temp: Option<u32>,
    pub load: Option<u32>,
    pub clock: Option<u32>,
    pub vram: Option<(f64, f64)>,
}

#[cfg(test)]
impl Gpu {
    /// A card reporting all four (none of its video memory where it says 0 of 0).
    pub fn of(temp: u32, load: u32, clock: u32, used: f64, total: f64) -> Gpu {
        Gpu { temp: Some(temp), load: Some(load), clock: Some(clock), vram: (total > 0.0).then_some((used, total)) }
    }
}

pub struct Stats {
    root: PathBuf,
    reader: crate::sensor_read::Reader,
    /// the computer's sensors, listed again every half minute
    listed: Option<(Instant, Catalog)>,
    /// the choices read by, where they aren't Ectodeck's own (a test's)
    pub setup: Option<crate::sensors::Setup>,
}

impl Stats {
    pub fn new(root: impl Into<PathBuf>, nv: Box<dyn Nvidia>) -> Stats {
        let root = root.into();
        Stats { reader: crate::sensor_read::Reader::new(root.clone(), nv), root, listed: None, setup: None }
    }

    /// This computer, its NVIDIA cards read through NVIDIA's driver.
    pub fn here() -> Stats {
        Stats::new("/", Box::new(crate::sensor_read::Nvml::default()))
    }

    /// The computer's sensors as last listed (again when half a minute old).
    pub fn catalog(&mut self) -> &Catalog {
        if self.listed.as_ref().is_none_or(|(at, _)| at.elapsed().as_secs() >= 30) {
            let cat = crate::sensor_list::catalog(&self.root, self.reader.nvidia());
            crate::sensors::remember(&cat, &self.setup.clone().unwrap_or_else(crate::sensors::setup));
            self.listed = Some((Instant::now(), cat));
        }
        &self.listed.as_ref().unwrap().1
    }

    /// A reading once, so the next one's load is what the processor did since (the first
    /// reading of a load is 0), as the service did before it showed a page.
    pub fn prime(&mut self) {
        self.read();
    }

    /// Every sensor on the computer read now, checked or not, by id, for the Sensors box.
    pub fn read_all(&mut self) -> std::collections::HashMap<String, Val> {
        self.catalog();
        let cat = &self.listed.as_ref().unwrap().1;
        let all: Vec<&Sensor> = cat.devices.iter().flat_map(|d| d.sensors.iter()).collect();
        self.reader.read(&all)
    }

    /// Everything the computer reads now, for a page (the time, the profile and the
    /// weather are the page's player's to add): every sensor checked, as Settings has it.
    pub fn read(&mut self) -> crate::lcd_page::Readings {
        self.catalog();
        let setup = self.setup.clone().unwrap_or_else(crate::sensors::setup);
        let cat = &self.listed.as_ref().unwrap().1;
        let checked = setup.checked_in(cat);
        let want: Vec<&Sensor> = cat.devices.iter().flat_map(|d| d.sensors.iter()).filter(|s| checked.contains(&s.id)).collect();
        let v = self.reader.read(&want);
        let one = |id: &str| v.get(id).map(|x| x.one());
        let cpu_temp = cat.cpu_temp().and_then(|s| one(&s.id));
        let ram = match v.get("memory/used") {
            Some(Val::Of(u, t)) => Some((*u, *t)),
            _ => None,
        };
        let gpu = setup.main_card(cat).map(|d| {
            let at = |k: &str| v.get(&format!("{}/{k}", d.key)).copied();
            let n = |k: &str| at(k).map(|x| x.one().round().max(0.0) as u32);
            let vram = match at("video-memory") {
                Some(Val::Of(u, t)) if t > 0.0 => Some((u, t)),
                _ => None,
            };
            Gpu { temp: n("temperature"), load: n("load"), clock: n("clock"), vram }
        });
        let sensors = want.iter().filter_map(|s| Some((crate::sensors::number(&s.id), (s.what, *v.get(&s.id)?)))).collect();
        crate::lcd_page::Readings { cpu_temp, cpu_load: one("cpu/load"), cpu_ghz: one("cpu/clock/average").map(|mhz| mhz / 1000.0), ram, gpu, sensors, ..Default::default() }
    }

    /// The Stats page's four rows, as the service wrote them. A reading that
    /// can't be had says so in its row.
    pub fn rows(&mut self) -> Vec<String> {
        let r = self.read();
        crate::lcd_page::render(&crate::lcd_page::Page::stats(), &r)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::sensor_list::tests::{nvidia, temp_dir, FakeNvidia};
    use crate::sensor_list::{Nv, Probe};

    /// NVIDIA's driver with one card, at 0000:01:00.0, reading as given: its temperature,
    /// load, clock, and video memory used and in all (none in all: none reported); or not
    /// loaded.
    pub struct FakeGpu(pub Vec<Result<(u32, u32, u32, f64, f64), String>>);

    impl Nvidia for FakeGpu {
        fn probe(&mut self, pci: &str) -> Option<Probe> {
            (pci == "0000:01:00.0" && self.0.first()?.is_ok()).then(|| Probe { has: vec![Nv::Temp, Nv::Load, Nv::Clock, Nv::Vram], vram: 0.0 })
        }
        fn read(&mut self, pci: &str, what: Nv) -> Option<Val> {
            let (t, l, c, u, all) = self.0.first()?.clone().ok().filter(|_| pci == "0000:01:00.0")?;
            Some(match what {
                Nv::Temp => Val::One(t as f64),
                Nv::Load => Val::One(l as f64),
                Nv::Clock => Val::One(c as f64),
                Nv::Vram => Val::Of(u, all),
                _ => return None,
            })
        }
    }

    /// A made-up computer as the z10-lcd service read one: k10temp's temperature (its only
    /// one, unlabeled), its threads' clocks, /proc/stat and /proc/meminfo, and an NVIDIA card
    /// (read through [FakeGpu]). Made again over itself, its files are written anew.
    pub fn computer(dir: &Path, temp: i64, freqs: &[u64], stat: [u64; 8], mem: (u64, u64)) {
        let h = dir.join("sys/class/hwmon/hwmon3");
        std::fs::create_dir_all(&h).unwrap();
        std::fs::write(h.join("name"), "k10temp\n").unwrap();
        std::fs::write(h.join("temp1_input"), format!("{temp}\n")).unwrap();
        let dev = dir.join("sys/devices/pci0000:00/0000:00:18.3");
        std::fs::create_dir_all(&dev).unwrap();
        let _ = std::os::unix::fs::symlink(&dev, h.join("device"));
        let other = dir.join("sys/class/hwmon/hwmon0");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("name"), "nvme\n").unwrap();
        for (i, f) in freqs.iter().enumerate() {
            let c = dir.join(format!("sys/devices/system/cpu/cpu{i}/cpufreq"));
            std::fs::create_dir_all(&c).unwrap();
            std::fs::write(c.join("scaling_cur_freq"), format!("{f}\n")).unwrap();
        }
        std::fs::create_dir_all(dir.join("sys/devices/system/cpu/cpufreq")).unwrap();
        std::fs::create_dir_all(dir.join("proc")).unwrap();
        let s: Vec<String> = stat.iter().map(|v| v.to_string()).collect();
        std::fs::write(dir.join("proc/stat"), format!("cpu  {}\ncpu0 1 2 3 4\n", s.join(" "))).unwrap();
        std::fs::write(dir.join("proc/meminfo"), format!("MemTotal:       {} kB\nMemFree:        1 kB\nMemAvailable:   {} kB\n", mem.0, mem.1)).unwrap();
        let card = dir.join("sys/devices/pci0000:00/0000:00:01.1/0000:01:00.0");
        std::fs::create_dir_all(&card).unwrap();
        for (f, v) in [("class", "0x030000"), ("vendor", "0x10de"), ("device", "0x2b85")] {
            std::fs::write(card.join(f), format!("{v}\n")).unwrap();
        }
        std::fs::create_dir_all(dir.join("sys/bus/pci/devices")).unwrap();
        let _ = std::os::unix::fs::symlink(&card, dir.join("sys/bus/pci/devices/0000:01:00.0"));
        std::fs::create_dir_all(dir.join("usr/share/hwdata")).unwrap();
        std::fs::write(dir.join("usr/share/hwdata/pci.ids"), "10de  NVIDIA Corporation\n\t2b85  GB202 [GeForce RTX 5090]\n").unwrap();
    }

    #[test]
    fn the_rows_as_the_service_wrote_them() {
        let dir = temp_dir("stats");
        crate::sensor_list::tests::computer(&dir);
        std::fs::write(dir.join("proc/stat"), "cpu  100 0 50 800 50 0 0 0 0 0\n").unwrap();
        let mut s = Stats::new(&dir, Box::new(nvidia()));
        s.setup = Some(Default::default());
        s.prime();
        std::fs::write(dir.join("proc/stat"), "cpu  150 0 100 850 50 0 0 0 0 0\n").unwrap();
        // (the made-up computer's four cores at 5.4 to 5.7 GHz, their second threads asleep at 624 MHz)
        assert_eq!(s.rows(), vec!["CPU 66°  67% 3087MHz", "GPU 43°   6% 1882MHz", "RAM   33.4 / 60.2 GB", "VRAM   4.8 / 31.8 GB"]);
        // NVIDIA's driver not loaded: the main card can't be read, and its row says so
        let mut s = Stats::new(&dir, Box::new(FakeNvidia(Vec::new())));
        s.setup = Some(Default::default());
        assert_eq!(s.rows()[1..], ["GPU unavailable".to_string(), "RAM   33.4 / 60.2 GB".into(), String::new()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reading_unchecked_is_unavailable_and_a_sensor_checked_is_read() {
        let dir = temp_dir("stats-checked");
        crate::sensor_list::tests::computer(&dir);
        let mut s = Stats::new(&dir, Box::new(nvidia()));
        let cat = s.catalog().clone();
        let mut setup = crate::sensors::Setup::default();
        setup.check(&cat, "card@0000:01:00.0/temperature", false);
        setup.check(&cat, "nct6687@nct6687.2592/fan/Pump Fan", true);
        s.setup = Some(setup);
        let r = s.read();
        assert_eq!(r.gpu.map(|g| (g.temp, g.load)), Some((None, Some(6))));
        let pump = crate::sensors::number("nct6687@nct6687.2592/fan/Pump Fan");
        assert_eq!(r.sensors.get(&pump), Some(&(crate::sensor_list::What::Fan, Val::One(2836.0))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
