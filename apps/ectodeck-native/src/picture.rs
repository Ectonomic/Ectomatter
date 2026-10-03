//! The window's picture: the page as Slint drew it, with the keyboard's
//! picture in the page's hole, met in linear light and encoded as the
//! compositor is given it: premultiplied PQ with BT.2020 primaries.

use std::rc::Rc;

use ectodeck_draw::hdr::Encoder;
use slint::platform::software_renderer::PremultipliedRgbaColor;

/// The keyboard lit: premultiplied linear RGBA, the desktop's white at 1.0.
pub struct Lit {
    pub px: Vec<f32>,
    pub w: usize,
    pub h: usize,
}

pub struct Picture {
    pub w: usize,
    pub h: usize,
    /// the page, as Slint draws it: premultiplied sRGB
    pub ui: Vec<PremultipliedRgbaColor>,
    /// the picture as the compositor is given it
    pub out: Vec<[u16; 4]>,
    enc: Encoder,
    reference: f64,
    /// sRGB to linear light, as KWin decodes SDR apps: gamma 2.2
    lin: [f32; 256],
    /// what the hole shows before the keyboard is drawn: the ground it sits on
    ground: [f32; 3],
    /// the app's ground, laid under the page from a corner to the window's
    /// far corner, which is rounded: where it starts, its radius, its colour
    page_ground: Ground,
    /// where the keyboard goes, the picture it was drawn as, and that picture
    /// at the hole's size (itself, or stretched while the right size is drawn)
    rect: (i32, i32, usize, usize),
    src: Option<Rc<Lit>>,
    shown: Option<Rc<Lit>>,
}

impl Picture {
    pub fn new(reference: f64, ground: [u8; 3]) -> Picture {
        let mut lin = [0f32; 256];
        for (i, v) in lin.iter_mut().enumerate() {
            *v = (i as f32 / 255.0).powf(2.2);
        }
        let ground = [lin[ground[0] as usize], lin[ground[1] as usize], lin[ground[2] as usize]];
        let page_ground = Ground { x: 0, y: 0, radius: 0.0, rgb: [0.0; 3] };
        Picture { w: 0, h: 0, ui: Vec::new(), out: Vec::new(), enc: Encoder::new(reference), reference, lin, ground, page_ground, rect: (0, 0, 0, 0), src: None, shown: None }
    }

    /// Lay the app's ground under the page; true if it changed.
    pub fn set_ground(&mut self, x: usize, y: usize, radius: f32, rgb: [u8; 3]) -> bool {
        let g = Ground { x, y, radius, rgb: [self.lin[rgb[0] as usize], self.lin[rgb[1] as usize], self.lin[rgb[2] as usize]] };
        let changed = g != self.page_ground;
        self.page_ground = g;
        changed
    }

    /// Draw for another desktop white from now on.
    pub fn set_reference(&mut self, reference: f64) {
        self.enc = Encoder::new(reference);
        self.reference = reference;
    }

    pub fn reference(&self) -> f64 {
        self.reference
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        self.w = w;
        self.h = h;
        self.ui = vec![PremultipliedRgbaColor::default(); w * h];
        self.out = vec![[0; 4]; w * h];
    }

    pub fn keyboard_rect(&self) -> (i32, i32, usize, usize) {
        self.rect
    }

    /// Put the keyboard at `rect`; true if what the hole shows has changed.
    pub fn set_keyboard(&mut self, rect: (i32, i32, usize, usize), lit: Option<&Rc<Lit>>) -> bool {
        let same_src = match (&self.src, lit) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same_src && rect == self.rect {
            return false;
        }
        let size_changed = (rect.2, rect.3) != (self.rect.2, self.rect.3);
        self.rect = rect;
        if rect.2 == 0 || rect.3 == 0 {
            self.src = None;
            self.shown = None;
        } else if !same_src || size_changed {
            self.src = lit.cloned();
            self.shown = lit.map(|l| if (l.w, l.h) == (rect.2, rect.3) { l.clone() } else { Rc::new(stretch(l, rect.2, rect.3)) });
        }
        true
    }

    pub fn encode_all(&mut self) {
        self.encode(0, 0, self.w, self.h);
    }

