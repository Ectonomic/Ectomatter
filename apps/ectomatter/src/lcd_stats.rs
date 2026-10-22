//! The Stats page: the computer's processor, graphics card and memory, in
//! the four rows Grant laid out for the z10-lcd service, read and written as
//! z10lcd.py reads and writes them, so the keyboard shows the same
//! characters; and every other reading a page shows of the computer. The
//! readings are the sensors checked in Settings (see [crate::sensors]): the
//! processor's own temperature (Tctl by default), its load and its average
//! clock, the memory in use, the main card's temperature, load, clock and
//! video memory, and each other sensor checked. A reading unchecked can't be
//! had, as one the computer doesn't have.
//!
//! Ectomatter never reads the sensors itself: the Sensors plugin lists them
//! and reads those it's asked to watch ([crate::hosted::Readings]), and this
//! asks for what it shows. Tests hand in a made-up computer, read here.

#[cfg(test)]
use std::path::{Path, PathBuf};
use std::time::Instant;

#[cfg(test)]
use ectomatter_sensors::list::Nvidia;
use ectomatter_sensors::list::{Catalog, Sensor, Val};

/// The Sensors plugin, by its id.
pub const SENSORS: &str = "sensors";

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

/// Where the readings come from: the Sensors plugin, as it tells them, for
/// one who shows them (the keyboard's screen, a page); or, in a test, a
/// made-up computer read here.
enum From {
    Plugin(crate::hosted::Readings, &'static str),
    #[cfg(test)]
    Here(PathBuf, ectomatter_sensors::read::Reader),
}

pub struct Stats {
    from: From,
    /// the computer's sensors, as last listed (looked at again every half minute)
    listed: Option<(Instant, Catalog)>,
    /// the choices read by, where they aren't Ectomatter's own (a test's)
    pub setup: Option<crate::sensors::Setup>,
}

impl Stats {
    /// The readings the Sensors plugin tells, for `who` (the screen, a page),
    /// which asks it to watch what it shows.
    pub fn of(readings: crate::hosted::Readings, who: &'static str) -> Stats {
        Stats { from: From::Plugin(readings, who), listed: None, setup: None }
    }

    /// A made-up computer under `root`, its NVIDIA cards as `nv` says, read here.
    #[cfg(test)]
    pub fn new(root: impl Into<PathBuf>, nv: Box<dyn Nvidia>) -> Stats {
        let root = root.into();
        Stats { from: From::Here(root.clone(), ectomatter_sensors::read::Reader::new(root, nv)), listed: None, setup: None }
    }

    /// The computer's sensors as last listed (again when half a minute old):
    /// as the Sensors plugin last told them, none while it isn't running.
    pub fn catalog(&mut self) -> &Catalog {
        if self.listed.as_ref().is_none_or(|(at, _)| at.elapsed().as_secs() >= 30) {
            let cat = match &mut self.from {
                From::Plugin(r, _) => r.of(SENSORS).and_then(|t| serde_json::from_value::<Catalog>(t.list).ok()).unwrap_or_default(),
                #[cfg(test)]
                From::Here(root, reader) => ectomatter_sensors::list::catalog(root, reader.nvidia()),
            };
            crate::sensors::remember(&cat, &self.setup.clone().unwrap_or_else(crate::sensors::setup));
            // (a list not yet told isn't kept: the next look asks again)
            let at = if cat.devices.is_empty() { Instant::now() - std::time::Duration::from_secs(30) } else { Instant::now() };
            self.listed = Some((at, cat));
        }
        &self.listed.as_ref().unwrap().1
    }

    /// A reading once, so the next one's load is what the processor did since (the first
    /// reading of a load is 0), as the service did before it showed a page.
    pub fn prime(&mut self) {
        self.read();
    }

    /// Each of these sensors read now: as the Sensors plugin last read it,
    /// these being the ones asked for from now on.
    fn values(&mut self, want: &[&Sensor]) -> std::collections::HashMap<String, Val> {
        match &mut self.from {
            From::Plugin(r, who) => {
                r.want(SENSORS, who, want.iter().map(|s| s.id.clone()).collect());
                let told = r.of(SENSORS).map(|t| t.values).unwrap_or_default();
                want.iter().filter_map(|s| Some((s.id.clone(), serde_json::from_value::<Val>(told.get(&s.id)?.clone()).ok()?))).collect()
            }
            #[cfg(test)]
            From::Here(_, reader) => reader.read(want),
        }
    }

    /// Every sensor on the computer read now, checked or not, by id, for the Sensors box.
    pub fn read_all(&mut self) -> std::collections::HashMap<String, Val> {
        let all: Vec<Sensor> = self.catalog().devices.iter().flat_map(|d| d.sensors.iter().cloned()).collect();
        self.values(&all.iter().collect::<Vec<_>>())
    }

    /// Nothing read for this one any longer (a page left).
    pub fn rest(&mut self) {
        if let From::Plugin(r, who) = &self.from {
            r.want(SENSORS, who, Vec::new());
        }
    }

    /// Everything the computer reads now, for a page (the time, the profile and the
    /// weather are the page's player's to add): every sensor checked, as Settings has it.
    pub fn read(&mut self) -> crate::lcd_page::Readings {
        let cat = self.catalog().clone();
        let setup = self.setup.clone().unwrap_or_else(crate::sensors::setup);
        let checked = setup.checked_in(&cat);
        let want: Vec<&Sensor> = cat.devices.iter().flat_map(|d| d.sensors.iter()).filter(|s| checked.contains(&s.id)).collect();
        let v = self.values(&want);
        let one = |id: &str| v.get(id).map(|x| x.one());
        let cpu_temp = cat.cpu_temp().and_then(|s| one(&s.id));
        let ram = match v.get("memory/used") {
            Some(Val::Of(u, t)) => Some((*u, *t)),
            _ => None,
        };
        let gpu = setup.main_card(&cat).map(|d| {
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
    use ectomatter_sensors::fake::{nvidia, temp_dir, FakeNvidia};
    use ectomatter_sensors::list::{Nv, Probe};

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
        std::fs::create_dir_all(dir.join("sys/bus/pci/drivers/nvidia")).unwrap();
        let _ = std::os::unix::fs::symlink(dir.join("sys/bus/pci/drivers/nvidia"), card.join("driver"));
        std::fs::create_dir_all(dir.join("usr/share/hwdata")).unwrap();
        std::fs::write(dir.join("usr/share/hwdata/pci.ids"), "10de  NVIDIA Corporation\n\t2b85  GB202 [GeForce RTX 5090]\n").unwrap();
    }

    #[test]
    fn the_rows_as_the_service_wrote_them() {
        let dir = temp_dir("stats");
        ectomatter_sensors::fake::computer(&dir);
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
        ectomatter_sensors::fake::computer(&dir);
        let mut s = Stats::new(&dir, Box::new(nvidia()));
        let cat = s.catalog().clone();
        let mut setup = crate::sensors::Setup::default();
        setup.check(&cat, "card@0000:01:00.0/temperature", false);
        setup.check(&cat, "nct6687@nct6687.2592/fan/Pump Fan", true);
        s.setup = Some(setup);
        let r = s.read();
        assert_eq!(r.gpu.map(|g| (g.temp, g.load)), Some((None, Some(6))));
        let pump = crate::sensors::number("nct6687@nct6687.2592/fan/Pump Fan");
        assert_eq!(r.sensors.get(&pump), Some(&(ectomatter_sensors::list::What::Fan, Val::One(2836.0))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
