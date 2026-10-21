//! Ectodeck's windows' shadows. Ectodeck draws its own title bars, so KWin
//! gives its windows none of its own; KDE's shadow protocol hands KWin a
//! shadow to draw round each window instead: one picture of a window's
//! shadow, cut into its four corners and four sides, and how far it reaches
//! past each edge. A maximized window has none.

use std::collections::HashMap;

use smithay_client_toolkit::reexports::client::backend::ObjectId;
use smithay_client_toolkit::reexports::client::protocol::wl_shm;
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{Proxy, QueueHandle};
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};

pub mod client {
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/kde-shadow.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/kde-shadow.xml");
}
use client::org_kde_kwin_shadow::OrgKdeKwinShadow;
use client::org_kde_kwin_shadow_manager::OrgKdeKwinShadowManager;

/// How far the shadow reaches past the window's left, top, right and bottom
/// edges (it falls downward), and how far its corner pieces reach into the
/// window: past where the shadow along each edge stops changing, so a side's
/// one slice meets them without a step. The softer layer sets it: its 18 px
/// fall and three of its blur's sigmas (72 px) below the top, and 72 px in
/// from each side. The smallest window, 420 high, has room for two.
const REACH: [usize; 4] = [64, 48, 64, 80];
const INNER: usize = 96;
/// The window's corners, as Ectodeck draws them.
const RADIUS: f32 = 10.0;
/// Its two layers, as the approved mock has them: a tight one that defines
/// the edge (2 px down, 6 px of blur, 45% black) and a soft one that gives
/// depth (18 px down, 48 px of blur, 55% black). CSS's blur is twice the
/// Gaussian's sigma.
const LAYERS: [(f32, f32, f32); 2] = [(2.0, 3.0, 0.45), (18.0, 24.0, 0.55)];

/// The eight pieces, made once and kept for every window.
struct Pieces {
    /// left, top-left, top, top-right, right, bottom-right, bottom, bottom-left
    buffers: Vec<Buffer>,
}

pub struct Shadows {
    manager: Option<OrgKdeKwinShadowManager>,
    pieces: Option<Pieces>,
    /// each window's shadow, while it has one
    on: HashMap<ObjectId, OrgKdeKwinShadow>,
}

impl Shadows {
    pub fn new(manager: Option<OrgKdeKwinShadowManager>) -> Shadows {
        Shadows { manager, pieces: None, on: HashMap::new() }
    }

    /// Give a window its shadow, or (maximized, `on` false) take it away.
    pub fn set<D>(&mut self, surface: &WlSurface, on: bool, pool: &mut SlotPool, qh: &QueueHandle<D>)
    where
        D: smithay_client_toolkit::reexports::client::Dispatch<OrgKdeKwinShadow, ()> + 'static,
    {
        let Some(manager) = &self.manager else { return };
        let id = surface.id();
        if !on {
            if let Some(s) = self.on.remove(&id) {
                manager.unset(surface);
                if s.version() >= 2 {
                    s.destroy();
                }
            }
            return;
        }
        if self.on.contains_key(&id) {
            return;
        }
        if self.pieces.is_none() {
            self.pieces = make_pieces(pool);
        }
        let Some(pieces) = &self.pieces else { return };
        let s = manager.create(surface, qh, ());
        let b = |i: usize| pieces.buffers[i].wl_buffer();
        s.attach_left(b(0));
        s.attach_top_left(b(1));
        s.attach_top(b(2));
        s.attach_top_right(b(3));
        s.attach_right(b(4));
        s.attach_bottom_right(b(5));
        s.attach_bottom(b(6));
        s.attach_bottom_left(b(7));
        s.set_left_offset(REACH[0] as f64);
        s.set_top_offset(REACH[1] as f64);
        s.set_right_offset(REACH[2] as f64);
        s.set_bottom_offset(REACH[3] as f64);
        s.commit();
        self.on.insert(id, s);
    }
}

/// The window's side in the picture: large enough that the softer layer's
/// blur is whole along its sides, as round a real window.
const SIDE: usize = 2 * (INNER + 80) + 1;

/// The shadow round a window with Ectodeck's corners, as alpha (0 to 1),
/// `w` by `h`, the window SIDE square at (left, top).
pub fn picture() -> (Vec<f32>, usize, usize) {
    let [l, t, r, b] = REACH;
    let side = SIDE;
    let (w, h) = (l + side + r, t + side + b);
    // the window's shape, its corners rounded
    let mask = |dy: f32| -> Vec<f32> {
        let mut m = vec![0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let (px, py) = (x as f32 + 0.5 - l as f32, y as f32 + 0.5 - t as f32 - dy);
                let s = side as f32;
                // distance outside the rounded square (negative inside)
                let qx = (px - s / 2.0).abs() - (s / 2.0 - RADIUS);
                let qy = (py - s / 2.0).abs() - (s / 2.0 - RADIUS);
                let out = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - RADIUS;
                m[y * w + x] = (0.5 - out).clamp(0.0, 1.0);
            }
        }
        m
    };
    let blur = |m: &[f32], sigma: f32| -> Vec<f32> {
        let k = (sigma * 3.0).ceil() as i32;
        let weights: Vec<f32> = (-k..=k).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
        let sum: f32 = weights.iter().sum();
        let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
            let mut out = vec![0f32; w * h];
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let mut a = 0.0;
                    for (j, wt) in weights.iter().enumerate() {
                        let d = j as i32 - k;
                        let (sx, sy) = if horizontal { (x + d, y) } else { (x, y + d) };
                        if sx >= 0 && sy >= 0 && sx < w as i32 && sy < h as i32 {
                            a += wt * src[sy as usize * w + sx as usize];
                        }
                    }
                    out[y as usize * w + x as usize] = a / sum;
                }
            }
            out
        };
        pass(&pass(m, true), false)
    };
    let mut alpha = vec![0f32; w * h];
    for (dy, sigma, strength) in LAYERS {
        let layer = blur(&mask(dy), sigma);
        for (a, v) in alpha.iter_mut().zip(layer) {
            *a = 1.0 - (1.0 - *a) * (1.0 - v * strength);
        }
    }
    (alpha, w, h)
}

