//! One light across Home's desk: an effect played over where each device's
//! lights sit on the desk, in millimeters, so a wave crosses the keyboard and
//! then the mouse as it would cross a real desk. Each device is sent its part
//! in its direct mode (the keyboard's keys, the mouse's logo and wheel), which
//! its memory never keeps; turned off, each plays its own profile again.

/// The effects the desk can play, by the words Home's menu has for them, and
/// what each is kept as in Ectomatter's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Off,
    /// the spectrum sweeping across the desk, left to right
    Wave,
    /// the whole desk going round the spectrum together
    Shift,
}

pub const EFFECTS: [(Effect, &str, &str); 3] = [(Effect::Off, "off", "Off"), (Effect::Wave, "wave", "Wave across the desk"), (Effect::Shift, "shift", "Color shift together")];

impl Effect {
    pub fn id(self) -> &'static str {
        EFFECTS.iter().find(|(e, ..)| *e == self).map_or("off", |(_, id, _)| id)
    }

    pub fn of(id: &str) -> Effect {
        EFFECTS.iter().find(|(_, i, _)| *i == id).map_or(Effect::Off, |(e, ..)| *e)
    }

    /// Its words on Home's Desk light button, short.
    pub fn short(self) -> &'static str {
        match self {
            Effect::Off => "",
            Effect::Wave => "Wave",
            Effect::Shift => "Color shift",
        }
    }
}

/// How wide one pass of the wave's spectrum is across the desk, and how long
/// a pass takes to go by; how long the whole desk takes round the spectrum.
const WAVE_MM: f32 = 420.0;
const WAVE_S: f32 = 4.0;
const SHIFT_S: f32 = 6.0;

/// The effect's color at a place on the desk (millimeters from the desk's
/// left and top) `t` seconds in.
pub fn color(e: Effect, x_mm: f32, _y_mm: f32, t: f32) -> [u8; 3] {
    match e {
        Effect::Off => [0; 3],
        Effect::Wave => hue((x_mm / WAVE_MM - t / WAVE_S).rem_euclid(1.0)),
        Effect::Shift => hue((t / SHIFT_S).rem_euclid(1.0)),
    }
}

/// A hue (0 to 1 round the spectrum) at its full strength, as an LED shows it.
fn hue(h: f32) -> [u8; 3] {
    let k = |n: f32| {
        let k = (n + h * 6.0).rem_euclid(6.0);
        (1.0 - (k.min(4.0 - k).clamp(0.0, 1.0))) * 255.0
    };
    [k(5.0).round() as u8, k(3.0).round() as u8, k(1.0).round() as u8]
}

/// A device's lights on the desk: each light (a keyboard's key by its place,
/// a mouse's light by its id) and where it sits, in millimeters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Places {
    pub keys: Vec<(u32, f32, f32)>,
    pub mouse: Vec<(&'static str, f32, f32)>,
}

/// Where a point of a device's drawing is on the desk: the drawing's box
/// (`view`, x, y, width, height in its own units) laid on the device's
/// place on the desk (`item`, x, y, width, height in the desk's units).
pub fn on_desk(view: (f64, f64, f64, f64), item: [f32; 4], x: f64, y: f64) -> (f32, f32) {
    let (vx, vy, vw, vh) = view;
    let [ix, iy, iw, ih] = item;
    let dx = ix + ((x - vx) / vw.max(1e-6)) as f32 * iw;
    let dy = iy + ((y - vy) / vh.max(1e-6)) as f32 * ih;
    (dx / crate::desk::PER_MM, dy / crate::desk::PER_MM)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wave crosses the desk: two places a pass apart show one color, a
    /// place half a pass on the opposite one, and the wave moves on in time;
    /// the color shift is one color everywhere; off is dark.
    #[test]
    fn the_wave_crosses_the_desk() {
        let w = |x: f32, t: f32| color(Effect::Wave, x, 0.0, t);
        assert_eq!(w(0.0, 0.0), w(WAVE_MM, 0.0));
        assert_ne!(w(0.0, 0.0), w(WAVE_MM / 2.0, 0.0));
        assert_eq!(w(WAVE_MM / 4.0, WAVE_S / 4.0), w(0.0, 0.0), "the same color a quarter on, a quarter later");
        assert_eq!(color(Effect::Shift, 0.0, 0.0, 1.0), color(Effect::Shift, 300.0, 80.0, 1.0));
        assert_eq!(color(Effect::Off, 10.0, 10.0, 1.0), [0; 3]);
        assert_eq!(hue(0.0), [255, 0, 0]);
        assert_eq!(hue(1.0 / 3.0), [0, 255, 0]);
        assert_eq!(Effect::of(Effect::Wave.id()), Effect::Wave);
        // (a drawing's middle at its place's middle, in millimeters)
        let (x, y) = on_desk((0.0, 0.0, 100.0, 50.0), [760.0, 0.0, 760.0, 380.0], 50.0, 25.0);
        assert!((x - 470.0 * 1.5).abs() < 0.01 && (y - 470.0 / 4.0).abs() < 0.01, "{x} {y}");
    }
}