    /// Encode the pixels in a rectangle, on as many threads as it is worth.
    pub fn encode(&mut self, x: usize, y: usize, w: usize, h: usize) {
        let (x0, y0) = (x.min(self.w), y.min(self.h));
        let (x1, y1) = ((x + w).min(self.w), (y + h).min(self.h));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let rows = y1 - y0;
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
        let band = rows.div_ceil(threads).max(24);
        let Picture { w: stride, h: height, ui, out, enc, lin, ground, page_ground, rect, shown, .. } = self;
        let (stride, height, ui, enc, lin, ground, rect, shown) = (*stride, *height, &*ui, &*enc, &*lin, *ground, *rect, shown.as_deref());
        let page_ground = &*page_ground;
        // the rows being encoded, split in bands, one to a thread
        let out = &mut out[y0 * stride..y1 * stride];
        std::thread::scope(|s| {
            for (i, chunk) in out.chunks_mut(band * stride).enumerate() {
                let top = y0 + i * band;
                s.spawn(move || {
                    for (r, row) in chunk.chunks_mut(stride).enumerate() {
                        encode_row(row, top + r, x0, x1, (stride, height), ui, enc, lin, ground, page_ground, rect, shown);
                    }
                });
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn encode_row(
    row: &mut [[u16; 4]],
    y: usize,
    x0: usize,
    x1: usize,
    (stride, height): (usize, usize),
    ui: &[PremultipliedRgbaColor],
    enc: &Encoder,
    lin: &[f32; 256],
    ground: [f32; 3],
    page_ground: &Ground,
    rect: (i32, i32, usize, usize),
    kb: Option<&Lit>,
) {
    let (kx, ky, kw, kh) = rect;
    let g = page_ground;
    // how much of a pixel the app's ground covers: all of it, but at its rounded corner
    let (cx, cy) = (stride as f32 - g.radius, height as f32 - g.radius);
    let cover = |x: usize| -> f32 {
        if x < g.x || y < g.y {
            return 0.0;
        }
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        if g.radius > 0.0 && px > cx && py > cy {
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            return (g.radius - d + 0.5).clamp(0.0, 1.0);
        }
        1.0
    };
    let in_rows = kw > 0 && (y as i32) >= ky && (y as i32) < ky + kh as i32;
    let page = |p: PremultipliedRgbaColor| -> [f32; 4] {
        match p.alpha {
            0 => [0.0; 4],
            255 => [lin[p.red as usize], lin[p.green as usize], lin[p.blue as usize], 1.0],
            a => {
                let un = |c: u8| lin[((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as usize];
                let a = a as f32 / 255.0;
                [un(p.red) * a, un(p.green) * a, un(p.blue) * a, a]
            }
        }
    };
    // The page over what lies under it (the app's ground, the keyboard), mixed
    // as a browser mixes: in sRGB's own encoding, not in linear light, so a
    // faint white glare or a press's shade comes out as it did in the web page.
    // Under is linear RGB, `ua` its alpha; the result is premultiplied linear.
    let over = |p: PremultipliedRgbaColor, under: [f32; 3], ua: f32| -> [f32; 4] {
        let a = p.alpha as f32 / 255.0;
        let t = 1.0 - a;
        let oa = a + ua * t;
        if oa <= 0.0 {
            return [0.0; 4];
        }
        let enc = |v: f32| v.max(0.0).powf(1.0 / 2.2);
        let mix = |c: u8, u: f32| {
            let e = c as f32 / 255.0 + enc(u) * ua * t;
            (e / oa).powf(2.2) * oa
        };
        [mix(p.red, under[0]), mix(p.green, under[1]), mix(p.blue, under[2]), oa]
    };
    // most of the page is long runs of one colour: encode each run once
    let mut last: Option<((u32, bool), [u16; 4])> = None;
    for x in x0..x1 {
        let p = ui[y * stride + x];
        let inside = in_rows && (x as i32) >= kx && (x as i32) < kx + kw as i32;
        if !inside {
            // the page over the app's ground, where the page is not opaque
            let c = if p.alpha == 255 { 1.0 } else { cover(x) };
            if c > 0.0 && c < 1.0 {
                row[x] = enc.pixel(over(p, g.rgb, c));
                continue;
            }
            let key = (u32::from_le_bytes([p.red, p.green, p.blue, p.alpha]), c > 0.0);
            row[x] = match last {
                Some((k, v)) if k == key => v,
                _ => {
                    let v = enc.pixel(if c > 0.0 { over(p, g.rgb, 1.0) } else { page(p) });
                    last = Some((key, v));
                    v
                }
            };
            continue;
        }
        // the page over the keyboard
        let (k, ka) = match kb {
            Some(l) => {
                let i = (((y as i32 - ky) as usize) * l.w + (x as i32 - kx) as usize) * 4;
                let a = l.px[i + 3];
                if a > 0.0 {
                    let r = 1.0 / a;
                    ([l.px[i] * r, l.px[i + 1] * r, l.px[i + 2] * r], a)
                } else {
                    ([0.0; 3], 0.0)
                }
            }
            None => (ground, 1.0),
        };
        row[x] = if p.alpha == 0 { enc.straight(k, ka) } else { enc.pixel(over(p, k, ka)) };
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Ground {
    x: usize,
    y: usize,
    radius: f32,
    rgb: [f32; 3],
}

/// The keyboard stretched to another size, smoothly, to show while it is
/// drawn again at that size.
fn stretch(l: &Lit, w: usize, h: usize) -> Lit {
    let mut px = vec![0f32; w * h * 4];
    let (sx, sy) = (l.w as f32 / w as f32, l.h as f32 / h as f32);
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * sy - 0.5).clamp(0.0, (l.h - 1) as f32);
        let (y0, ty) = (fy as usize, fy.fract());
        let y1 = (y0 + 1).min(l.h - 1);
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * sx - 0.5).clamp(0.0, (l.w - 1) as f32);
            let (x0, tx) = (fx as usize, fx.fract());
            let x1 = (x0 + 1).min(l.w - 1);
            for c in 0..4 {
                let at = |xx: usize, yy: usize| l.px[(yy * l.w + xx) * 4 + c];
                let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
                let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
                px[(y * w + x) * 4 + c] = top * (1.0 - ty) + bottom * ty;
            }
        }
    }
    Lit { px, w, h }
}
