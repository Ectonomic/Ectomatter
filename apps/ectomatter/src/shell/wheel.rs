//! A mouse wheel's steps, eased. Slint eases a wheel's step over a Flickable
//! only for its own window backends: the event a window of one's own sends,
//! `WindowEvent::PointerScrolled`, reaches the Flickable marked cancelled
//! and is taken at once (i-slint-core 1.18.1, api.rs, and its current source
//! the same). So Ectomatter eases each step itself, as Slint would: the
//! step's distance run out over 180 ms, slowing steadily to a stop
//! (flickable.rs, `WHEEL_SCROLL_DURATION` and its constant deceleration), a
//! step that comes before the last has run out adding what is left of it. A
//! window's loop sends what is due each frame as a scroll of its own. A
//! touchpad's scrolling, smooth already, goes as it comes.

use std::time::{Duration, Instant};

/// How long a step takes to run out.
pub const STEP: Duration = Duration::from_millis(180);

/// The wheel's steps running out over one window.
#[derive(Default)]
pub struct Wheel {
    /// when the run began, how far it goes (across and down), and how far it has been sent
    run: Option<(Instant, [f32; 2], [f32; 2])>,
}

impl Wheel {
    /// A step of the wheel, `d` across and down, at `now`: what's left of the
    /// run going, and this, run out from now.
    pub fn step(&mut self, d: [f32; 2], now: Instant) {
        let left = self.run.map_or([0.0; 2], |(_, all, sent)| [all[0] - sent[0], all[1] - sent[1]]);
        self.run = Some((now, [left[0] + d[0], left[1] + d[1]], [0.0; 2]));
    }

    /// What is due to be scrolled by `now` and hasn't been, if anything.
    pub fn due(&mut self, now: Instant) -> Option<[f32; 2]> {
        let (t0, all, sent) = self.run.as_mut()?;
        let k = (now.saturating_duration_since(*t0).as_secs_f32() / STEP.as_secs_f32()).min(1.0);
        // (slowing steadily to a stop at the end: so far, the whole times 2k − k²)
        let gone = k * (2.0 - k);
        let at = [all[0] * gone, all[1] * gone];
        let d = [at[0] - sent[0], at[1] - sent[1]];
        *sent = at;
        if k >= 1.0 {
            self.run = None;
        }
        (d != [0.0; 2]).then_some(d)
    }

    /// Whether a run is going.
    pub fn running(&self) -> bool {
        self.run.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A step runs out over 180 ms, fast at first and slowing to a stop, all
    /// of it and no more; one that comes while one runs adds what is left.
    #[test]
    fn a_wheels_step_runs_out_smoothly() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut w = Wheel::default();
        assert!(w.due(t0).is_none() && !w.running());
        w.step([0.0, -120.0], t0);
        let mut parts = Vec::new();
        for n in (16..=208).step_by(16) {
            if let Some(d) = w.due(ms(n)) {
                parts.push(d[1]);
            }
        }
        assert!(!w.running(), "it has run out");
        assert!((parts.iter().sum::<f32>() + 120.0).abs() < 1e-3, "all of it: {parts:?}");
        assert!(parts.len() >= 10, "over many frames: {parts:?}");
        assert!(parts.windows(2).all(|p| p[1].abs() <= p[0].abs() + 1e-4), "slowing: {parts:?}");
        // a second step halfway through a first: what's left of the first and the second
        w.step([0.0, 100.0], ms(1000));
        w.due(ms(1090));
        w.step([0.0, 100.0], ms(1090));
        let mut sum = w.due(ms(1090)).map_or(0.0, |d| d[1]);
        let mut n = 1106;
        while w.running() {
            sum += w.due(ms(n)).map_or(0.0, |d| d[1]);
            n += 16;
        }
        let first = 100.0 * (0.5 * (2.0 - 0.5));
        assert!((first + sum - 200.0).abs() < 1e-3, "both steps whole: {}", first + sum);
    }
}
