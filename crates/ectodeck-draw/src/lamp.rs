//! A device's lights that shine through a window of diffusing plastic, as a
//! mouse's logo and wheel do: an LED under each window, its light spread
//! across the window by the law the keys' light falls by (see falloff), all
//! that passes leaving through the window's shape, so the window is as
//! bright as the LED's lumens make it. A share of the light, the halo, gets
//! out round the window's edge onto the shell beside it. The keys are lit by
//! the simulation (sim); this is the same light, in the same units, for a
//! part with no keys over it.
//!
//! A window that lets a share `through` of an LED's Φ lumens out, over an
//! area A, glows as a diffuser does, the same in every direction: its
//! luminance is what leaves each square meter, over π. The LED's light falls
//! on the window's back most strongly straight over it, as `falloff` has it
//! `depth` below, so the window is brightest there.

use serde_json::Value;

use super::{falloff, lin, within_peak};

/// How one light is made, in the units that mean something.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Make {
    /// the light its LED gives off at full white, in lumens
    pub lumens: f32,
    /// the share of it that comes out of the window
    pub through: f32,
    /// how far under the window's face the LED sits, in millimeters
    pub depth: f32,
    /// the share that gets out round the window's edge onto the shell, and
    /// how far from the edge that reaches before it falls to about a third,
    /// in millimeters
    pub halo: f32,
    pub reach: f32,
    /// where the LED sits under the window: so many millimeters east and
    /// north of the window's middle (below 0, west and south)
    pub east: f32,
    pub north: f32,
}

impl Make {
    /// A small RGB LED behind a light pipe: what a light the core says nothing of has.
    pub const DEFAULT: Make = Make { lumens: 0.8, through: 0.5, depth: 8.0, halo: 0.05, reach: 2.0, east: 0.0, north: 0.0 };

    /// A light's make as the core's light describes it: its own values (its
    /// `window` under `keys`, by its id, where the light keeps each part's
    /// own, as a keyboard's keeps each key's), then what the light says for
    /// every window (`window`), then DEFAULT.
    pub fn of(light: &Value, id: &str) -> Make {
        let own = &light["keys"][id]["window"];
        let all = &light["window"];
        let f = |k: &str, d: f32| own[k].as_f64().or_else(|| all[k].as_f64()).map_or(d, |v| v as f32);
        let d = Make::DEFAULT;
        Make {
            lumens: f("lumens", d.lumens).max(0.0),
            through: f("through", d.through).clamp(0.0, 1.0),
            depth: f("depth", d.depth).max(0.05),
            halo: f("halo", d.halo).clamp(0.0, 1.0),
            reach: f("reach", d.reach).max(0.05),
            east: f("east", d.east),
            north: f("north", d.north),
        }
    }
}

/// One light at one size: its luminance at each pixel of its box, in nits,
/// its LED at full white.
#[derive(Clone, Debug)]
pub struct Lamp {
    pub x: i32,
    pub y: i32,
    pub w: usize,
    pub h: usize,
    nits: Vec<f32>,
}

