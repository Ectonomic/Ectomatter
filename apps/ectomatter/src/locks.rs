//! The computer's lock keys (Num Lock, Caps Lock, Scroll Lock) as the
//! keyboards' lights show them, for the keyboard's picture. They are the
//! computer's, the same on every keyboard, so no device is opened for them:
//! on Linux the kernel keeps each keyboard's lock lights as lights of its own
//! (/sys/class/leds/input5::capslock, its brightness 0 or more), and they are
//! read there. Elsewhere none are read yet, and the picture shows them off.

use std::time::{Duration, Instant};

/// How often the lights are read, and how often the lights there are looked
/// for again (a keyboard plugged in or out).
const EVERY: Duration = Duration::from_millis(100);
const LOOK_EVERY: Duration = Duration::from_secs(2);

/// The lock lights each light's name ends with, and their bits.
const LOCKS: [(&str, u8); 3] = [("::numlock", 1), ("::capslock", 2), ("::scrolllock", 4)];

/// Read the lock lights on a thread of their own for as long as Ectomatter
/// runs: `tell` hears them as they are at first, and each time they change.
pub fn start(tell: impl Fn(u8) + Send + 'static) {
    std::thread::spawn(move || {
        let mut lights: Vec<(std::path::PathBuf, u8)> = Vec::new();
        let mut looked: Option<Instant> = None;
        let mut last: Option<u8> = None;
        loop {
            if looked.is_none_or(|t| t.elapsed() >= LOOK_EVERY) {
                lights = find();
                looked = Some(Instant::now());
            }
            let now = lights.iter().filter(|(p, _)| std::fs::read_to_string(p).is_ok_and(|b| b.trim().parse::<u32>().is_ok_and(|v| v > 0))).fold(0, |on, (_, bit)| on | bit);
            if last != Some(now) {
                last = Some(now);
                tell(now);
            }
            std::thread::sleep(EVERY);
        }
    });
}

/// Every keyboard's lock lights: each one's brightness file, and its bit.
fn find() -> Vec<(std::path::PathBuf, u8)> {
    let Ok(dir) = std::fs::read_dir(crate::computer::here("/sys/class/leds")) else { return Vec::new() };
    dir.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let (_, bit) = LOCKS.iter().find(|(end, _)| name.starts_with("input") && name.ends_with(end))?;
            Some((e.path().join("brightness"), *bit))
        })
        .collect()
}
