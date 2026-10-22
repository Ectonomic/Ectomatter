//! Reading the sensors of [crate::list] now: a number for each (a share of a whole for
//! memory in use), and each written as the Sensors box and the screen editor's menu show it
//! ("66°", "4.7 GHz", "2,836 RPM") and as the keyboard's screen shows it ("66°", "2836RPM").
//!
//! A load is worked out from the time counted since the last reading, and a power from the
//! energy counted since, so the first reading of each is 0, as the processor's load always
//! was on the keyboard's screen. An NVIDIA card is read through NVIDIA's driver, as the
//! [Nvidia](crate::list::Nvidia) the reader is handed reaches it (in the Sensors plugin,
//! through Ectomatter, which alone may load the driver).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::files;
use crate::list::{How, Nvidia, Part, Sensor, Val, What};

fn num(p: &Path) -> Option<f64> {
    files::read_to_string(p).ok()?.trim().parse::<f64>().ok()
}

/// What reads the sensors, with the counts it read last time.
pub struct Reader {
    root: PathBuf,
    nv: Box<dyn Nvidia>,
    /// counts read last time (an energy's microjoules, an Intel card's sleep), by file: the
    /// count, when, and whether it has moved since it was first read
    counts: HashMap<PathBuf, (f64, Instant, bool)>,
    /// each thread's idle and whole time at the last reading of /proc/stat (all of them as -1)
    stat: HashMap<i64, (u64, u64)>,
}

impl Reader {
    #[cfg(any(test, feature = "read"))]
    pub fn new(root: impl Into<PathBuf>, nv: Box<dyn Nvidia>) -> Reader {
        Reader { root: root.into(), nv, counts: HashMap::new(), stat: HashMap::new() }
    }

    pub fn nvidia(&mut self) -> &mut dyn Nvidia {
        self.nv.as_mut()
    }

    /// Each thread's share of time busy since the last reading, and every thread's (at -1),
    /// in percent; 0 the first time.
    fn loads(&mut self) -> HashMap<i64, f64> {
        let mut out = HashMap::new();
        let Ok(s) = files::read_to_string(self.root.join("proc/stat")) else { return out };
        for line in s.lines().filter(|l| l.starts_with("cpu")) {
            let mut f = line.split_whitespace();
            let name = f.next().unwrap_or("");
            let Ok(n) = (if name == "cpu" { Ok(-1) } else { name[3..].parse::<i64>() }) else { continue };
            // user, nice, system, idle, iowait, irq, softirq and steal (a guest's time is in user's)
            let v: Vec<u64> = f.take(8).filter_map(|x| x.parse().ok()).collect();
            if v.len() < 5 {
                continue;
            }
            let (idle, total) = (v[3] + v[4], v.iter().sum::<u64>());
            let share = match self.stat.insert(n, (idle, total)) {
                Some((pi, pt)) if total > pt => 100.0 * (1.0 - (idle.saturating_sub(pi)) as f64 / (total - pt) as f64),
                _ => 0.0,
            };
            out.insert(n, share.clamp(0.0, 100.0));
        }
        out
    }

    /// Each thread's clock now, MHz.
    fn clocks(&self) -> HashMap<usize, f64> {
        let mut out = HashMap::new();
        let Ok(rd) = files::read_dir(self.root.join("sys/devices/system/cpu")) else { return out };
        for e in rd.flatten() {
            if let Some(n) = e.file_name().to_str().and_then(|n| n.strip_prefix("cpu")).and_then(|n| n.parse::<usize>().ok()) {
                if let Some(khz) = num(&e.path().join("cpufreq/scaling_cur_freq")) {
                    out.insert(n, khz / 1000.0);
                }
            }
        }
        out
    }

    /// How much a count has grown each second since it was last read: (per second, whether
    /// the count has ever moved). None the first time it is read.
    fn rate(&mut self, p: &Path) -> Option<(f64, bool)> {
        let v = num(p)?;
        let now = Instant::now();
        let was = self.counts.insert(p.to_path_buf(), (v, now, false));
        let (pv, at, moved) = was?;
        let moved = moved || v != pv;
        self.counts.get_mut(p).unwrap().2 = moved;
        let secs = now.duration_since(at).as_secs_f64();
        Some((if secs > 0.0 { (v - pv).max(0.0) / secs } else { 0.0 }, moved))
    }

