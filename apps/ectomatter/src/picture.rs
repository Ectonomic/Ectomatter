//! The window's picture: the page as Slint drew it, with the keyboard's
//! picture in the page's hole, met in linear light and encoded as the
//! compositor is given it: premultiplied PQ with sRGB's primaries (BT.709).

use std::rc::Rc;
use std::sync::Arc;

use ectomatter_draw::hdr::Encoder;
use slint::platform::software_renderer::PremultipliedRgbaColor;

/// The keyboard lit: premultiplied linear RGBA, the desktop's white at 1.0.
pub struct Lit {
    pub px: Vec<f32>,
    pub w: usize,
    pub h: usize,
}

/// What a page lays on the app's ground, under the page and the keyboard:
/// the pads drawn on Home's desk, dots, as the desk shows them while it is
/// arranged, and soft highlights under what the pointer is over. All in the
/// buffer's pixels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decor {
    pub pads: Vec<PadLaid>,
    pub dots: Option<Dots>,
    pub plates: Vec<Plate>,
}

/// A pad on the ground, as a desk mat lies on a desk: its box and its
/// corners' radius, its color (sRGB), the person's picture filling it, if
/// it has one, and the box it shows only inside (the desk's room).
#[derive(Clone, Debug)]
pub struct PadLaid {
    pub rect: [f32; 4],
    pub radius: f32,
    pub color: [u8; 3],
    pub picture: Option<Arc<Photo>>,
    pub bounds: [f32; 4],
}

impl PartialEq for PadLaid {
    fn eq(&self, o: &PadLaid) -> bool {
        let same = match (&self.picture, &o.picture) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        same && self.rect == o.rect && self.radius == o.radius && self.color == o.color && self.bounds == o.bounds
    }
}

/// A picture of the person's own, as a pad shows it: premultiplied sRGB, at
/// its size and at each half of that down to a few pixels, so drawn smaller
/// it is smoothed as it should be rather than shimmering.
pub struct Photo {
    levels: Vec<(usize, usize, Vec<[u8; 4]>)>,
}

impl std::fmt::Debug for Photo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let (w, h) = self.size();
        write!(f, "Photo({w}x{h})")
    }
}

impl Photo {
    /// The picture from its pixels (premultiplied sRGB, `w` by `h`).
    pub fn new(w: usize, h: usize, px: Vec<[u8; 4]>) -> Photo {
        let mut levels = vec![(w.max(1), h.max(1), px)];
        loop {
            let (lw, lh, lp) = &levels[levels.len() - 1];
            if *lw < 8 || *lh < 8 {
                break;
            }
            let (lw, nw, nh) = (*lw, lw / 2, lh / 2);
            let mut next = vec![[0u8; 4]; nw * nh];
            for y in 0..nh {
                for x in 0..nw {
                    let at = |dx: usize, dy: usize| lp[(2 * y + dy) * lw + 2 * x + dx];
                    let four = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
                    next[y * nw + x] = [0, 1, 2, 3].map(|c| ((four.iter().map(|p| p[c] as u32).sum::<u32>() + 2) / 4) as u8);
                }
            }
            levels.push((nw, nh, next));
        }
        Photo { levels }
    }

    /// Its size, in its own pixels.
    pub fn size(&self) -> (usize, usize) {
        (self.levels[0].0, self.levels[0].1)
    }

    /// Its color at a point of it (`u`, `v`, in its own pixels), drawn at
    /// `scale` of its size: premultiplied sRGB, 0 to 1.
    fn at(&self, u: f32, v: f32, scale: f32) -> [f32; 4] {
        let k = ((1.0 / scale.max(1e-6)).log2().floor().max(0.0) as usize).min(self.levels.len() - 1);
        let (w, h, px) = &self.levels[k];
        let d = (1usize << k) as f32;
        let fx = (u / d - 0.5).clamp(0.0, (*w - 1) as f32);
        let fy = (v / d - 0.5).clamp(0.0, (*h - 1) as f32);
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        [0, 1, 2, 3].map(|c| {
            let p = |x: usize, y: usize| px[y * w + x][c] as f32;
            let top = p(x0, y0) + (p(x1, y0) - p(x0, y0)) * tx;
            let bottom = p(x0, y1) + (p(x1, y1) - p(x0, y1)) * tx;
            (top + (bottom - top) * ty) / 255.0
        })
    }
}