impl Lamp {
    /// The light through a window whose shape covers `cover` of each pixel
    /// (0 to 1, `w` by `h`, its top left corner at `x`, `y` on the picture),
    /// drawn `px_mm` pixels to the millimeter. The box grows to take the
    /// halo round the window.
    pub fn new(cover: &[f32], w: usize, h: usize, x: i32, y: i32, px_mm: f32, make: &Make) -> Lamp {
        let px_mm = px_mm.max(1e-3);
        // (a pixel's area, in square meters)
        let da = (1.0 / (px_mm * 1000.0)).powi(2);
        // the halo's room round the window: three of its reaches
        let sigma = make.reach * px_mm;
        let pad = if make.halo > 0.0 { (3.0 * sigma).ceil() as usize } else { 0 };
        let (bw, bh) = (w + 2 * pad, h + 2 * pad);
        let mut c = vec![0f32; bw * bh];
        for r in 0..h {
            c[(r + pad) * bw + pad..(r + pad) * bw + pad + w].copy_from_slice(&cover[r * w..(r + 1) * w]);
        }
        // the LED's place: the window's middle (its coverage's), moved as the make says
        let total: f32 = c.iter().sum();
        let mut nits = vec![0f32; bw * bh];
        if total <= 0.0 {
            return Lamp { x: x - pad as i32, y: y - pad as i32, w: bw, h: bh, nits };
        }
        let (mut mx, mut my) = (0f32, 0f32);
        for (i, v) in c.iter().enumerate() {
            mx += v * ((i % bw) as f32 + 0.5);
            my += v * ((i / bw) as f32 + 0.5);
        }
        let (lx, ly) = (mx / total + make.east * px_mm, my / total - make.north * px_mm);
        let depth = make.depth * px_mm;
        // through the window: the light on its back, as it falls from the LED
        let mut fall = vec![0f32; bw * bh];
        let mut sum = 0f32;
        for (i, f) in fall.iter_mut().enumerate() {
            if c[i] > 0.0 {
                let r = ((((i % bw) as f32 + 0.5) - lx).powi(2) + (((i / bw) as f32 + 0.5) - ly).powi(2)).sqrt();
                *f = c[i] * falloff(r, depth);
                sum += *f;
            }
        }
        // what leaves the window, each square meter's share over π
        let out = make.lumens * make.through;
        if sum > 0.0 {
            let k = out / (std::f32::consts::PI * sum * da);
            for (n, f) in nits.iter_mut().zip(&fall) {
                *n = f * k;
            }
        }
        // the halo: the window's light spread past its edge, on the shell round it
        if make.halo > 0.0 && sigma > 0.0 {
            let blurred = blur(&c, bw, bh, sigma);
            let ring: Vec<f32> = blurred.iter().zip(&c).map(|(b, c)| (b * (1.0 - c)).max(0.0)).collect();
            let rs: f32 = ring.iter().sum();
            if rs > 0.0 {
                let k = make.lumens * make.halo / (std::f32::consts::PI * rs * da);
                for (n, r) in nits.iter_mut().zip(&ring) {
                    *n += r * k;
                }
            }
        }
        Lamp { x: x - pad as i32, y: y - pad as i32, w: bw, h: bh, nits }
    }

    /// The brightest it gets, in nits, its LED at full white.
    pub fn peak(&self) -> f32 {
        self.nits.iter().copied().fold(0.0, f32::max)
    }

    /// Its light at a pixel of its box, in nits at full white.
    pub fn at(&self, x: usize, y: usize) -> f32 {
        if x < self.w && y < self.h { self.nits[y * self.w + x] } else { 0.0 }
    }

    /// All the light that leaves it, in lumens: its nits over each pixel's
    /// area, times π (`px_mm` the pixels to a millimeter it was made at).
    pub fn lumens(&self, px_mm: f32) -> f32 {
        let da = (1.0 / (px_mm * 1000.0)).powi(2);
        self.nits.iter().sum::<f32>() * da * std::f32::consts::PI
    }

    /// Its light, its LED in `rgb` at `strength` (0 to 1), added to a
    /// picture `pw` by `ph` (premultiplied linear light, the SDR white of
    /// `white` nits at 1.0), where the device is (`shell`, how much of each
    /// of the picture's pixels it covers: the halo lands on it, not past it).
    /// An LED gives off as much light as its color has, as the keys' do:
    /// white the most, a red about a fifth.
    #[allow(clippy::too_many_arguments)]
    pub fn add_to(&self, px: &mut [f32], pw: usize, ph: usize, rgb: [u8; 3], strength: f32, white: f32, shell: &[f32]) {
        let s = strength.clamp(0.0, 1.0) / white.max(1.0);
        let col = [lin(rgb[0]) * s, lin(rgb[1]) * s, lin(rgb[2]) * s];
        if col == [0.0; 3] {
            return;
        }
        for r in 0..self.h {
            let y = self.y + r as i32;
            if y < 0 || y >= ph as i32 {
                continue;
            }
            for c in 0..self.w {
                let x = self.x + c as i32;
                let n = self.nits[r * self.w + c];
                if x < 0 || x >= pw as i32 || n <= 0.0 {
                    continue;
                }
                let j = y as usize * pw + x as usize;
                let on = shell.get(j).copied().unwrap_or(1.0);
                for k in 0..3 {
                    px[j * 4 + k] += col[k] * n * on;
                }
            }
        }
    }

    /// What is brighter than the screen shows in its box, drawn at the most
    /// it shows, `ceiling` times the white, each pixel in its own color, as
    /// the keys' light is (within_peak).
    pub fn clip(&self, px: &mut [f32], pw: usize, ph: usize, ceiling: f32) {
        for r in 0..self.h {
            let y = self.y + r as i32;
            if y < 0 || y >= ph as i32 {
                continue;
            }
            for c in 0..self.w {
                let x = self.x + c as i32;
                if x < 0 || x >= pw as i32 || self.nits[r * self.w + c] <= 0.0 {
                    continue;
                }
                let i = (y as usize * pw + x as usize) * 4;
                within_peak(&mut px[i..i + 4], ceiling);
            }
        }
    }
}

