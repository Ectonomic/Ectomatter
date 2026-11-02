//! The Blob behind About: Ectodeck's built-in background (the orb from ectonomic.com) in its
//! Deep look, drawn on the computer's processor. A near-black wet body bent by slow waves,
//! lime only at its rim, in its highlight and in a glow drifting under the skin; a press
//! pokes it, a small dent and a ripple running out across the skin. Ported from Ectodeck's
//! `src/lib/backgrounds/source/blob.frag` (its deep mode, the front of the body only),
//! with About's framing and its fade into the page's gray below, as the mock has them.

use std::time::Duration;

/// How the body is framed and how it moves.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    /// the body's radius, in the scene's units, and where its centre sits on the screen
    pub scale: f32,
    pub offset: [f32; 2],
    /// how hard the skin bends, and how fast
    pub amp: f32,
    pub speed: f32,
    /// the light at the rim and under the skin, and the body's own color (straight RGB, 0 to 1)
    pub glow: [f32; 3],
    pub body: [f32; 3],
    /// the page's ground the body sits on, and where down the frame the body fades into it
    pub ground: [f32; 3],
    pub fade: (f32, f32),
}

impl Look {
    /// About's: the body large and low, its top clear of the bar, fading into the gray
    /// (the mock's scale 1.7, offset 0, -0.35, faded from 58% of the way down to 94%).
    pub fn about(ground: [f32; 3]) -> Look {
        Look { scale: 1.7, offset: [0.0, -0.35], amp: 2.2, speed: 0.45, glow: [0.784, 0.949, 0.18], body: [0.608, 0.878, 0.071], ground, fade: (0.58, 0.94) }
    }
}

/// A press on the body: where, on the screen (-1 to 1 across and up), and how long ago.
#[derive(Clone, Copy, Debug)]
pub struct Poke {
    pub at: [f32; 2],
    pub age: f32,
}

/// How long a poke goes on rippling.
pub const POKE_LIFE: Duration = Duration::from_secs(7);

type V = [f32; 3];
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V, k: f32) -> V {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len(a: V) -> f32 {
    dot(a, a).sqrt()
}
fn norm(a: V) -> V {
    let l = len(a).max(1e-12);
    mul(a, 1.0 / l)
}

const FOV: f32 = 0.42;

/// How finely the body's mesh is cut: around it, and from pole to pole. A side of a piece is
/// under a fifth of a pixel's sag at About's full size.
const AROUND: usize = 128;
const POLE_TO_POLE: usize = 64;

/// One frame's scene: the time, the turn of the body, its framing and its pokes.
struct Scene<'a> {
    look: &'a Look,
    t: f32,
    aspect: f32,
    /// the body's turn, as the shader's mat3 (its columns)
    rot: [V; 3],
    centre: V,
    pokes: &'a [Poke],
    /// the frame's light, and where the glow under the skin is (in the body's own turn)
    light: V,
    glow_in_body: V,
}