    /// These sensors read now, by id; one that can't be read is left out.
    pub fn read(&mut self, want: &[&Sensor]) -> HashMap<String, Val> {
        let loads = if want.iter().any(|s| matches!(s.how, How::Load(_))) { self.loads() } else { HashMap::new() };
        let clocks = if want.iter().any(|s| matches!(s.how, How::Clock(..))) { self.clocks() } else { HashMap::new() };
        let gib = (1u64 << 30) as f64;
        let mut out = HashMap::new();
        for s in want {
            let v = match &s.how {
                How::File(p, scale) => num(p).map(|v| Val::One(v / scale)),
                How::Energy(p) => match self.rate(p) {
                    Some((uj, _)) => Some(Val::One(uj / 1e6)),
                    None => Some(Val::One(0.0)),
                },
                How::Asleep(p) => match self.rate(p) {
                    // (ms asleep each second: a count that never moved is one Linux keeps still, not a card never asleep)
                    Some((ms, moved)) => Some(Val::One(if moved { (100.0 - ms / 10.0).clamp(0.0, 100.0) } else { 0.0 })),
                    None => Some(Val::One(0.0)),
                },
                How::Load(ts) if ts.is_empty() => loads.get(&-1).map(|v| Val::One(*v)),
                How::Load(ts) => {
                    let got: Vec<f64> = ts.iter().filter_map(|t| loads.get(&(*t as i64))).copied().collect();
                    (!got.is_empty()).then(|| Val::One(got.iter().sum::<f64>() / got.len() as f64))
                }
                How::Clock(ts, fastest) => {
                    let got: Vec<f64> = ts.iter().filter_map(|t| clocks.get(t)).copied().collect();
                    match (got.is_empty(), fastest) {
                        (true, _) => None,
                        (false, true) => got.iter().copied().reduce(f64::max).map(Val::One),
                        (false, false) => Some(Val::One(got.iter().sum::<f64>() / got.len() as f64)),
                    }
                }
                How::Memory => {
                    let s = files::read_to_string(self.root.join("proc/meminfo")).unwrap_or_default();
                    let field = |k: &str| s.lines().find_map(|l| l.split_once(':').filter(|(n, _)| *n == k).and_then(|(_, v)| v.split_whitespace().next()?.parse::<f64>().ok()));
                    match (field("MemTotal"), field("MemAvailable")) {
                        (Some(t), Some(a)) => Some(Val::Of((t - a) * 1024.0 / gib, t * 1024.0 / gib)),
                        _ => None,
                    }
                }
                How::Nvml(pci, what) => self.nv.read(pci, *what),
                How::Busy(p) | How::Charge(p) => num(p).map(Val::One),
                How::Vram(u, t) => match (num(u), num(t)) {
                    (Some(u), Some(t)) if t > 0.0 => Some(Val::Of(u / gib, t / gib)),
                    _ => None,
                },
                How::Out(dir) => {
                    let out = files::read_to_string(dir.join("status")).is_ok_and(|s| s.trim() == "Discharging");
                    let w = num(&dir.join("power_now")).map(|uw| uw / 1e6).or_else(|| Some(num(&dir.join("current_now"))? * num(&dir.join("voltage_now"))? / 1e12));
                    w.map(|w| Val::One(if out { w } else { 0.0 }))
                }
            };
            if let Some(v) = v {
                out.insert(s.id.clone(), v);
            }
        }
        out
    }
}

/// A number with its thousands set apart: 2,836.
fn thousands(v: f64) -> String {
    let s = format!("{:.0}", v.abs());
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if v < -0.5 { format!("-{out}") } else { out }
}

/// Memory in use as a person reads it: "33.4 / 60.2 GB", or for a little of it, "72 MB / 2 GB".
fn in_use(u: f64, t: f64) -> String {
    if t < 4.0 && u < 1.0 {
        format!("{:.0} MB / {} GB", u * 1024.0, if t.fract().abs() < 0.05 { format!("{t:.0}") } else { format!("{t:.1}") })
    } else {
        format!("{u:.1} / {t:.1} GB")
    }
}

