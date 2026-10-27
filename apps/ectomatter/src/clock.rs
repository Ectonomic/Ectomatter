//! The time of day as Ectomatter and its packages read it, and the window's
//! animations' clock. A snapshot can fix the time of day (`ECTOMATTER_NOW`,
//! seconds since 1970) so its pictures come out the same at any hour; then
//! the animations' clock moves only a frame's 16 ms each frame the snapshot
//! lets pass (`frame`), however long a frame really takes, so an animation
//! stands at the same place in every drawing. A running Ectomatter reads the
//! computer's clocks. Timers and waits keep the real time either way.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static FIXED: OnceLock<Duration> = OnceLock::new();

/// A frame, as the stepped animations' clock counts it.
pub const FRAME: Duration = Duration::from_millis(16);

/// The animations' clock while the time is fixed, in milliseconds.
static STEPPED: AtomicU64 = AtomicU64::new(0);

/// Fixes the time of day from here on, since 1970, and steps the
/// animations' clock by frames.
pub fn fix(since_1970: Duration) {
    let _ = FIXED.set(since_1970);
}

/// The animations' clock: since `start`, or the frames let pass while the
/// time is fixed.
pub fn animations(start: std::time::Instant) -> Duration {
    match fixed() {
        Some(_) => Duration::from_millis(STEPPED.load(Ordering::Relaxed)),
        None => start.elapsed(),
    }
}

/// How many frames make `ms`.
pub fn frames(ms: u64) -> u64 {
    ms.div_ceil(FRAME.as_millis() as u64)
}

/// A frame passes: a frame's real time, for what's working on its own
/// threads, and the animations' clock a frame on while the time is fixed.
pub fn frame() {
    std::thread::sleep(FRAME);
    if fixed().is_some() {
        STEPPED.fetch_add(FRAME.as_millis() as u64, Ordering::Relaxed);
    }
}

/// The time of day as fixed, if it is.
pub fn fixed() -> Option<Duration> {
    FIXED.get().copied()
}

/// The time now, since 1970.
pub fn now() -> Duration {
    fixed().unwrap_or_else(|| SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default())
}

/// A fixed time of day, as a package's clock.
pub struct Still(pub Duration);

impl wasmtime_wasi::HostWallClock for Still {
    fn resolution(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn now(&self) -> Duration {
        self.0
    }
}