/// A dot every `step` pixels from `(ox, oy)`, `r` round, white at `alpha`
/// over the ground, only inside `bounds` and outside each of `holes`.
#[derive(Clone, Debug, PartialEq)]
pub struct Dots {
    pub ox: f32,
    pub oy: f32,
    pub step: f32,
    pub r: f32,
    pub alpha: f32,
    pub bounds: [f32; 4],
    pub holes: Vec<[f32; 4]>,
}

/// A box with rounded corners, white at `alpha` over the ground.
#[derive(Clone, Debug, PartialEq)]
pub struct Plate {
    pub rect: [f32; 4],
    pub radius: f32,
    pub alpha: f32,
}

impl Decor {
    /// The ground at a pixel with the pads on it, in sRGB's encoding (0 to
    /// 1), from the ground's own (`ground`); none where no pad is.
    fn padded(&self, px: f32, py: f32, ground: [f32; 3]) -> Option<[f32; 3]> {
        let mut out: Option<[f32; 3]> = None;
        for p in &self.pads {
            let [x0, y0, x1, y1] = p.rect;
            let [b0, b1, b2, b3] = p.bounds;
            if px < x0 - 1.0 || px > x1 + 1.0 || py < y0 - 1.0 || py > y1 + 1.0 || px < b0 || px >= b2 || py < b1 || py >= b3 {
                continue;
            }
            let c = rounded_cover(p.rect, p.radius, px, py);
            if c <= 0.0 {
                continue;
            }
            let pad = p.color.map(|v| v as f32 / 255.0);
            // the picture filling the pad, as a picture set to cover does, its
            // middle at the pad's: over the pad's color where it's see-through
            let e = match &p.picture {
                Some(photo) => {
                    let (iw, ih) = photo.size();
                    let (pw, ph) = (x1 - x0, y1 - y0);
                    let scale = (pw / iw as f32).max(ph / ih as f32);
                    let (ox, oy) = (x0 + (pw - iw as f32 * scale) / 2.0, y0 + (ph - ih as f32 * scale) / 2.0);
                    let q = photo.at((px - ox) / scale, (py - oy) / scale, scale);
                    [0, 1, 2].map(|i| q[i] + pad[i] * (1.0 - q[3]))
                }
                None => pad,
            };
            let under = out.unwrap_or(ground);
            out = Some([0, 1, 2].map(|i| under[i] + (e[i] - under[i]) * c));
        }
        out
    }

    /// How much white it lays over a pixel's ground, in sRGB's encoding, 0 to 1.
    fn white_at(&self, px: f32, py: f32) -> f32 {
        let mut a = 0f32;
        for p in &self.plates {
            let c = rounded_cover(p.rect, p.radius, px, py);
            a += (1.0 - a) * c * p.alpha;
        }
        if let Some(d) = &self.dots {
            let (lx, ly) = (d.ox + ((px - d.ox) / d.step).round() * d.step, d.oy + ((py - d.oy) / d.step).round() * d.step);
            let [b0, b1, b2, b3] = d.bounds;
            if lx >= b0 && lx <= b2 && ly >= b1 && ly <= b3 && !d.holes.iter().any(|h| lx >= h[0] && lx <= h[2] && ly >= h[1] && ly <= h[3]) {
                let dist = ((px - lx).powi(2) + (py - ly).powi(2)).sqrt();
                let c = (d.r + 0.5 - dist).clamp(0.0, 1.0);
                a += (1.0 - a) * c * d.alpha;
            }
        }
        a
    }

    /// Whether anything of it can fall on row `y` (at its pixels' middle).
    fn on_row(&self, py: f32) -> bool {
        self.plates.iter().any(|p| py >= p.rect[1] - 1.0 && py <= p.rect[3] + 1.0)
            || self.pads.iter().any(|p| py >= p.rect[1] - 1.0 && py <= p.rect[3] + 1.0)
            || self.dots.as_ref().is_some_and(|d| {
                let ly = d.oy + ((py - d.oy) / d.step).round() * d.step;
                (py - ly).abs() <= d.r + 1.0 && py >= d.bounds[1] - d.r - 1.0 && py <= d.bounds[3] + d.r + 1.0
            })
    }
}