/// The picture cut into KWin's eight pieces, each in a buffer of its own:
/// the corners reach INNER into the window, the sides are one pixel long,
/// taken from the middle of each side.
fn make_pieces(pool: &mut SlotPool) -> Option<Pieces> {
    let (alpha, w, h) = picture();
    let [l, t, r, b] = REACH;
    let (mx, my) = (w / 2, h / 2);
    // (x, y, width, height) of each piece, in the order the protocol's attach requests take
    let rects = [
        (0, my, l, 1),                                      // left
        (0, 0, l + INNER, t + INNER),                       // top-left
        (mx, 0, 1, t),                                      // top
        (w - r - INNER, 0, r + INNER, t + INNER),           // top-right
        (w - r, my, r, 1),                                  // right
        (w - r - INNER, h - b - INNER, r + INNER, b + INNER), // bottom-right
        (mx, h - b, 1, b),                                  // bottom
        (0, h - b - INNER, l + INNER, b + INNER),           // bottom-left
    ];
    let mut buffers = Vec::new();
    for (x0, y0, pw, ph) in rects {
        let (buffer, canvas) = pool.create_buffer(pw as i32, ph as i32, (pw * 4) as i32, wl_shm::Format::Argb8888).ok()?;
        for y in 0..ph {
            for x in 0..pw {
                let a = (alpha[(y0 + y) * w + x0 + x] * 255.0).round() as u8;
                // premultiplied black: blue, green, red, alpha in memory
                let i = (y * pw + x) * 4;
                canvas[i..i + 4].copy_from_slice(&[0, 0, 0, a]);
            }
        }
        buffers.push(buffer);
    }
    Some(Pieces { buffers })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where KWin lays a side piece against a corner piece, the two agree:
    /// each side is one slice of the picture stretched along its edge, so
    /// the corner must reach in to where the shadow along that edge no
    /// longer changes. (Grant, 2026-10-21: "weird shadow coming off the
    /// edge", a step where the left side met the top-left corner, 11 px
    /// down, where the softer layer was still building up.)
    #[test]
    fn the_pieces_meet_without_a_step() {
        let (a, w, h) = picture();
        let [l, t, r, b] = REACH;
        let (mx, my) = (w / 2, h / 2);
        let at = |x: usize, y: usize| a[y * w + x];
        let mut worst = (0.0f32, "");
        let mut check = |name: &'static str, d: f32| {
            if d > worst.0 {
                worst = (d, name);
            }
        };
        for x in 0..l {
            check("left side, top-left corner", (at(x, my) - at(x, t + INNER - 1)).abs());
            check("left side, bottom-left corner", (at(x, my) - at(x, h - b - INNER)).abs());
        }
        for x in w - r..w {
            check("right side, top-right corner", (at(x, my) - at(x, t + INNER - 1)).abs());
            check("right side, bottom-right corner", (at(x, my) - at(x, h - b - INNER)).abs());
        }
        for y in 0..t {
            check("top side, top-left corner", (at(mx, y) - at(l + INNER - 1, y)).abs());
            check("top side, top-right corner", (at(mx, y) - at(w - r - INNER, y)).abs());
        }
        for y in h - b..h {
            check("bottom side, bottom-left corner", (at(mx, y) - at(l + INNER - 1, y)).abs());
            check("bottom side, bottom-right corner", (at(mx, y) - at(w - r - INNER, y)).abs());
        }
        eprintln!("the worst seam: {} differ by {:.2} levels of 255", worst.1, worst.0 * 255.0);
        // (under half a level of the pieces' eight bits)
        assert!(worst.0 < 0.5 / 255.0, "{} differ by {:.1} levels of 255", worst.1, worst.0 * 255.0);
    }

    #[test]
    fn the_shadow_falls_downward_and_fades_out() {
        let (a, w, h) = picture();
        let [l, t, _, _] = REACH;
        let side = SIDE;
        let at = |x: usize, y: usize| a[y * w + x];
        // darker just under the window than just over it, and nothing at the picture's edges
        let mid = l + side / 2;
        assert!(at(mid, t + side + 8) > at(mid, t - 8) * 1.5, "below {} above {}", at(mid, t + side + 8), at(mid, t - 8));
        assert!(at(0, 0) < 0.01 && at(w - 1, h - 1) < 0.02 && at(mid, h - 1) < 0.02);
        // under the window it is about as dark as the two layers together
        assert!((at(mid, t + side / 2) - (1.0 - 0.55 * 0.45)).abs() < 0.05);
    }
}