/// A map blurred as light spreads, a gaussian `sigma` pixels across, in two
/// passes, its edges taken as empty.
fn blur(v: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
    let r = (3.0 * sigma).ceil() as i32;
    let kernel: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let norm: f32 = kernel.iter().sum();
    let kernel: Vec<f32> = kernel.iter().map(|k| k / norm).collect();
    let mut across = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for (j, k) in kernel.iter().enumerate() {
                let xx = x as i32 + j as i32 - r;
                if xx >= 0 && (xx as usize) < w {
                    s += k * v[y * w + xx as usize];
                }
            }
            across[y * w + x] = s;
        }
    }
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for (j, k) in kernel.iter().enumerate() {
                let yy = y as i32 + j as i32 - r;
                if yy >= 0 && (yy as usize) < h {
                    s += k * across[yy as usize * w + x];
                }
            }
            out[y * w + x] = s;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A round window 10 mm across, 20 pixels to the millimeter.
    fn disc() -> (Vec<f32>, usize) {
        let n = 240;
        let mut c = vec![0f32; n * n];
        for (i, v) in c.iter_mut().enumerate() {
            let (x, y) = ((i % n) as f32 + 0.5 - 120.0, (i / n) as f32 + 0.5 - 120.0);
            *v = (100.0 - (x * x + y * y).sqrt() + 0.5).clamp(0.0, 1.0);
        }
        (c, n)
    }

    /// All that passes leaves the window, and the halo's share round it:
    /// the light is as much as the LED's lumens make it, no more, no less.
    #[test]
    fn a_window_gives_off_what_its_led_lets_through() {
        let (c, n) = disc();
        let m = Make { lumens: 0.2, through: 0.5, depth: 3.0, halo: 0.1, reach: 1.0, east: 0.0, north: 0.0 };
        let lamp = Lamp::new(&c, n, n, 0, 0, 20.0, &m);
        let got = lamp.lumens(20.0);
        assert!((got - 0.2 * 0.6).abs() < 1e-3, "{got} lumens");
        // a window as wide evenly lit would glow at Φ·τ / (π A)
        let even = 0.2 * 0.5 / (std::f32::consts::PI * std::f32::consts::PI * 0.005f32.powi(2));
        let middle = lamp.at(n / 2 + lamp.w / 2 - n / 2, lamp.h / 2);
        assert!(middle > even, "brightest over the LED: {middle} against {even} evenly");
        assert!(lamp.at(lamp.w / 2 + 99, lamp.h / 2) < middle, "dimmer at the edge");
    }

    /// The LED moved east lights the window's east side most; deeper, the
    /// light is more even across the window; no halo, no light past it.
    #[test]
    fn where_and_how_deep_the_led_is_shape_the_light() {
        let (c, n) = disc();
        let at = |m: Make| {
            let l = Lamp::new(&c, n, n, 0, 0, 20.0, &m);
            let (ox, oy) = ((l.w - n) / 2, (l.h - n) / 2);
            (l.at(ox + 60, oy + 120), l.at(ox + 180, oy + 120), l.at(ox + 120, oy + 120), l.at(ox + 120 - 105, oy + 120))
        };
        let base = Make { halo: 0.0, depth: 3.0, ..Make::DEFAULT };
        let (west, east, _, _) = at(Make { east: 3.0, ..base });
        assert!(east > west * 1.5, "east {east} west {west}");
        let (w1, _, m1, _) = at(Make { depth: 1.0, ..base });
        let (w9, _, m9, _) = at(Make { depth: 9.0, ..base });
        assert!(w9 / m9 > w1 / m1, "deeper is more even");
        let (_, _, _, past) = at(base);
        assert_eq!(past, 0.0, "nothing past the window without a halo");
        let (_, _, _, halo) = at(Make { halo: 0.1, reach: 2.0, ..base });
        assert!(halo > 0.0, "the halo past its edge");
    }

    /// A light's make: its own, then every window's, then the default.
    #[test]
    fn a_lights_make_comes_from_the_core() {
        let l = serde_json::json!({ "window": { "lumens": 0.3, "depth": 2.0 }, "keys": { "logo": { "window": { "lumens": 0.5, "east": 1.0, "north": -2.0 } } } });
        let logo = Make::of(&l, "logo");
        assert_eq!((logo.lumens, logo.depth, logo.east, logo.north, logo.through), (0.5, 2.0, 1.0, -2.0, Make::DEFAULT.through));
        assert_eq!(Make::of(&l, "wheel").lumens, 0.3);
        assert_eq!(Make::of(&Value::Null, "wheel"), Make::DEFAULT);
    }
}
