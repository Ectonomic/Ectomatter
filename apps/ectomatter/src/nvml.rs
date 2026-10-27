//! NVIDIA's driver through NVML, which Ectomatter loads itself and lends a
//! plugin only through the system interface's `nvidia`, for a plugin whose
//! manifest names it (computer.rs): the Sensor Feed reads NVIDIA cards so.

use std::collections::HashMap;
use std::time::Instant;

use ectomatter_sensors::list::{Nv, Nvidia, Probe, Val};

// ------------------------------------------------------------------ NVIDIA's driver

type NvmlInit = unsafe extern "C" fn() -> i32;
type NvmlBus = unsafe extern "C" fn(*const libc::c_char, *mut *mut libc::c_void) -> i32;
type NvmlU32 = unsafe extern "C" fn(*mut libc::c_void, u32, *mut u32) -> i32;
type NvmlOne = unsafe extern "C" fn(*mut libc::c_void, *mut u32) -> i32;
type NvmlUtil = unsafe extern "C" fn(*mut libc::c_void, *mut [u32; 2]) -> i32;
type NvmlMem = unsafe extern "C" fn(*mut libc::c_void, *mut [u64; 3]) -> i32;

struct NvmlLib {
    by_bus: NvmlBus,
    temp: NvmlU32,
    clock: NvmlU32,
    util: NvmlUtil,
    mem: NvmlMem,
    power: NvmlOne,
    fan: NvmlOne,
}

/// NVIDIA's driver through NVML: opened when first asked, and again a while after it
/// couldn't be (a driver that loads late); each card by its PCI address.
#[derive(Default)]
pub struct Nvml {
    lib: Option<NvmlLib>,
    failed: Option<Instant>,
    cards: HashMap<String, usize>,
}

// (the handles are NVML's own and used only from the thread that holds this)
unsafe impl Send for Nvml {}

impl Nvml {
    fn open(&mut self) -> Option<&NvmlLib> {
        if self.lib.is_none() {
            if self.failed.is_some_and(|t| t.elapsed().as_secs() < 30) {
                return None;
            }
            self.lib = unsafe { Self::load() };
            if self.lib.is_none() {
                self.failed = Some(Instant::now());
            }
        }
        self.lib.as_ref()
    }

    #[cfg(target_os = "linux")]
    unsafe fn load() -> Option<NvmlLib> {
        unsafe {
            let h = libc::dlopen(c"libnvidia-ml.so.1".as_ptr(), libc::RTLD_NOW);
            if h.is_null() {
                return None;
            }
            let sym = |n: &std::ffi::CStr| libc::dlsym(h, n.as_ptr());
            let init = sym(c"nvmlInit_v2");
            let names = [c"nvmlDeviceGetHandleByPciBusId_v2", c"nvmlDeviceGetTemperature", c"nvmlDeviceGetClockInfo", c"nvmlDeviceGetUtilizationRates", c"nvmlDeviceGetMemoryInfo", c"nvmlDeviceGetPowerUsage", c"nvmlDeviceGetFanSpeed"];
            let p: Vec<*mut libc::c_void> = names.iter().map(|n| sym(n)).collect();
            if init.is_null() || p.iter().any(|x| x.is_null()) || std::mem::transmute::<*mut libc::c_void, NvmlInit>(init)() != 0 {
                return None;
            }
            Some(NvmlLib {
                by_bus: std::mem::transmute::<*mut libc::c_void, NvmlBus>(p[0]),
                temp: std::mem::transmute::<*mut libc::c_void, NvmlU32>(p[1]),
                clock: std::mem::transmute::<*mut libc::c_void, NvmlU32>(p[2]),
                util: std::mem::transmute::<*mut libc::c_void, NvmlUtil>(p[3]),
                mem: std::mem::transmute::<*mut libc::c_void, NvmlMem>(p[4]),
                power: std::mem::transmute::<*mut libc::c_void, NvmlOne>(p[5]),
                fan: std::mem::transmute::<*mut libc::c_void, NvmlOne>(p[6]),
            })
        }
    }

    /// (elsewhere NVIDIA's library isn't loaded yet: docs/deferred.md, stage 5)
    #[cfg(not(target_os = "linux"))]
    unsafe fn load() -> Option<NvmlLib> {
        None
    }

    /// The card at a PCI address, as NVML holds it.
    fn card(&mut self, pci: &str) -> Option<(*mut libc::c_void, &NvmlLib)> {
        if !self.cards.contains_key(pci) {
            let d = {
                let lib = self.open()?;
                let b = std::ffi::CString::new(pci).ok()?;
                let mut d: *mut libc::c_void = std::ptr::null_mut();
                if unsafe { (lib.by_bus)(b.as_ptr(), &mut d) } != 0 {
                    return None;
                }
                d as usize
            };
            self.cards.insert(pci.to_string(), d);
        }
        let d = *self.cards.get(pci)? as *mut libc::c_void;
        Some((d, self.lib.as_ref()?))
    }

    fn get(&mut self, pci: &str, what: Nv) -> Result<Val, i32> {
        let Some((d, lib)) = self.card(pci) else { return Err(-1) };
        let (mut x, mut u, mut m) = (0u32, [0u32; 2], [0u64; 3]);
        let gib = (1u64 << 30) as f64;
        // the temperature of the chip itself (sensor 0); the graphics clock (0), the memory's (2)
        let rc = unsafe {
            match what {
                Nv::Temp => (lib.temp)(d, 0, &mut x),
                Nv::Clock => (lib.clock)(d, 0, &mut x),
                Nv::MemClock => (lib.clock)(d, 2, &mut x),
                Nv::Load => (lib.util)(d, &mut u),
                Nv::Vram => (lib.mem)(d, &mut m),
                Nv::Power => (lib.power)(d, &mut x),
                Nv::Fan => (lib.fan)(d, &mut x),
            }
        };
        if rc != 0 {
            return Err(rc);
        }
        Ok(match what {
            Nv::Load => Val::One(u[0] as f64),
            // (total, free, used)
            Nv::Vram => Val::Of(m[2] as f64 / gib, m[0] as f64 / gib),
            Nv::Power => Val::One(x as f64 / 1000.0),
            _ => Val::One(x as f64),
        })
    }
}

impl Nvidia for Nvml {
    fn probe(&mut self, pci: &str) -> Option<Probe> {
        self.card(pci)?;
        let all = [Nv::Temp, Nv::Load, Nv::Clock, Nv::Vram, Nv::MemClock, Nv::Power, Nv::Fan];
        let mut p = Probe::default();
        for n in all {
            // (a reading the card doesn't have answers "not supported"; a fan at rest still reads)
            if let Ok(v) = self.get(pci, n) {
                p.has.push(n);
                if let Val::Of(_, t) = v {
                    p.vram = t;
                }
            }
        }
        Some(p)
    }

    fn read(&mut self, pci: &str, what: Nv) -> Option<Val> {
        match self.get(pci, what) {
            Ok(v) => Some(v),
            Err(_) => {
                // (the card asked for again next time: NVML's handle goes with a driver reloaded)
                self.cards.remove(pci);
                None
            }
        }
    }
}

