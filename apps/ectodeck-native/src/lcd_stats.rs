//! The Stats page: the computer's processor, graphics card and memory, in
//! the four rows Grant laid out for the z10-lcd service, read and written as
//! z10lcd.py reads and writes them, so the keyboard shows the same
//! characters. The processor and memory come from the kernel (k10temp's
//! Tctl, each core's clock, /proc), the graphics card from NVIDIA's driver
//! (NVML, opened when first needed). Everything is read under `root`, which
//! is `/` but for tests, which hand in a made-up computer.

use std::path::{Path, PathBuf};

/// One reading of the graphics card: temperature, load, core clock, video
/// memory used and in all (GiB).
pub type GpuReading = (u32, u32, u32, f64, f64);

pub trait Gpu: Send {
    fn read(&mut self) -> Result<GpuReading, String>;
}

pub struct Stats {
    root: PathBuf,
    k10: Option<PathBuf>,
    prev: Option<(u64, u64)>,
    gpu: Box<dyn Gpu>,
}

impl Stats {
    pub fn new(root: impl Into<PathBuf>, gpu: Box<dyn Gpu>) -> Stats {
        let root = root.into();
        let k10 = hwmon(&root, "k10temp");
        Stats { root, k10, prev: None, gpu }
    }

    /// This computer, its graphics card through NVML.
    pub fn here() -> Stats {
        Stats::new("/", Box::new(Nvml::default()))
    }

    fn at(&self, p: &str) -> PathBuf {
        self.root.join(p.trim_start_matches('/'))
    }

    fn cpu_temp(&self) -> Option<f64> {
        let k = self.k10.as_ref()?;
        let v: i64 = std::fs::read_to_string(k.join("temp1_input")).ok()?.trim().parse().ok()?;
        Some(v as f64 / 1000.0)
    }

    fn cpu_ghz(&self) -> Option<f64> {
        let mut fs = Vec::new();
        for e in std::fs::read_dir(self.at("/sys/devices/system/cpu")).ok()?.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.len() > 3 && name.starts_with("cpu") && name.as_bytes()[3].is_ascii_digit() {
                if let Some(v) = std::fs::read_to_string(e.path().join("cpufreq/scaling_cur_freq")).ok().and_then(|s| s.trim().parse::<u64>().ok()) {
                    fs.push(v);
                }
            }
        }
        if fs.is_empty() {
            return None;
        }
        Some(fs.iter().sum::<u64>() as f64 / fs.len() as f64 / 1e6)
    }

    /// The processor's load since the last reading, in percent; 0 the first time.
    pub fn cpu_load(&mut self) -> f64 {
        let Ok(s) = std::fs::read_to_string(self.at("/proc/stat")) else { return 0.0 };
        let v: Vec<u64> = s.lines().next().unwrap_or("").split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
        if v.len() < 5 {
            return 0.0;
        }
        let (idle, total) = (v[3] + v[4], v.iter().sum::<u64>());
        let Some((pi, pt)) = self.prev.replace((idle, total)) else { return 0.0 };
        let (di, dt) = (idle as i64 - pi as i64, total as i64 - pt as i64);
        if dt != 0 { 100.0 * (1.0 - di as f64 / dt as f64) } else { 0.0 }
    }

    fn ram_gib(&self) -> Option<(f64, f64)> {
        let s = std::fs::read_to_string(self.at("/proc/meminfo")).ok()?;
        let field = |k: &str| -> Option<u64> {
            s.lines().find_map(|l| l.split_once(':').filter(|(n, _)| *n == k).and_then(|(_, v)| v.split_whitespace().next()?.parse::<u64>().ok()))
        };
        let (total, avail) = (field("MemTotal")? * 1024, field("MemAvailable")? * 1024);
        Some(((total - avail) as f64 / (1u64 << 30) as f64, total as f64 / (1u64 << 30) as f64))
    }

    /// Everything the computer reads now, for a page (the time, the profile
    /// and the weather are the page's player's to add).
    pub fn read(&mut self) -> crate::lcd_page::Readings {
        // in the order the service read them
        let (cpu_temp, cpu_load, cpu_ghz) = (self.cpu_temp(), self.cpu_load(), self.cpu_ghz());
        let ram = self.ram_gib();
        let gpu = self.gpu.read().ok();
        crate::lcd_page::Readings { cpu_temp, cpu_load, cpu_ghz, ram, gpu, ..Default::default() }
    }

    /// The Stats page's four rows, as the service wrote them. A reading that
    /// can't be had says so in its row.
    pub fn rows(&mut self) -> Vec<String> {
        let r = self.read();
        crate::lcd_page::render(&crate::lcd_page::Page::stats(), &r)
    }
}

/// The hwmon device of a name, under root.
fn hwmon(root: &Path, name: &str) -> Option<PathBuf> {
    let dir = root.join("sys/class/hwmon");
    let mut all: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    all.sort();
    all.into_iter().find(|h| std::fs::read_to_string(h.join("name")).is_ok_and(|n| n.trim() == name))
}

// ------------------------------------------------------------------ NVIDIA's driver

type NvmlFn0 = unsafe extern "C" fn() -> i32;
type NvmlHandle = unsafe extern "C" fn(u32, *mut *mut libc::c_void) -> i32;
type NvmlU32 = unsafe extern "C" fn(*mut libc::c_void, u32, *mut u32) -> i32;
type NvmlUtil = unsafe extern "C" fn(*mut libc::c_void, *mut [u32; 2]) -> i32;
type NvmlMem = unsafe extern "C" fn(*mut libc::c_void, *mut [u64; 3]) -> i32;