/// How much of a pixel (its middle at `px`, `py`) a box with rounded corners
/// covers, 0 to 1, its edges soft over a pixel.
fn rounded_cover([x0, y0, x1, y1]: [f32; 4], radius: f32, px: f32, py: f32) -> f32 {
    let r = radius.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0).max(0.0);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (qx, qy) = ((px - cx).abs() - ((x1 - x0) / 2.0 - r), (py - cy).abs() - ((y1 - y0) / 2.0 - r));
    let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
    (0.5 - d).clamp(0.0, 1.0)
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
    /// far corner, which is rounded: where it starts, its radius, its color
    page_ground: Ground,
    /// where the keyboard goes, the picture it was drawn as, and that picture
    /// at the hole's size (itself, or stretched while the right size is drawn)
    rect: (i32, i32, usize, usize),
    src: Option<Rc<Lit>>,
    shown: Option<Rc<Lit>>,
    /// what is laid on the ground; how much of the ground shows in each of the
    /// keyboard's pixels (as its picture was drawn, see Board::ground_share),
    /// and that at the hole's size as the picture shows there, for what is laid
    /// on the ground to reach under it; and how strongly the keyboard shows (1,
    /// or less for one not plugged in)
    decor: Decor,
    share: Option<Rc<Vec<f32>>>,
    share_shown: Option<Rc<Vec<f32>>>,
    dim: f32,
    /// the mouse, laid over the keyboard and the ground as its own picture's
    /// coverage has it: where it goes, and its picture at that size
    mouse_rect: (i32, i32, usize, usize),
    mouse: Option<Rc<Lit>>,
}