impl Scene<'_> {
    /// rot · v, and its transpose · v
    fn turn(&self, v: V) -> V {
        add(add(mul(self.rot[0], v[0]), mul(self.rot[1], v[1])), mul(self.rot[2], v[2]))
    }
    fn unturn(&self, v: V) -> V {
        [dot(self.rot[0], v), dot(self.rot[1], v), dot(self.rot[2], v)]
    }

    /// the body, as orb.js bends it
    fn bump(&self, o: V) -> f32 {
        let t = self.t;
        ((o[0] * 2.1 + t * 0.6).sin() * (o[1] * 1.8 + t * 0.5).cos() * (o[2] * 2.4 + t * 0.7).sin() * 0.09 + (o[0] * 4.3 + o[1] * 3.7 + t * 0.4).sin() * 0.035) * self.look.amp
    }
    fn surf(&self, o: V) -> V {
        mul(o, 1.0 + self.bump(o))
    }
    /// the normal of the bent surface, from two neighbours along it
    fn skin_normal(&self, o: V, e: f32) -> V {
        let s = self.surf(o);
        let ta = norm(cross(o, if o[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] }));
        let tb = cross(o, ta);
        let sa = self.surf(norm(add(o, mul(ta, e))));
        let sb = self.surf(norm(add(o, mul(tb, e))));
        norm(cross(sub(sa, s), sub(sb, s)))
    }

    /// how far a press has pushed the skin at a point of the screen: a dent, and a ripple running out
    fn poke_height(&self, at: [f32; 2]) -> f32 {
        let mut h = 0.0;
        for p in self.pokes.iter().take(4) {
            let age = p.age;
            let (dx, dy) = ((at[0] - p.at[0]) * self.aspect, at[1] - p.at[1]);
            let r = (dx * dx + dy * dy).sqrt();
            let dent = -0.5 * (-r * r / 0.17).exp() * smoothstep(0.0, 0.6, age) * (-age * 0.75).exp();
            let rr = r - 0.25 - age * 0.55;
            let ripple = 0.11 * (-rr * rr / 0.04).exp() * (-age * 0.4).exp() * smoothstep(0.0, 0.5, age);
            h += dent + ripple;
        }
        h
    }

    /// the ray through a point of the screen, from the camera at the origin
    fn ray_at(&self, ndc: [f32; 2]) -> V {
        let k = FOV.tan();
        norm([ndc[0] * k * self.aspect, ndc[1] * k, -1.0])
    }
    fn to_body(&self, x: V) -> V {
        mul(self.unturn(sub(x, self.centre)), 1.0 / self.look.scale)
    }

    /// The color where the ray `d` through a point of the frame (its place across and down,
    /// 0 to 1) meets the skin `front` along it, with the skin's normal there `n`.
    fn colour(&self, ndc: [f32; 2], uv: [f32; 2], e: [f32; 2], noise: f32, d: V, front: f32, mut n: V) -> V {
        let look = self.look;
        let (lime, deep) = (look.glow, look.body);
        let o = norm(self.to_body(mul(d, front)));
        let mut q = mul(d, front);
        let ph = self.poke_height(ndc);
        if !self.pokes.is_empty() {
            let sx = (self.poke_height([ndc[0] + e[0], ndc[1]]) - self.poke_height([ndc[0] - e[0], ndc[1]])) / (2.0 * e[0]);
            let sy = (self.poke_height([ndc[0], ndc[1] + e[1]]) - self.poke_height([ndc[0], ndc[1] - e[1]])) / (2.0 * e[1]);
            n = norm(sub(n, [sx * 0.4, sy * 0.4, 0.0]));
        }
        let lift = (ph / 0.45).clamp(-1.0, 1.0);
        // (the skin pushed by the pokes, mostly straight back)
        let push = mul(n, ph * look.scale);
        q = add(q, mul(d, dot(push, d)));
        let v = norm(mul(q, -1.0));
        let l = self.light;
        let fres = (1.0 - dot(n, v)).clamp(0.0, 1.0).powf(3.2);
        let diff = dot(n, l).max(0.0);
        let spec = dot(n, norm(add(l, v))).max(0.0).powf(110.0);
        let dg = sub(o, self.glow_in_body);
        let inner = (-dot(dg, dg) * look.scale * look.scale * 1.4).exp();
        let shine = [1.0 + (lime[0] - 1.0) * 0.45, 1.0 + (lime[1] - 1.0) * 0.45, 1.0 + (lime[2] - 1.0) * 0.45];
        let mut c = add(add(add(add(mul(deep, 0.026), mul(deep, diff * 0.09)), mul(lime, fres * 0.62)), mul(lime, inner * 0.5)), mul(shine, spec * 0.55));
        c = mul(c, 1.0 + lift * 0.24);
        let fog = ((q[2] + 7.6) / 4.2).clamp(0.0, 1.0);
        c = mul(c, fog * fog);
        let (ux, uy) = (uv[0] - 0.5, uv[1] - 0.5);
        c = mul(c, (1.0 - (ux * ux + uy * uy) * 1.15).max(0.0));
        let (hx, hy) = (ux, uy * 0.75);
        c = mul(c, 1.0 - 0.4 * (-(hx * hx + hy * hy) * 12.0).exp());
        add(c, [noise, noise, noise])
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Dave Hoskins' hash, as Ectodeck's shaders use it: the grain that keeps the dark from banding.
fn hash(x: f32, y: f32) -> f32 {
    let fr = |v: f32| v - v.floor();
    let mut q = [fr(x * 0.1031), fr(y * 0.1031), fr(x * 0.1031)];
    let k = dot(q, [q[1] + 33.33, q[2] + 33.33, q[0] + 33.33]);
    q = [q[0] + k, q[1] + k, q[2] + k];
    fr((q[0] + q[1]) * q[2])
}

/// A corner of the body's mesh: where it lands in the frame (in pixels from the top left),
/// how near (one over its depth), and the skin's normal there.
#[derive(Clone, Copy, Default)]
struct Corner {
    x: f32,
    y: f32,
    near: f32,
    n: V,
    facing: bool,
}

/// What a pixel sees of the mesh: the nearest piece over it, and where in that piece.
#[derive(Clone, Copy)]
struct Seen {
    near: f32,
    piece: u32,
    b1: f32,
    b2: f32,
}

/// How many rows a band of the frame is: the bands are dealt out to the threads in turn.
const BAND: usize = 8;

fn scene<'a>(look: &'a Look, real: f32, pokes: &'a [Poke], w: usize, h: usize) -> Scene<'a> {
    let (yaw, pitch) = (real * 0.07, 0.22 + (real * 0.19).sin() * 0.10);
    let (cy, sy, cx, sx) = (yaw.cos(), yaw.sin(), pitch.cos(), pitch.sin());
    // the light circles slowly, so the wet highlight sweeps across the body
    let la = real * 0.11;
    let light = norm([la.cos() * 0.9, 0.95 + (real * 0.07).sin() * 0.2, la.sin() * 0.9 + 0.5]);
    // a soft light under the skin, wandering slowly round the point facing the viewer
    let (s, off) = (look.scale, look.offset);
    let glow_at = [-off[0] / s, -off[1] / s, (s * s - (off[0] * off[0] + off[1] * off[1])).max(0.0).sqrt() / s];
    let glow = add(glow_at, [(real * 0.13).sin() * 0.45, (real * 0.09).cos() * 0.40, (real * 0.05).sin() * 0.20]);
    let mut sc = Scene {
        look,
        t: real * look.speed,
        aspect: w as f32 / h.max(1) as f32,
        rot: [[cy, sx * sy, -cx * sy], [0.0, cx, sx], [sy, -sx * cy, cx * cy]],
        centre: [look.offset[0], look.offset[1], -4.2],
        pokes,
        light,
        glow_in_body: [0.0; 3],
    };
    sc.glow_in_body = sc.unturn(glow);
    sc
}

/// A frame of the Blob, `w` by `h`, at `real` seconds, with its pokes: straight RGBA, 8 bits
/// a channel, row by row from the top, drawn across the computer's cores.
///
/// The shader marches each pixel's ray to the skin; here the bent sphere is cut into a mesh
/// (as orb.js builds it) and each piece is laid onto the pixels it covers, nearest kept, so
/// only the body's own pixels cost anything. Each is then lit as the shader lights it, and
/// each pixel along the outline (against the ground, or where a lobe passes in front of the
/// body) takes what it shows of each side by how much of the pixel that side covers.
pub fn frame(look: &Look, real: f32, pokes: &[Poke], w: usize, h: usize) -> Vec<u8> {
    drawn(look, real, pokes, w, h, true)
}

/// A frame `w` by `h` (opaque RGBA) made `to_w` by `to_h`, each pixel blended from the four
/// nearest it (bilinear), as a browser shows a picture on a screen denser than it was drawn
/// for: smoother than Slint's software renderer stretching it, which repeats pixels, for
/// about a ninth of the processor's time drawing the frame that big takes (blob_frame_time:
/// 15 ms to enlarge a 1380 by 936 frame twice, 138 to draw it so).
pub fn enlarged(src: &[u8], w: usize, h: usize, to_w: usize, to_h: usize) -> Vec<u8> {
    let mut out = vec![0u8; to_w * to_h * 4];
    if w == 0 || h == 0 || to_w == 0 || to_h == 0 {
        return out;
    }
    // (for each place along, its two source pixels and the second's weight, in 256ths)
    let along = |n: usize, to: usize| -> Vec<(usize, usize, u32)> {
        (0..to)
            .map(|i| {
                let f = ((i as f32 + 0.5) * n as f32 / to as f32 - 0.5).clamp(0.0, (n - 1) as f32);
                let a = f as usize;
                (a, (a + 1).min(n - 1), ((f - a as f32) * 256.0).round() as u32)
            })
            .collect()
    };
    let (xs, ys) = (along(w, to_w), along(h, to_h));
    // first across: each source row made `to_w` wide once, in 256ths of a level
    let mut wide = vec![0u16; h * to_w * 4];
    for (row, line) in src.chunks_exact(w * 4).zip(wide.chunks_exact_mut(to_w * 4)) {
        for (o, &(a, b, f)) in line.chunks_exact_mut(4).zip(&xs) {
            let (pa, pb) = (&row[a * 4..a * 4 + 4], &row[b * 4..b * 4 + 4]);
            for c in 0..4 {
                o[c] = ((pa[c] as u32 * (256 - f) + pb[c] as u32 * f) >> 4) as u16;
            }
        }
    }
    // then down, each row of the frame from the two widened rows nearest it
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    let per = to_h.div_ceil(threads);
    let wide = &wide;
    std::thread::scope(|s| {
        for (part, rows) in out.chunks_mut(per * to_w * 4).enumerate() {
            let ys = &ys;
            s.spawn(move || {
                for (r, row) in rows.chunks_exact_mut(to_w * 4).enumerate() {
                    let (a, b, f) = ys[part * per + r];
                    let (top, bottom) = (&wide[a * to_w * 4..(a + 1) * to_w * 4], &wide[b * to_w * 4..(b + 1) * to_w * 4]);
                    for ((o, &t), &u) in row.iter_mut().zip(top).zip(bottom) {
                        *o = ((t as u32 * (256 - f) + u as u32 * f + 2048) >> 12) as u8;
                    }
                }
            });
        }
    });
    out
}

/// The points of a pixel along the outline looked at, to see how much of it the body
/// covers, from its centre in sixteenths of a pixel: the standard pattern of 8x
/// multisampling (Direct3D's and Vulkan's), spread so no two share a row or a column.
const COVER: [(f32, f32); 8] = [(1.0, -3.0), (-1.0, 3.0), (5.0, 1.0), (-3.0, -5.0), (-5.0, 5.0), (-7.0, -1.0), (3.0, 7.0), (7.0, -7.0)];

/// What a pixel sees of the mesh, at its centre, or (`smooth`) along the outline by how
/// much of it the body covers. Unsmoothed, its outline is the shader's, which looks at each
/// pixel's centre alone.
fn drawn(look: &Look, real: f32, pokes: &[Poke], w: usize, h: usize, smooth: bool) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    if w == 0 || h == 0 {
        return out;
    }
    let sc = scene(look, real, pokes, w, h);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    let k = FOV.tan();
    let (wf, hf) = (w as f32, h as f32);

    // the corners: the sphere's directions, bent, turned, placed and seen from the camera
    let cols = AROUND + 1;
    let mut corners = vec![Corner::default(); cols * (POLE_TO_POLE + 1)];
    std::thread::scope(|s| {
        let per = (POLE_TO_POLE + 1).div_ceil(threads) * cols;
        for (part, chunk) in corners.chunks_mut(per).enumerate() {
            let sc = &sc;
            s.spawn(move || {
                for (i, c) in chunk.iter_mut().enumerate() {
                    let at = part * per + i;
                    let (row, col) = (at / cols, at % cols);
                    let th = std::f32::consts::PI * row as f32 / POLE_TO_POLE as f32;
                    let ph = std::f32::consts::TAU * col as f32 / AROUND as f32;
                    let o = [th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()];
                    let q = add(sc.centre, sc.turn(mul(sc.surf(o), sc.look.scale)));
                    let n = norm(sc.turn(sc.skin_normal(o, 0.01)));
                    let depth = -q[2];
                    let ndc = [q[0] / depth / (k * sc.aspect), q[1] / depth / k];
                    *c = Corner { x: (ndc[0] + 1.0) * 0.5 * wf, y: hf - (ndc[1] + 1.0) * 0.5 * hf, near: 1.0 / depth, n, facing: dot(n, q) < 0.0 };
                }
            });
        }
    });

    // the pieces, two to each square of the mesh, sorted into the bands they cross; pieces
    // whose corners all face away are behind the front and never seen
    let bands = h.div_ceil(BAND);
    let mut pieces: Vec<[u32; 3]> = Vec::with_capacity(AROUND * POLE_TO_POLE * 2);
    let mut in_band: Vec<Vec<u32>> = vec![Vec::new(); bands];
    for row in 0..POLE_TO_POLE {
        for col in 0..AROUND {
            let (a, b) = ((row * cols + col) as u32, (row * cols + col + 1) as u32);
            let (c, d) = (a + cols as u32, b + cols as u32);
            for tri in [[a, b, d], [a, d, c]] {
                let [p, q, r] = tri.map(|i| corners[i as usize]);
                if !(p.facing || q.facing || r.facing) {
                    continue;
                }
                let (top, bottom) = (p.y.min(q.y).min(r.y), p.y.max(q.y).max(r.y));
                if bottom < 0.0 || top > hf || p.x.max(q.x).max(r.x) < 0.0 || p.x.min(q.x).min(r.x) > wf {
                    continue;
                }
                let id = pieces.len() as u32;
                pieces.push(tri);
                // (a band also lays its pieces on the row over it and the next band's first row:
                // its last row's light is blended from below, and its outline found from both)
                let (first, last) = (((top - 1.0).max(0.0) as usize) / BAND, (((bottom + 1.0).min(hf - 1.0)) as usize) / BAND);
                for band in in_band.iter_mut().take(last + 1).skip(first) {
                    band.push(id);
                }
            }
        }
    }

    // the body's outline as the mesh has it: each side between a piece turned to the camera and
    // one turned away, wherever it lands (against the ground, or a lobe's edge in front of the
    // body behind it), and the pixels it crosses, 64 to a word; and those pixels with the ones
    // round them, where the light isn't blended across from the other side of it
    let words = w.div_ceil(64);
    let (mut outline, mut by_outline) = (vec![0u64; h * words], vec![0u64; h * words]);
    if smooth {
        let turned = |[p, q, r]: [u32; 3]| {
            let [p, q, r] = [p, q, r].map(|i| corners[i as usize]);
            let area = (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
            if area.abs() < 1e-6 { 0 } else { area.signum() as i32 }
        };
        let mut cross = |a: u32, b: u32| {
            let (p, q) = (corners[a as usize], corners[b as usize]);
            let steps = (((q.x - p.x).hypot(q.y - p.y)) * 4.0).ceil() as usize + 1;
            for s in 0..=steps {
                let f = s as f32 / steps as f32;
                let (x, y) = (p.x + (q.x - p.x) * f, p.y + (q.y - p.y) * f);
                if x >= 0.0 && y >= 0.0 && x < wf && y < hf {
                    let (x, y) = (x as usize, y as usize);
                    outline[y * words + x / 64] |= 1u64 << (x % 64);
                }
            }
        };
        let at = |row: usize, col: usize| (row * cols + col % AROUND) as u32;
        for row in 0..POLE_TO_POLE {
            for col in 0..AROUND {
                let (a, b, c, d) = (at(row, col), at(row, col + 1), at(row + 1, col), at(row + 1, col + 1));
                let (up, down) = (turned([a, b, d]), turned([a, d, c]));
                // (the sides of this square's two pieces shared with the square's own, the next
                // square round's and the one below's; the poles' pieces, folded flat, are passed
                // over to the ones beside them)
                let next_down = turned([b, at(row + 1, col + 2), d]);
                let sides = [
                    Some((up, down, a, d)),
                    Some((up, next_down, b, d)),
                    (row + 1 < POLE_TO_POLE).then(|| (down, turned([c, d, at(row + 2, col + 1)]), c, d)),
                    (row == 0).then_some((down, next_down, a, d)),
                    (row + 1 == POLE_TO_POLE).then(|| (up, turned([b, at(row, col + 2), at(row + 1, col + 2)]), b, d)),
                ];
                for (s, t, from, to) in sides.into_iter().flatten() {
                    if s * t < 0 {
                        cross(from, to);
                    }
                }
            }
        }
        let right = |row: &[u64], i: usize| (row[i] << 1) | if i > 0 { row[i - 1] >> 63 } else { 0 };
        let left = |row: &[u64], i: usize| (row[i] >> 1) | if i + 1 < row.len() { row[i + 1] << 63 } else { 0 };
        for y in 0..h {
            for i in 0..words {
                by_outline[y * words + i] = (y.saturating_sub(1)..(y + 2).min(h)).fold(0, |m, yy| {
                    let row = &outline[yy * words..(yy + 1) * words];
                    m | row[i] | right(row, i) | left(row, i)
                });
            }
        }
    }
    let (outline, by_outline) = (&outline, &by_outline);

    let e = [2.0 / wf, 2.0 / hf];
    let mut hands: Vec<Vec<(usize, &mut [u8])>> = (0..threads).map(|_| Vec::new()).collect();
    for (band, rows) in out.chunks_mut(BAND * w * 4).enumerate() {
        hands[band % threads].push((band, rows));
    }
    let (corners, pieces, in_band) = (&corners, &pieces, &in_band);
    let cw = w.div_ceil(2);
    // (the pieces near a pixel along the outline, found by columns of the band this wide)
    const SLICE: usize = 16;
    let slices = w.div_ceil(SLICE);
    std::thread::scope(|s| {
        for hand in hands {
            let sc = &sc;
            s.spawn(move || {
                let none = Seen { near: 0.0, piece: u32::MAX, b1: 0.0, b2: 0.0 };
                let mut seen = vec![none; (BAND + 2) * w];
                let mut lit: Vec<Option<V>> = vec![None; (BAND / 2 + 1) * cw];
                let mut in_slice: Vec<Vec<u32>> = vec![Vec::new(); slices];
                let mut near_by: Vec<([Corner; 3], f32, u32)> = Vec::new();
                let (mut mask, mut edges): (Vec<u64>, Vec<u64>) = (Vec::new(), Vec::new());
                for (band, rows) in hand {
                    let y0 = band * BAND;
                    let tall = rows.len() / (w * 4);
                    // the rows laid: the band's, the row over it and the next band's first
                    // (where there are such rows)
                    let above = y0.min(1);
                    let first = y0 - above;
                    let laid = (tall + 1 + above).min(h - first);
                    let sees = |seen: &[Seen], x: usize, y: usize| seen[(y - first) * w + x];
                    seen.iter_mut().for_each(|s| *s = none);
                    in_slice.iter_mut().for_each(|v| v.clear());
                    for &id in &in_band[band] {
                        let [p, q, r] = pieces[id as usize].map(|i| corners[i as usize]);
                        let area = (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
                        if area.abs() < 1e-9 {
                            continue;
                        }
                        let inv = 1.0 / area;
                        let (lo, hi) = (p.x.min(q.x).min(r.x), p.x.max(q.x).max(r.x));
                        if smooth && hi >= 0.0 {
                            for v in in_slice.iter_mut().take((hi.min(wf - 1.0) as usize) / SLICE + 1).skip((lo.max(0.0) as usize) / SLICE) {
                                v.push(id);
                            }
                        }
                        let left = (lo - 0.5).ceil().max(0.0) as usize;
                        let right = ((hi - 0.5).floor().min(wf - 1.0)).max(-1.0);
                        let top = ((p.y.min(q.y).min(r.y) - 0.5).ceil().max(first as f32)) as usize;
                        let bottom = (p.y.max(q.y).max(r.y) - 0.5).floor().min((first + laid) as f32 - 1.0);
                        if right < 0.0 || bottom < top as f32 {
                            continue;
                        }
                        for y in top..=bottom as usize {
                            let py = y as f32 + 0.5;
                            for x in left..=right as usize {
                                let px = x as f32 + 0.5;
                                // (how much of each corner the pixel's centre takes)
                                let b0 = ((r.x - q.x) * (py - q.y) - (r.y - q.y) * (px - q.x)) * inv;
                                let b1 = ((p.x - r.x) * (py - r.y) - (p.y - r.y) * (px - r.x)) * inv;
                                let b2 = 1.0 - b0 - b1;
                                if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                                    continue;
                                }
                                let near = b0 * p.near + b1 * q.near + b2 * r.near;
                                let s = &mut seen[(y - first) * w + x];
                                if s.piece == u32::MAX || near > s.near {
                                    *s = Seen { near, piece: id, b1, b2 };
                                }
                            }
                        }
                    }
                    // the skin's light at a point of the frame (in pixels from the top left)
                    // on a piece, where it takes its corners as b1 and b2 say, without the grain
                    let light_at = |px: f32, py: f32, piece: u32, b1: f32, b2: f32, near: f32| -> V {
                        let uv = [px / wf, 1.0 - py / hf];
                        let ndc = [uv[0] * 2.0 - 1.0, uv[1] * 2.0 - 1.0];
                        let [p, q, rr] = pieces[piece as usize].map(|i| corners[i as usize]);
                        let b0 = 1.0 - b1 - b2;
                        let n = norm(add(add(mul(p.n, b0), mul(q.n, b1)), mul(rr.n, b2)));
                        let d = sc.ray_at(ndc);
                        sc.colour(ndc, uv, e, 0.0, d, 1.0 / near / -d[2], n)
                    };
                    let light = |x: usize, y: usize| -> Option<V> {
                        let s = sees(&seen, x, y);
                        (s.piece != u32::MAX).then(|| light_at(x as f32 + 0.5, y as f32 + 0.5, s.piece, s.b1, s.b2, s.near))
                    };
                    // The skin a pixel by the outline shows, of its COVER points: how many of them
                    // the body covers, and its light summed over them, each piece met with lit
                    // once, at the middle of the points it's met at, for as many as those are
                    let cover = |x: usize, y: usize, near_by: &mut Vec<([Corner; 3], f32, u32)>| -> (usize, V) {
                        // (the pieces whose bounds reach the pixel, of those in its column)
                        near_by.clear();
                        let (x0, y0f) = (x as f32, y as f32);
                        for &id in &in_slice[x / SLICE] {
                            let t = pieces[id as usize].map(|i| corners[i as usize]);
                            let [p, q, r] = t;
                            if p.x.max(q.x).max(r.x) < x0 || p.x.min(q.x).min(r.x) > x0 + 1.0 || p.y.max(q.y).max(r.y) < y0f || p.y.min(q.y).min(r.y) > y0f + 1.0 {
                                continue;
                            }
                            let area = (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
                            if area.abs() >= 1e-9 {
                                near_by.push((t, 1.0 / area, id));
                            }
                        }
                        // (where a point falls on a piece: how much of each corner it takes, its
                        // nearness, and whether it's on the piece at all)
                        let on_piece = |[p, q, r]: [Corner; 3], inv: f32, px: f32, py: f32| {
                            let b0 = ((r.x - q.x) * (py - q.y) - (r.y - q.y) * (px - q.x)) * inv;
                            let b1 = ((p.x - r.x) * (py - r.y) - (p.y - r.y) * (px - r.x)) * inv;
                            let b2 = 1.0 - b0 - b1;
                            (b1, b2, b0 * p.near + b1 * q.near + b2 * r.near, b0 >= 0.0 && b1 >= 0.0 && b2 >= 0.0)
                        };
                        // (each piece met with: which of near_by, at how many points, their sum)
                        let mut met: [(usize, usize, f32, f32); COVER.len()] = [(0, 0, 0.0, 0.0); COVER.len()];
                        let (mut kinds, mut n) = (0, 0);
                        for (i, j) in COVER {
                            let (px, py) = (x0 + 0.5 + i / 16.0, y0f + 0.5 + j / 16.0);
                            let mut best: Option<(f32, usize)> = None;
                            for (k, &(t, inv, _)) in near_by.iter().enumerate() {
                                let (_, _, near, inside) = on_piece(t, inv, px, py);
                                if inside && best.is_none_or(|b| near > b.0) {
                                    best = Some((near, k));
                                }
                            }
                            if let Some((_, k)) = best {
                                n += 1;
                                let m = match met[..kinds].iter().position(|m| m.0 == k) {
                                    Some(m) => m,
                                    None => {
                                        kinds += 1;
                                        met[kinds - 1] = (k, 0, 0.0, 0.0);
                                        kinds - 1
                                    }
                                };
                                met[m] = (k, met[m].1 + 1, met[m].2 + px, met[m].3 + py);
                            }
                        }
                        let mut sum = [0.0; 3];
                        for &(k, count, sx, sy) in &met[..kinds] {
                            let (t, inv, id) = near_by[k];
                            let (px, py) = (sx / count as f32, sy / count as f32);
                            let (b1, b2, near, _) = on_piece(t, inv, px, py);
                            sum = add(sum, mul(light_at(px, py, id, b1, b2, near), count as f32));
                        }
                        (n, sum)
                    };
                    // lit at every other pixel each way, the rest blended from the four round
                    // them (the light changes slowly across the skin); a pixel by the outline,
                    // where one of the four is off the body, lit itself
                    for r in (0..(laid - above)).step_by(2) {
                        for cx in 0..cw {
                            lit[(r / 2) * cw + cx] = light(cx * 2, y0 + r);
                        }
                    }
                    let at = |cx: usize, cr: usize| if cx < cw && cr * 2 < laid - above { lit[cr * cw + cx] } else { None };
                    // where the body is, each laid row 64 pixels to a word (a row of none over and
                    // under them); and the outline: each pixel with a neighbour on the other side
                    // of it, found 64 at a time
                    mask.clear();
                    mask.resize((laid + 2) * words, 0);
                    edges.clear();
                    edges.resize(tall * words, 0);
                    if smooth {
                        for (k, s) in seen[..laid * w].iter().enumerate() {
                            let x = k % w;
                            if s.piece != u32::MAX {
                                mask[(k / w + 1) * words + x / 64] |= 1u64 << (x % 64);
                            }
                        }
                        // (a row moved a pixel right, each pixel getting its left neighbour's, and left)
                        let right = |row: &[u64], i: usize| (row[i] << 1) | if i > 0 { row[i - 1] >> 63 } else { 0 };
                        let left = |row: &[u64], i: usize| (row[i] >> 1) | if i + 1 < row.len() { row[i + 1] << 63 } else { 0 };
                        for r in 0..tall {
                            // (the band's row r is laid row r + above, a row down in the mask)
                            let at = r + above + 1;
                            let (up, here, down) = (&mask[(at - 1) * words..at * words], &mask[at * words..(at + 1) * words], &mask[(at + 1) * words..(at + 2) * words]);
                            for i in 0..words {
                                let m = here[i];
                                edges[r * words + i] = (m ^ right(here, i)) | (m ^ left(here, i)) | (m ^ up[i]) | (m ^ right(up, i)) | (m ^ left(up, i)) | (m ^ down[i]) | (m ^ right(down, i)) | (m ^ left(down, i));
                            }
                        }
                    }
                    for (r, row) in rows.chunks_mut(w * 4).enumerate() {
                        let y = y0 + r;
                        // (the shader's frame is from the bottom; this one from the top)
                        let fy = (h - 1 - y) as f32 + 0.5;
                        let down = (y as f32 + 0.5) / hf;
                        let fade = 1.0 - smoothstep(sc.look.fade.0, sc.look.fade.1, down);
                        let g = sc.look.ground;
                        for x in 0..w {
                            let on = sees(&seen, x, y).piece != u32::MAX;
                            let bit = |m: &[u64], at: usize| m[at * words + x / 64] >> (x % 64) & 1 == 1;
                            // (along the outline: a neighbour on the other side of it, or the
                            // outline crossing the pixel)
                            let edge = smooth && fade > 0.0 && (bit(&edges, r) || bit(outline, y));
                            let body = |x: usize| -> Option<V> {
                                let (cx, cr, ox, oy) = (x / 2, r / 2, x % 2, r % 2);
                                let blended = match (ox, oy) {
                                    (0, 0) => at(cx, cr),
                                    (1, 0) => at(cx, cr).zip(at(cx + 1, cr)).map(|(a, b)| mul(add(a, b), 0.5)),
                                    (0, _) => at(cx, cr).zip(at(cx, cr + 1)).map(|(a, b)| mul(add(a, b), 0.5)),
                                    _ => at(cx, cr).zip(at(cx + 1, cr)).zip(at(cx, cr + 1).zip(at(cx + 1, cr + 1))).map(|((a, b), (c, d))| mul(add(add(a, b), add(c, d)), 0.25)),
                                };
                                // (by the outline the four may lie either side of it)
                                if smooth && bit(by_outline, y) { light(x, y) } else { blended.or_else(|| light(x, y)) }
                            };
                            let noise = (hash(x as f32 + 0.5, fy) - 0.5) * (2.0 / 255.0);
                            let c = if edge {
                                let (n, sum) = cover(x, y, &mut near_by);
                                let share = n as f32 / COVER.len() as f32;
                                add(add(mul(g, 1.0 - share), mul(sum, 1.0 / COVER.len() as f32)), mul([noise, noise, noise], share))
                            } else if on {
                                body(x).map_or(g, |k| add(k, [noise, noise, noise]))
                            } else {
                                g
                            };
                            let px = &mut row[x * 4..x * 4 + 4];
                            for i in 0..3 {
                                let v = g[i] + (c[i] - g[i]) * fade;
                                px[i] = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                            }
                            px[3] = 255;
                        }
                    }
                }
            });
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The body is drawn whole within About's frame, its top clear of the frame's edge, on the
    /// ground everywhere else; a press dents it where it lands.
    #[test]
    fn the_blob_sits_whole_in_abouts_frame_and_a_press_dents_it() {
        let ground = [24.0 / 255.0, 24.0 / 255.0, 27.0 / 255.0];
        let look = Look::about(ground);
        let (w, h) = (276, 187);
        let f = frame(&look, 3.0, &[], w, h);
        let g = [24u8, 24, 27];
        let at = |x: usize, y: usize| [f[(y * w + x) * 4], f[(y * w + x) * 4 + 1], f[(y * w + x) * 4 + 2]];
        let off = |p: [u8; 3]| p.iter().zip(g).any(|(a, b)| (*a as i32 - b as i32).abs() > 4);
        assert!(!off(at(0, 0)) && !off(at(w - 1, 0)), "the corners are the ground");
        let top_row = (0..h).find(|&y| (0..w).any(|x| off(at(x, y)))).expect("a body");
        assert!(top_row > h / 20, "the body's top clear of the frame's top: row {top_row}");
        // (a press at the body's middle changes it there)
        let poked = frame(&look, 3.0, &[Poke { at: [0.0, -0.1], age: 0.8 }], w, h);
        let (cx, cy) = (w / 2, h * 55 / 100);
        let i = (cy * w + cx) * 4;
        assert_ne!(f[i..i + 3], poked[i..i + 3], "the press shows where it landed");
    }

    /// The shader's own way to the skin, for the test below: each pixel's ray stepped along
    /// from the sphere round the whole body, in steps never under half a pixel's worth, and
    /// narrowed down by halves once inside.
    impl Scene<'_> {
        fn meets_sphere(&self, d: V, r: f32) -> Option<(f32, f32)> {
            let c = self.centre;
            let b = dot(d, c);
            let disc = b * b - (dot(c, c) - r * r);
            (disc >= 0.0).then(|| (b - disc.sqrt(), b + disc.sqrt()))
        }
        fn gap(&self, x: V) -> f32 {
            let o = self.to_body(x);
            (len(o) - (1.0 + self.bump(norm(o)))) * self.look.scale
        }
        fn trace(&self, d: V) -> Option<f32> {
            let outer = self.look.scale * (1.0 + 0.125 * self.look.amp + 0.02);
            let (t0, t1) = self.meets_sphere(d, outer)?;
            let (mut tt, mut last) = (t0, 0.0);
            for _ in 0..400 {
                let g = self.gap(mul(d, tt));
                if g < 0.0 {
                    let (mut lo, mut hi) = (tt - last, tt);
                    for _ in 0..10 {
                        let m = 0.5 * (lo + hi);
                        if self.gap(mul(d, m)) < 0.0 { hi = m } else { lo = m }
                    }
                    return Some(hi);
                }
                last = (g * 0.22).max(0.004);
                tt += last;
                if tt > t1 {
                    return None;
                }
            }
            None
        }
    }

    fn traced(look: &Look, real: f32, w: usize, h: usize) -> Vec<u8> {
        let sc = scene(look, real, &[], w, h);
        let (wf, hf) = (w as f32, h as f32);
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            let fy = (h - 1 - y) as f32 + 0.5;
            let fade = 1.0 - smoothstep(look.fade.0, look.fade.1, (y as f32 + 0.5) / hf);
            for x in 0..w {
                let fx = x as f32 + 0.5;
                let uv = [fx / wf, fy / hf];
                let ndc = [uv[0] * 2.0 - 1.0, uv[1] * 2.0 - 1.0];
                let d = sc.ray_at(ndc);
                let c = match sc.trace(d) {
                    None => look.ground,
                    Some(front) => {
                        let o = norm(sc.to_body(mul(d, front)));
                        let n = norm(sc.turn(sc.skin_normal(o, 0.01)));
                        sc.colour(ndc, uv, [2.0 / wf, 2.0 / hf], (hash(fx, fy) - 0.5) * (2.0 / 255.0), d, front, n)
                    }
                };
                let px = &mut out[(y * w + x) * 4..(y * w + x) * 4 + 4];
                for i in 0..3 {
                    px[i] = ((look.ground[i] + (c[i] - look.ground[i]) * fade).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
                px[3] = 255;
            }
        }
        out
    }

    /// The mesh draws what the shader's stepping draws: the same pixels, near enough, across
    /// a few moments of the body's motion.
    #[test]
    fn the_mesh_draws_what_the_shader_does() {
        let look = Look::about([0.094, 0.094, 0.106]);
        let (w, h) = (552, 374);
        for real in [0.0, 7.0, 31.0] {
            let a = traced(&look, real, w, h);
            let b = drawn(&look, real, &[], w, h, false);
            if let Some(dir) = std::env::var_os("BLOB_DUMP") {
                std::fs::write(std::path::Path::new(&dir).join(format!("a{real}.raw")), &a).unwrap();
                std::fs::write(std::path::Path::new(&dir).join(format!("b{real}.raw")), &b).unwrap();
            }
            let far = a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| (0..3).any(|i| (p[i] as i32 - q[i] as i32).abs() > 12)).count();
            let mean: f64 = a.iter().zip(&b).map(|(p, q)| (*p as f64 - *q as f64).abs()).sum::<f64>() / a.len() as f64;
            println!("blob at {real} s: {far} pixels apart by more than 12 of {}, mean {mean:.2}", w * h);
            assert!(far < w * h / 500 && mean < 0.5, "the mesh strays from the shader at {real} s: {far} pixels, mean {mean:.2}");
        }
    }

    /// The outline smoothed: only pixels along it change from the unsmoothed frame, many of
    /// them to a share between the body's color and the ground's, and none out on the ground
    /// or inside the body away from where it passes in front of itself (beyond a few levels of
    /// light, by where it passes behind, lit where they are rather than blended from round them).
    #[test]
    fn the_outline_is_smoothed_and_nothing_else_changes() {
        let look = Look::about([0.094, 0.094, 0.106]);
        let (w, h) = (552, 374);
        let (hard, soft) = (drawn(&look, 7.0, &[], w, h, false), frame(&look, 7.0, &[], w, h));
        let g = look.ground.map(|c| (c * 255.0 + 0.5) as u8);
        let ground = |f: &[u8], i: usize| (0..3).all(|k| f[i * 4 + k] == g[k]);
        let (mut changed, mut between, mut far_from_outline) = (0, 0, 0);
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let i = y * w + x;
                if (0..3).all(|k| (hard[i * 4 + k] as i32 - soft[i * 4 + k] as i32).abs() <= 2) {
                    continue;
                }
                changed += 1;
                // (by the ground, or by a step in the unsmoothed frame: a lobe's edge in front)
                let round = |f: &dyn Fn(usize) -> bool| (-1..=1).any(|dy: isize| (-1..=1).any(|dx: isize| f(((y as isize + dy) as usize) * w + (x as isize + dx) as usize)));
                let step = |j: usize| (0..3).any(|k| (hard[j * 4 + k] as i32 - hard[i * 4 + k] as i32).abs() > 8);
                let near_outline = round(&|j| ground(&hard, j) != ground(&hard, i)) || round(&step);
                // (away from them, only the light taken where the pixel is: a few levels)
                if !near_outline && (0..3).any(|k| (hard[i * 4 + k] as i32 - soft[i * 4 + k] as i32).abs() > 8) {
                    far_from_outline += 1;
                }
                if !ground(&soft, i) {
                    between += 1;
                }
            }
        }
        assert!(changed > 200 && between > changed / 2, "the outline isn't smoothed: {changed} changed, {between} between");
        assert_eq!(far_from_outline, 0, "pixels away from the outline changed");
    }

    /// Every edge smoothed, the outline against the ground and where the body passes in front
    /// of itself alike: the frame stays near the same frame drawn eight times as big and shrunk
    /// (each pixel the mean of its 64), across a few moments of the body's motion, among them
    /// two where a lobe crosses in front of the body (76 s, 112 s).
    #[test]
    fn every_edge_is_smoothed_where_the_body_overlaps_itself_too() {
        let look = Look::about([0.094, 0.094, 0.106]);
        let (w, h, k) = (552, 374, 8);
        let mut jagged = Vec::new();
        for real in [0.0, 4.0, 52.0, 60.0, 76.0, 112.0] {
            let soft = frame(&look, real, &[], w, h);
            let big = drawn(&look, real, &[], w * k, h * k, false);
            let mut fine = vec![0u8; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    for c in 0..3 {
                        let sum: u32 = (0..k).flat_map(|j| (0..k).map(move |i| (j, i))).map(|(j, i)| big[(((y * k + j) * w * k) + x * k + i) * 4 + c] as u32).sum();
                        fine[(y * w + x) * 4 + c] = ((sum as f32 / (k * k) as f32) + 0.5) as u8;
                    }
                    fine[(y * w + x) * 4 + 3] = 255;
                }
            }
            let apart = |i: usize| (0..3).map(|c| (soft[i * 4 + c] as i32 - fine[i * 4 + c] as i32).abs()).max().unwrap_or(0);
            // (8 points to a pixel against 64: a point's worth off is up to about 12 of 255
            // along the lime rim, two points' 24; a step left unsmoothed is far more)
            let far: Vec<usize> = (0..w * h).filter(|&i| apart(i) > 24).collect();
            let most = (0..w * h).map(apart).max().unwrap_or(0);
            if let Some(dir) = std::env::var_os("BLOB_DUMP") {
                let save = |name: &str, f: &[u8]| {
                    let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
                    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
                    enc.set_color(png::ColorType::Rgba);
                    enc.write_header().unwrap().write_image_data(f).unwrap();
                };
                save(&format!("soft{real}.png"), &soft);
                save(&format!("fine{real}.png"), &fine);
                let mut marks = soft.clone();
                for &i in &far {
                    marks[i * 4..i * 4 + 3].copy_from_slice(&[255, 0, 255]);
                }
                save(&format!("far{real}.png"), &marks);
            }
            println!("blob at {real} s: {} pixels apart by more than 24 of 255 from the frame drawn {k}x and shrunk, {most} at most", far.len());
            if far.len() > 3 || most > 32 {
                jagged.push((real, far.len(), most));
            }
        }
        assert!(jagged.is_empty(), "edges left jagged (at seconds: pixels more than 24 apart, the most): {jagged:?}");
    }

    /// On a screen denser than the page's units the frame drawn a pixel to a unit is shown
    /// smooth, not in steps: enlarged twice, it is nearer the frame drawn twice as big than
    /// the same frame stretched as Slint's software renderer stretches it (each pixel
    /// repeated), and as near at 1.5 times; it takes far less than drawing it that big.
    #[test]
    fn a_frame_for_a_denser_screen_is_enlarged_smoothly() {
        let look = Look::about([0.094, 0.094, 0.106]);
        let (w, h) = (276, 187);
        for (k, real) in [(2.0, 4.0), (2.0, 76.0), (1.5, 52.0)] {
            let (bw, bh) = ((w as f32 * k).round() as usize, (h as f32 * k).round() as usize);
            let small = frame(&look, real, &[], w, h);
            let truth = frame(&look, real, &[], bw, bh);
            let smooth = enlarged(&small, w, h, bw, bh);
            let mut stretched = vec![0u8; bw * bh * 4];
            for y in 0..bh {
                for x in 0..bw {
                    let (sx, sy) = ((x * w / bw).min(w - 1), (y * h / bh).min(h - 1));
                    stretched[(y * bw + x) * 4..(y * bw + x) * 4 + 4].copy_from_slice(&small[(sy * w + sx) * 4..(sy * w + sx) * 4 + 4]);
                }
            }
            // (along the outline and the body's own edges: where the frame drawn big changes
            // sharply from a pixel to the next, the steps a stretch makes show)
            let lum = |a: &[u8], i: usize| a[i * 4] as f64 + a[i * 4 + 1] as f64 + a[i * 4 + 2] as f64;
            let edge: Vec<usize> = (0..bh)
                .flat_map(|y| (0..bw).map(move |x| (x, y)))
                .filter(|&(x, y)| x + 1 < bw && y + 1 < bh && ((lum(&truth, y * bw + x) - lum(&truth, y * bw + x + 1)).abs() > 30.0 || (lum(&truth, y * bw + x) - lum(&truth, (y + 1) * bw + x)).abs() > 30.0))
                .map(|(x, y)| y * bw + x)
                .collect();
            let off = |a: &[u8]| (edge.iter().map(|&i| (lum(a, i) - lum(&truth, i)).powi(2)).sum::<f64>() / edge.len().max(1) as f64).sqrt();
            let (smooth_off, stretched_off) = (off(&smooth), off(&stretched));
            assert!(edge.len() > 500, "{k}x at {real} s: too few edge pixels to tell ({})", edge.len());
            assert!(smooth_off < stretched_off * 0.85, "{k}x at {real} s: along the edges enlarged {smooth_off:.2} off, stretched {stretched_off:.2}");
        }
    }

    /// How long a frame takes at About's sizes, to choose how big it's drawn:
    /// `cargo test --release blob_frame_time -- --ignored --nocapture`
    #[test]
    #[ignore = "a measurement, not a check"]
    fn blob_frame_time() {
        let look = Look::about([0.094, 0.094, 0.106]);
        for (w, h) in [(1380, 936), (690, 468), (460, 312), (345, 234)] {
            let cpu = || {
                let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
                let f: Vec<f64> = stat.rsplit(')').next().unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect();
                (f.get(10).unwrap_or(&0.0) + f.get(11).unwrap_or(&0.0)) * 10.0
            };
            let (start, used) = (std::time::Instant::now(), cpu());
            let smooth = std::env::var_os("BLOB_HARD").is_none();
            for k in 0..10 {
                let _ = drawn(&look, k as f32 * 0.033, &[], w, h, smooth);
            }
            let wall = start.elapsed().as_secs_f64() * 100.0;
            println!("blob {w}x{h}: {wall:.1} ms a frame, {:.0} ms of the processor's time", (cpu() - used) / 10.0);
        }
        // and enlarged for a 2x screen
        let (w, h) = (1380, 936);
        let small = frame(&look, 1.0, &[], w, h);
        let cpu = || {
            let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
            let f: Vec<f64> = stat.rsplit(')').next().unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect();
            (f.get(10).unwrap_or(&0.0) + f.get(11).unwrap_or(&0.0)) * 10.0
        };
        let (start, used) = (std::time::Instant::now(), cpu());
        for _ in 0..10 {
            let _ = enlarged(&small, w, h, 2 * w, 2 * h);
        }
        println!("blob {w}x{h} enlarged 2x: {:.1} ms, {:.0} ms of the processor's time", start.elapsed().as_secs_f64() * 100.0, (cpu() - used) / 10.0);
    }
}