/// A reading as the Sensors box and the editor's menu write it: "66°", "13%", "4.7 GHz" (the
/// processor's clocks) or "1882 MHz", "33.4 / 60.2 GB", "2,836 RPM", "1.35 V", "73 W".
pub fn shown(what: What, part: Part, v: Val) -> String {
    let x = v.one();
    match (what, v) {
        (What::InUse, Val::Of(u, t)) => in_use(u, t),
        (What::Temperature, _) => format!("{x:.0}°"),
        (What::Load | What::Charge | What::FanShare, _) => format!("{x:.0}%"),
        (What::Clock, _) if part == Part::Processor => format!("{:.1} GHz", x / 1000.0),
        (What::Clock, _) => format!("{x:.0} MHz"),
        (What::Fan, _) => format!("{} RPM", thousands(x)),
        (What::Voltage, _) => format!("{x:.2} V"),
        (What::Power, _) if x < 10.0 => format!("{x:.1} W"),
        (What::Power, _) => format!("{x:.0} W"),
        (What::Current, _) => format!("{x:.2} A"),
        (What::InUse, _) => format!("{x:.1} GB"),
    }
}

/// A reading as the keyboard's screen writes it, as its other readings are written: "66°"
/// (°F where the block shows it so), " 13%", "4369MHz", "33.4 / 60.2 GB", "2836RPM", "1.35V", "73W".
pub fn on_screen(what: What, v: Val, fahrenheit: bool) -> String {
    let x = v.one();
    match (what, v) {
        (What::InUse, Val::Of(u, t)) => format!("{u:.1} / {t:.1} GB"),
        (What::Temperature, _) => format!("{:>2.0}°", if fahrenheit { x * 9.0 / 5.0 + 32.0 } else { x }),
        (What::Load | What::Charge | What::FanShare, _) => format!("{x:>3.0}%"),
        (What::Clock, _) => format!("{x:>4.0}MHz"),
        (What::Fan, _) => format!("{x:.0}RPM"),
        (What::Voltage, _) => format!("{x:.2}V"),
        (What::Power, _) if x < 10.0 => format!("{x:.1}W"),
        (What::Power, _) => format!("{x:.0}W"),
        (What::Current, _) => format!("{x:.2}A"),
        (What::InUse, _) => format!("{x:.1}GB"),
    }
}

/// A reading's share of its whole, 0 to 1, for a bar: a temperature of 100°, a percent, memory
/// in use of all of it. None for a reading with no whole (a clock, a fan's RPM).
pub fn share(what: What, v: Val) -> Option<f64> {
    Some(match (what, v) {
        (What::InUse, Val::Of(u, t)) if t > 0.0 => u / t,
        (What::Temperature | What::Load | What::Charge | What::FanShare, Val::One(x)) => x / 100.0,
        _ => return None,
    }
    .clamp(0.0, 1.0))
}