impl Picture {
    pub fn new(reference: f64, ground: [u8; 3]) -> Picture {
        let mut lin = [0f32; 256];
        for (i, v) in lin.iter_mut().enumerate() {
            *v = (i as f32 / 255.0).powf(2.2);
        }
        let ground = [lin[ground[0] as usize], lin[ground[1] as usize], lin[ground[2] as usize]];
        let page_ground = Ground { x: 0, y: 0, radius: 0.0, rgb: [0.0; 3] };
        Picture { w: 0, h: 0, ui: Vec::new(), out: Vec::new(), enc: Encoder::new(reference), reference, lin, ground, page_ground, rect: (0, 0, 0, 0), src: None, shown: None, decor: Decor::default(), share: None, share_shown: None, dim: 1.0, mouse_rect: (0, 0, 0, 0), mouse: None }
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
            self.share_shown = None;
        } else if !same_src || size_changed {
            self.src = lit.cloned();
            self.shown = lit.map(|l| if (l.w, l.h) == (rect.2, rect.3) { l.clone() } else { Rc::new(stretch(l, rect.2, rect.3)) });
            self.fit_share();
        }
        true
    }

    pub fn mouse_rect(&self) -> (i32, i32, usize, usize) {
        self.mouse_rect
    }

    /// Put the mouse at `rect` as `lit` shows it (a picture of the place's
    /// size; another is stretched to it); true if what shows there changed.
    pub fn set_mouse(&mut self, rect: (i32, i32, usize, usize), lit: Option<&Rc<Lit>>) -> bool {
        let same = match (&self.mouse, lit) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same && rect == self.mouse_rect {
            return false;
        }
        self.mouse_rect = rect;
        self.mouse = lit.filter(|_| rect.2 > 0 && rect.3 > 0).map(|l| if (l.w, l.h) == (rect.2, rect.3) { l.clone() } else { Rc::new(stretch(l, rect.2, rect.3)) });
        true
    }

    /// Lay `decor` on the ground; the box to draw again, if it changed: what
    /// changed of it, where it was and where it is (a highlight come under
    /// the pointer draws its own box again, not the pads it lies over).
    pub fn set_decor(&mut self, decor: Decor) -> Option<(usize, usize, usize, usize)> {
        if decor == self.decor {
            return None;
        }
        let (old, new) = (&self.decor, &decor);
        let mut boxes: Vec<[f32; 4]> = Vec::new();
        if old.pads != new.pads {
            boxes.extend(old.pads.iter().chain(&new.pads).map(|p| p.rect));
        }
        if old.dots != new.dots {
            boxes.extend(old.dots.iter().chain(&new.dots).map(|d| d.bounds));
        }
        if old.plates != new.plates {
            boxes.extend(old.plates.iter().chain(&new.plates).map(|p| p.rect));
        }
        let both = boxes.into_iter().reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]);
        self.decor = decor;
        // (a pixel past each side, for the dots' and the corners' soft edges)
        both.map(|[x0, y0, x1, y1]| {
            let (x0, y0) = ((x0 - 2.0).max(0.0) as usize, (y0 - 2.0).max(0.0) as usize);
            let (x1, y1) = (((x1 + 2.0).max(0.0) as usize).max(x0), ((y1 + 2.0).max(0.0) as usize).max(y0));
            (x0, y0, x1 - x0, y1 - y0)
        })
    }

    /// How much of the ground shows in each of the keyboard's pixels, and how
    /// strongly the keyboard shows; true if either changed.
    pub fn set_keyboard_ground(&mut self, share: Option<Rc<Vec<f32>>>, dim: f32) -> bool {
        let same = match (&self.share, &share) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same && (dim - self.dim).abs() < 1e-4 {
            return false;
        }
        self.share = share;
        self.dim = dim;
        if !same {
            self.fit_share();
        }
        true
    }

    /// The ground's share at the hole's size, as the keyboard's picture shows
    /// there: its own, or stretched with the picture (none for another
    /// picture's, or none given).
    fn fit_share(&mut self) {
        let (w, h) = (self.rect.2, self.rect.3);
        self.share_shown = match (&self.share, &self.src) {
            (Some(v), Some(l)) if v.len() == l.w * l.h && w > 0 && h > 0 => Some(if (l.w, l.h) == (w, h) { v.clone() } else { Rc::new(stretch_px(v, l.w, l.h, 1, w, h)) }),
            _ => None,
        };
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
        let Picture { w: stride, h: height, ui, out, enc, lin, ground, page_ground, rect, shown, decor, share_shown, dim, mouse_rect, mouse, .. } = self;
        let (stride, height, ui, enc, lin, ground, rect, shown, decor, dim) = (*stride, *height, &*ui, &*enc, &*lin, *ground, *rect, shown.as_deref(), &*decor, *dim);
        let page_ground = &*page_ground;
        let share = share_shown.as_deref().map(|v| v.as_slice()).filter(|v| shown.is_some() && v.len() == rect.2 * rect.3);
        let kb = KbGround { share, dim };
        let ms = MouseOver { rect: *mouse_rect, lit: mouse.as_deref() };
        // the rows being encoded, split in bands, one to a thread
        let out = &mut out[y0 * stride..y1 * stride];
        std::thread::scope(|s| {
            for (i, chunk) in out.chunks_mut(band * stride).enumerate() {
                let top = y0 + i * band;
                s.spawn(move || {
                    for (r, row) in chunk.chunks_mut(stride).enumerate() {
                        encode_row(row, top + r, x0, x1, (stride, height), ui, enc, lin, ground, page_ground, rect, shown, decor, kb, ms);
                    }
                });
            }
        });
    }
}

/// The keyboard on the ground: how much of the ground shows in each of its
/// pixels, and how strongly it shows.
#[derive(Clone, Copy)]
struct KbGround<'a> {
    share: Option<&'a [f32]>,
    dim: f32,
}