struct NvmlLib {
    handle: NvmlHandle,
    temp: NvmlU32,
    clock: NvmlU32,
    util: NvmlUtil,
    mem: NvmlMem,
}

/// The first graphics card, through NVML, opened when first read and again
/// after a failure.
#[derive(Default)]
pub struct Nvml {
    lib: Option<NvmlLib>,
    dev: Option<usize>,
}

// (the device handle is NVML's own and used only from the thread that holds this)
unsafe impl Send for Nvml {}

impl Nvml {
    fn open(&mut self) -> Result<(), String> {
        if self.lib.is_none() {
            unsafe {
                let h = libc::dlopen(c"libnvidia-ml.so.1".as_ptr(), libc::RTLD_NOW);
                if h.is_null() {
                    return Err("no NVML".into());
                }
                let sym = |n: &std::ffi::CStr| libc::dlsym(h, n.as_ptr());
                let init = sym(c"nvmlInit_v2");
                let names = [c"nvmlDeviceGetHandleByIndex_v2", c"nvmlDeviceGetTemperature", c"nvmlDeviceGetClockInfo", c"nvmlDeviceGetUtilizationRates", c"nvmlDeviceGetMemoryInfo"];
                let ptrs: Vec<*mut libc::c_void> = names.iter().map(|n| sym(n)).collect();
                if init.is_null() || ptrs.iter().any(|p| p.is_null()) {
                    return Err("NVML is missing a function".into());
                }
                if std::mem::transmute::<*mut libc::c_void, NvmlFn0>(init)() != 0 {
                    return Err("nvmlInit failed".into());
                }
                self.lib = Some(NvmlLib {
                    handle: std::mem::transmute::<*mut libc::c_void, NvmlHandle>(ptrs[0]),
                    temp: std::mem::transmute::<*mut libc::c_void, NvmlU32>(ptrs[1]),
                    clock: std::mem::transmute::<*mut libc::c_void, NvmlU32>(ptrs[2]),
                    util: std::mem::transmute::<*mut libc::c_void, NvmlUtil>(ptrs[3]),
                    mem: std::mem::transmute::<*mut libc::c_void, NvmlMem>(ptrs[4]),
                });
            }
        }
        let lib = self.lib.as_ref().unwrap();
        let mut d: *mut libc::c_void = std::ptr::null_mut();
        if unsafe { (lib.handle)(0, &mut d) } != 0 {
            return Err("no GPU 0".into());
        }
        self.dev = Some(d as usize);
        Ok(())
    }
}

impl Gpu for Nvml {
    fn read(&mut self) -> Result<GpuReading, String> {
        if self.dev.is_none() {
            self.open()?;
        }
        let (lib, d) = (self.lib.as_ref().unwrap(), self.dev.unwrap() as *mut libc::c_void);
        let (mut t, mut clk, mut u, mut m) = (0u32, 0u32, [0u32; 2], [0u64; 3]);
        // temperature of the GPU die (sensor 0), the graphics clock (clock 0)
        let rc = unsafe {
            let a = (lib.temp)(d, 0, &mut t);
            if a != 0 { a } else {
                let b = (lib.clock)(d, 0, &mut clk);
                if b != 0 { b } else {
                    let c = (lib.util)(d, &mut u);
                    if c != 0 { c } else { (lib.mem)(d, &mut m) }
                }
            }
        };
        if rc != 0 {
            self.dev = None;
            return Err(format!("NVML error {rc}"));
        }
        let gib = (1u64 << 30) as f64;
        Ok((t, u[0], clk, m[2] as f64 / gib, m[0] as f64 / gib))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub struct FakeGpu(pub Vec<Result<GpuReading, String>>);
    impl Gpu for FakeGpu {
        fn read(&mut self) -> Result<GpuReading, String> {
            if self.0.len() > 1 { self.0.remove(0) } else { self.0[0].clone() }
        }
    }

    /// A made-up computer: k10temp, four cores, /proc/stat and /proc/meminfo.
    pub fn computer(dir: &Path, temp: i64, freqs: &[u64], stat: [u64; 8], mem: (u64, u64)) {
        let h = dir.join("sys/class/hwmon/hwmon3");
        std::fs::create_dir_all(&h).unwrap();
        std::fs::write(h.join("name"), "k10temp\n").unwrap();
        std::fs::write(h.join("temp1_input"), format!("{temp}\n")).unwrap();
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
    }

    #[test]
    fn the_rows_as_the_service_wrote_them() {
        let dir = std::env::temp_dir().join(format!("ectodeck-stats-{}", std::process::id()));
        computer(&dir, 64500, &[4365000, 4400000, 4300000, 4410000], [100, 0, 50, 800, 50, 0, 0, 0], (63119752, 36839852));
        let mut s = Stats::new(&dir, Box::new(FakeGpu(vec![Ok((45, 12, 1837, 13.13, 31.84))])));
        let rows = s.rows();
        assert_eq!(rows, vec!["CPU 64°   0% 4369MHz", "GPU 45°  12% 1837MHz", "RAM   25.1 / 60.2 GB", "VRAM  13.1 / 31.8 GB"]);
        let mut s = Stats::new(&dir, Box::new(FakeGpu(vec![Err("no".into())])));
        assert_eq!(s.rows()[1..], ["GPU unavailable".to_string(), "RAM   25.1 / 60.2 GB".into(), String::new()]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