/// Whether a reading can't be right: a temperature no part of a computer reaches, as a
/// motherboard's input with no sensor on it reads (193°).
pub fn odd(what: What, v: Val) -> bool {
    what == What::Temperature && !(-20.0..=120.0).contains(&v.one())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{computer, nvidia, temp_dir};

    #[test]
    fn each_sensor_read_and_written_as_the_box_and_the_screen_show_it() {
        let dir = temp_dir("read");
        computer(&dir);
        let cat = crate::list::catalog(&dir, &mut nvidia());
        std::fs::write(dir.join("proc/stat"), "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 10 0 5 80 5 0 0 0\ncpu4 10 0 5 80 5 0 0 0\n").unwrap();
        let mut r = Reader::new(&dir, Box::new(nvidia()));
        let all: Vec<&Sensor> = cat.devices.iter().flat_map(|d| d.sensors.iter()).collect();
        let first = r.read(&all);
        // the processor's load is 0 the first time, then what the threads did since
        assert_eq!(first.get("cpu/load"), Some(&Val::One(0.0)));
        std::fs::write(dir.join("proc/stat"), "cpu  150 0 100 850 50 0 0 0 0 0\ncpu0 30 0 5 80 5 0 0 0\ncpu4 10 0 5 100 5 0 0 0\n").unwrap();
        let v = r.read(&all);
        assert_eq!(v.get("cpu/load"), Some(&Val::One(100.0 * (1.0 - 50.0 / 150.0))));
        // the first core: its threads' loads averaged (100% and 0%)
        assert_eq!(v.get("cpu/load/core1"), Some(&Val::One(50.0)));
        // the clocks: the average of every thread, the fastest, and each core's fastest thread
        let ghz = |id: &str| shown(What::Clock, Part::Processor, v[id]);
        assert_eq!((ghz("cpu/clock/average"), ghz("cpu/clock/fastest"), ghz("cpu/clock/core1")), ("3.1 GHz".to_string(), "5.7 GHz".to_string(), "5.4 GHz".to_string()));
        let at = |id: &str| {
            let (d, s) = cat.sensor(id).unwrap();
            shown(s.what, d.part, v[id])
        };
        assert_eq!(at("k10temp@0000:00:18.3/temp/Tctl"), "66°");
        assert_eq!(at("nct6687@nct6687.2592/fan/Pump Fan"), "2,836 RPM");
        assert_eq!(at("nct6687@nct6687.2592/in/+5V"), "4.98 V");
        assert_eq!(at("memory/used"), "33.4 / 60.2 GB");
        assert_eq!(at("card@0000:18:00.0/video-memory"), "72 MB / 2 GB");
        assert_eq!(at("card@0000:18:00.0/clock"), "2200 MHz");
        assert_eq!(at("card@0000:18:00.0/power"), "0.1 W");
        assert_eq!(at("card@0000:01:00.0/video-memory"), "4.8 / 31.8 GB");
        assert_eq!(at("card@0000:01:00.0/power"), "73 W");
        assert_eq!(at("card@0000:01:00.0/fan"), "0%");
        assert_eq!(at("card@0000:16:00.0/clock"), "1200 MHz");
        let (_, m2) = cat.sensor("nct6687@nct6687.2592/temp/M2_1").unwrap();
        assert!(odd(m2.what, v[&m2.id]) && !odd(What::Temperature, Val::One(66.0)));
        // the screen's own way of writing them
        assert_eq!(on_screen(What::Fan, Val::One(2836.0), false), "2836RPM");
        assert_eq!(on_screen(What::Temperature, Val::One(66.0), true), "151°");
        assert_eq!(on_screen(What::Clock, Val::One(1882.0), false), "1882MHz");
        assert_eq!(share(What::InUse, Val::Of(4.8, 31.8)).map(|s| (s * 100.0).round()), Some(15.0));
        assert_eq!(share(What::Fan, Val::One(900.0)), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_count_read_twice_gives_its_rate() {
        let dir = temp_dir("rate");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("count");
        std::fs::write(&f, "1000\n").unwrap();
        let mut r = Reader::new(&dir, Box::new(nvidia()));
        assert_eq!(r.rate(&f), None);
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(&f, "1000\n").unwrap();
        // (a count that hasn't moved: none of it, and never seen to move)
        assert_eq!(r.rate(&f), Some((0.0, false)));
        std::fs::write(&f, "1500\n").unwrap();
        let (per_sec, moved) = r.rate(&f).unwrap();
        assert!(moved && per_sec > 500.0);
        assert_eq!(thousands(1255.0), "1,255");
        assert_eq!(thousands(734.0), "734");
        assert_eq!(thousands(1234567.0), "1,234,567");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod here {
    /// This computer's sensors, as the Sensors box would list them, read twice a second apart
    /// (`cargo test this_computer -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn this_computer() {
        let mut r = super::Reader::new("/", Box::new(crate::list::NoNvidia));
        let cat = crate::list::catalog(std::path::Path::new("/"), r.nvidia());
        let all: Vec<&crate::list::Sensor> = cat.devices.iter().flat_map(|d| d.sensors.iter()).collect();
        r.read(&all);
        std::thread::sleep(std::time::Duration::from_secs(1));
        let v = r.read(&all);
        for d in &cat.devices {
            println!("== {} | {} | {} | {} | {:?} | {:?}", d.part.name(), d.key, d.name, d.src, d.inside, d.note);
            for s in &d.sensors {
                let shown = v.get(&s.id).map(|x| super::shown(s.what, d.part, *x)).unwrap_or_else(|| "-".into());
                println!("   {:<44} {:<22} {:<12} {:<10} {:?}{}", s.id, s.name, s.orig.clone().unwrap_or_default(), shown, s.run, if s.usual { " usual" } else { "" });
            }
        }
        for (p, why) in &cat.missing {
            println!("missing {}: {why}", p.name());
        }
        println!("main by rule: {:?}", cat.main_by_rule().map(|d| &d.key));
        println!("{} sensors on {} devices", all.len(), cat.devices.len());
    }
}