/// The mouse over the keyboard and the ground: where, and its picture there.
#[derive(Clone, Copy)]
struct MouseOver<'a> {
    rect: (i32, i32, usize, usize),
    lit: Option<&'a Lit>,
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
    decor: &Decor,
    kbg: KbGround,
    ms: MouseOver,
) {
    let (kx, ky, kw, kh) = rect;
    let (mx, my, mw, mh) = ms.rect;
    let mouse_rows = ms.lit.filter(|_| mw > 0 && (y as i32) >= my && (y as i32) < my + mh as i32);
    let g = page_ground;
    // the ground with what is laid on it, where anything is: white over it, mixed
    // in sRGB's encoding as the page's colors are
    let py = y as f32 + 0.5;
    let decorated = decor.on_row(py);
    let ground_e = g.rgb.map(|c| c.max(0.0).powf(1.0 / 2.2));
    let laid = |x: usize| -> Option<[f32; 3]> {
        if !decorated {
            return None;
        }
        let px = x as f32 + 0.5;
        let padded = decor.padded(px, py, ground_e);
        let a = decor.white_at(px, py);
        if padded.is_none() && a <= 0.0 {
            return None;
        }
        Some(padded.unwrap_or(ground_e).map(|e| (e + a * (1.0 - e)).powf(2.2)))
    };
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
    // most of the page is long runs of one color: encode each run once (and
    // each run of one color over one color laid on the ground, as a pad is)
    let mut last: Option<((u32, bool), [u16; 4])> = None;
    let mut last_laid: Option<((u32, [u32; 3], u32), [u16; 4])> = None;
    for x in x0..x1 {
        let p = ui[y * stride + x];
        // (the keyboard lies on the app's ground: none of it shows past it, under
        // the title bar or the rail, or past the window's rounded corner)
        let kc = if in_rows && (x as i32) >= kx && (x as i32) < kx + kw as i32 { cover(x) } else { 0.0 };
        let mouse = mouse_rows.filter(|_| (x as i32) >= mx && (x as i32) < mx + mw as i32);
        let mc = if mouse.is_some() { cover(x) } else { 0.0 };
        if kc <= 0.0 && mc <= 0.0 {
            // the page over the app's ground, where the page is not opaque
            let c = if p.alpha == 255 { 1.0 } else { cover(x) };
            if c > 0.0 && p.alpha < 255 {
                if let Some(gd) = laid(x) {
                    let key = (u32::from_le_bytes([p.red, p.green, p.blue, p.alpha]), gd.map(f32::to_bits), c.to_bits());
                    row[x] = match last_laid {
                        Some((k, v)) if k == key => v,
                        _ => {
                            let v = enc.pixel(over(p, gd, c));
                            last_laid = Some((key, v));
                            v
                        }
                    };
                    continue;
                }
            }
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
        // the page over the keyboard (or, where the keyboard isn't, the ground the mouse lies on)
        if kc <= 0.0 {
            let under = laid(x).unwrap_or(g.rgb);
            let (k, ka) = mouse_over(mouse, mx, my, y, x, mc, under, mc);
            row[x] = if p.alpha == 0 { enc.straight(k, ka) } else { enc.pixel(over(p, k, ka)) };
            continue;
        }
        let (mut k, ka) = match kb {
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
        // what is laid on the ground, reaching under the keyboard as far as the
        // ground shows there; and the keyboard dimmed over its ground
        if let (Some(share), Some(l)) = (kbg.share, kb) {
            let j = ((y as i32 - ky) as usize) * l.w + (x as i32 - kx) as usize;
            let s = share[j];
            // (none of the ground shows here, nor what is laid on it: not worked out)
            let gd = if s > 0.0 { laid(x).unwrap_or(g.rgb) } else { g.rgb };
            if gd != g.rgb {
                for c in 0..3 {
                    k[c] += (gd[c] - g.rgb[c]) * s;
                }
            }
            if kbg.dim < 1.0 {
                for c in 0..3 {
                    let under = gd[c] * s;
                    k[c] = under + kbg.dim * (k[c] - under);
                }
            }
        }
        let ka = ka * kc;
        let (k, ka) = if mc > 0.0 { mouse_over(mouse, mx, my, y, x, mc, k, ka) } else { (k, ka) };
        row[x] = if p.alpha == 0 { enc.straight(k, ka) } else { enc.pixel(over(p, k, ka)) };
    }
}

/// The mouse's pixel at `x`, `y` (`mc` of it on the app's ground) laid over
/// what is under it (`under`, straight linear light, `ua` its alpha): the
/// mixed color, straight, and its alpha.
#[allow(clippy::too_many_arguments)]
fn mouse_over(mouse: Option<&Lit>, mx: i32, my: i32, y: usize, x: usize, mc: f32, under: [f32; 3], ua: f32) -> ([f32; 3], f32) {
    let Some(l) = mouse else { return (under, ua) };
    let i = (((y as i32 - my) as usize) * l.w + (x as i32 - mx) as usize) * 4;
    let ma = l.px[i + 3] * mc;
    let a = ma + ua * (1.0 - ma);
    if a <= 0.0 {
        return ([0.0; 3], 0.0);
    }
    let k = [0, 1, 2].map(|c| (l.px[i + c] * mc + under[c] * ua * (1.0 - ma)) / a);
    (k, a)
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
    Lit { px: stretch_px(&l.px, l.w, l.h, 4, w, h), w, h }
}

/// Pixels of `n` channels each, `lw` by `lh`, stretched smoothly to `w` by `h`.
fn stretch_px(from: &[f32], lw: usize, lh: usize, n: usize, w: usize, h: usize) -> Vec<f32> {
    let mut px = vec![0f32; w * h * n];
    let (sx, sy) = (lw as f32 / w as f32, lh as f32 / h as f32);
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * sy - 0.5).clamp(0.0, (lh - 1) as f32);
        let (y0, ty) = (fy as usize, fy.fract());
        let y1 = (y0 + 1).min(lh - 1);
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * sx - 0.5).clamp(0.0, (lw - 1) as f32);
            let (x0, tx) = (fx as usize, fx.fract());
            let x1 = (x0 + 1).min(lw - 1);
            for c in 0..n {
                let at = |xx: usize, yy: usize| from[(yy * lw + xx) * n + c];
                let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
                let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
                px[(y * w + x) * n + c] = top * (1.0 - ty) + bottom * ty;
            }
        }
    }
    px
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dots laid on the ground come out whole and lighter than it, only where
    /// their bounds let them, and not in a hole; a highlight lifts the ground
    /// under it, rounded at its corners.
    #[test]
    fn the_ground_takes_dots_and_a_highlight() {
        let mut p = Picture::new(254.0, [0x18, 0x18, 0x1b]);
        p.resize(120, 80);
        p.set_ground(0, 0, 0.0, [0x18, 0x18, 0x1b]);
        p.set_decor(Decor {
            pads: Vec::new(),
            dots: Some(Dots { ox: 10.0, oy: 10.0, step: 20.0, r: 1.25, alpha: 0.3, bounds: [16.0, 16.0, 104.0, 64.0], holes: vec![[45.0, 45.0, 55.0, 55.0]] }),
            plates: vec![Plate { rect: [0.0, 0.0, 30.0, 12.0], radius: 6.0, alpha: 0.2 }],
        });
        p.encode_all();
        let at = |x: usize, y: usize| p.out[y * 120 + x][0];
        let ground = at(40, 40);
        assert!(at(30, 30) > ground, "a dot inside the bounds");
        assert_eq!(at(10, 30), at(11, 39), "none outside them (10 is before 16)");
        assert!(at(10, 30) <= ground + 1);
        assert_eq!(at(50, 50), ground, "none in the hole");
        assert!(at(30, 30) > at(31, 31) && at(31, 31) >= ground, "soft at its edge");
        assert!(at(15, 6) > ground, "the highlight lifts the ground");
        assert_eq!(at(0, 0), at(119, 79), "but not at its rounded corner");
    }

    /// A pad lies on the ground in its color, rounded at its corners, and
    /// only inside its bounds; a picture fills it as a picture set to cover
    /// does, its middle at the pad's; the dots lie over it.
    #[test]
    fn a_pad_lies_on_the_ground() {
        let encoded = |pad: PadLaid, dots: bool| {
            let mut p = Picture::new(254.0, [0x18, 0x18, 0x1b]);
            p.resize(100, 60);
            p.set_ground(0, 0, 0.0, [0x18, 0x18, 0x1b]);
            let dots = dots.then(|| Dots { ox: 30.0, oy: 30.0, step: 20.0, r: 1.25, alpha: 0.3, bounds: [0.0, 0.0, 100.0, 60.0], holes: Vec::new() });
            p.set_decor(Decor { pads: vec![pad], dots, plates: Vec::new() });
            p.encode_all();
            p.out
        };
        let pad = PadLaid { rect: [10.0, 10.0, 90.0, 50.0], radius: 8.0, color: [200, 40, 10], picture: None, bounds: [0.0, 0.0, 70.0, 60.0] };
        let out = encoded(pad.clone(), false);
        let at = |x: usize, y: usize| out[y * 100 + x];
        let ground = at(5, 5);
        assert!(at(40, 20)[0] > ground[0] && at(40, 20)[0] > at(40, 20)[2], "the pad's red on the ground");
        assert_eq!(at(10, 10), ground, "not at its rounded corner");
        assert_eq!(at(80, 20), ground, "nor past its bounds");
        assert_eq!(at(40, 20), at(55, 44), "one color across it");
        let dotted = encoded(pad.clone(), true);
        assert!(dotted[30 * 100 + 30][1] > out[30 * 100 + 30][1], "a dot over it");
        // a picture twice as wide as high, red then blue, on a square pad
        let photo = Arc::new(Photo::new(2, 1, vec![[255, 0, 0, 255], [0, 0, 255, 255]]));
        let square = PadLaid { rect: [10.0, 10.0, 50.0, 50.0], picture: Some(photo), bounds: [0.0, 0.0, 100.0, 60.0], ..pad };
        let out = encoded(square, false);
        let (left, right) = (out[30 * 100 + 15], out[30 * 100 + 45]);
        assert!(left[0] > left[2] && right[2] > right[0], "red at its left, blue at its right: {left:?} {right:?}");
    }

    /// A picture drawn smaller is smoothed from its halves: a fine checker
    /// of black and white comes out gray, not black or white.
    #[test]
    fn a_picture_drawn_smaller_is_smooth() {
        let px: Vec<[u8; 4]> = (0..64 * 64).map(|i| if (i % 64 + i / 64) % 2 == 0 { [255; 4] } else { [0, 0, 0, 255] }).collect();
        let photo = Photo::new(64, 64, px);
        let c = photo.at(31.3, 17.8, 0.125);
        assert!((c[0] - 0.5).abs() < 0.05 && (c[3] - 1.0).abs() < 1e-3, "{c:?}");
        assert_eq!(photo.size(), (64, 64));
    }

    /// A keyboard all one color, `w` by `h`.
    fn plain(rgb: [f32; 3], w: usize, h: usize) -> Rc<Lit> {
        Rc::new(Lit { px: [rgb[0], rgb[1], rgb[2], 1.0].repeat(w * h), w, h })
    }

    /// The keyboard lies on the app's ground: moved past it (under the title
    /// bar or the rail, as Home's desk can carry it), none of it shows there.
    #[test]
    fn the_keyboard_shows_only_on_the_ground() {
        let mut p = Picture::new(254.0, [0x18, 0x18, 0x1b]);
        p.resize(100, 60);
        p.set_ground(20, 10, 0.0, [0x18, 0x18, 0x1b]);
        p.set_keyboard((-10, -5, 80, 50), Some(&plain([1.0, 0.0, 0.0], 80, 50)));
        p.encode_all();
        let at = |x: usize, y: usize| p.out[y * 100 + x];
        assert_eq!(at(5, 30)[3], 0, "nothing under the rail");
        assert_eq!(at(40, 5)[3], 0, "nor under the title bar");
        assert!(at(40, 30)[0] > at(40, 30)[1] && at(40, 30)[3] > 0, "the keyboard on the ground");
        assert_eq!(at(80, 30), at(40, 55), "and the ground past it");
    }

    /// While a picture of another size is stretched to the keyboard's place,
    /// what is laid on the ground still reaches under it as far as the ground
    /// shows, and a keyboard not plugged in still shows dimmed.
    #[test]
    fn a_stretched_keyboard_keeps_its_ground() {
        let encoded = |lit: Rc<Lit>, share: f32, dim: f32, plate: bool| {
            let mut p = Picture::new(254.0, [0x18, 0x18, 0x1b]);
            p.resize(80, 50);
            p.set_ground(0, 0, 0.0, [0x18, 0x18, 0x1b]);
            p.set_keyboard((10, 5, 60, 40), Some(&lit));
            p.set_keyboard_ground(Some(Rc::new(vec![share; lit.w * lit.h])), dim);
            if plate {
                p.set_decor(Decor { pads: Vec::new(), dots: None, plates: vec![Plate { rect: [0.0, 0.0, 80.0, 50.0], radius: 0.0, alpha: 0.2 }] });
            }
            p.encode_all();
            p.out[25 * 80 + 40]
        };
        let g = (0x18 as f32 / 255.0).powf(2.2);
        let open = plain([g, g, g * (0x1b as f32 / 0x18 as f32).powf(2.2)], 30, 20);
        assert!(encoded(open.clone(), 1.0, 1.0, true)[0] > encoded(open, 1.0, 1.0, false)[0], "the highlight under the keyboard's open ground");
        let body = plain([1.0, 0.0, 0.0], 30, 20);
        assert!(encoded(body.clone(), 0.0, 0.5, false)[0] < encoded(body, 0.0, 1.0, false)[0], "dimmed, not plugged in");
    }
}
