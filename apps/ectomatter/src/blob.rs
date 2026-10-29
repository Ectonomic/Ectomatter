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
/// only the body's own pixels cost anything. Each is then lit as the shader lights it.
pub fn frame(look: &Look, real: f32, pokes: &[Poke], w: usize, h: usize) -> Vec<u8> {
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
                // (a band also lays its pieces on the next band's first row, where its last
                // row's light is blended from)
                let (first, last) = (((top - 1.0).max(0.0) as usize) / BAND, ((bottom.min(hf - 1.0)) as usize) / BAND);
                for band in in_band.iter_mut().take(last + 1).skip(first) {
                    band.push(id);
                }
            }
        }
    }

    let e = [2.0 / wf, 2.0 / hf];
    let mut hands: Vec<Vec<(usize, &mut [u8])>> = (0..threads).map(|_| Vec::new()).collect();
    for (band, rows) in out.chunks_mut(BAND * w * 4).enumerate() {
        hands[band % threads].push((band, rows));
    }
    let (corners, pieces, in_band) = (&corners, &pieces, &in_band);
    let cw = w.div_ceil(2);
    std::thread::scope(|s| {
        for hand in hands {
            let sc = &sc;
            s.spawn(move || {
                let none = Seen { near: 0.0, piece: u32::MAX, b1: 0.0, b2: 0.0 };
                let mut seen = vec![none; (BAND + 1) * w];
                let mut lit: Vec<Option<V>> = vec![None; (BAND / 2 + 1) * cw];
                for (band, rows) in hand {
                    let y0 = band * BAND;
                    let tall = rows.len() / (w * 4);
                    // (and the next band's first row, if there is one)
                    let laid = (tall + 1).min(h - y0);
                    seen.iter_mut().for_each(|s| *s = none);
                    for &id in &in_band[band] {
                        let [p, q, r] = pieces[id as usize].map(|i| corners[i as usize]);
                        let area = (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
                        if area.abs() < 1e-9 {
                            continue;
                        }
                        let inv = 1.0 / area;
                        let left = (p.x.min(q.x).min(r.x) - 0.5).ceil().max(0.0) as usize;
                        let right = ((p.x.max(q.x).max(r.x) - 0.5).floor().min(wf - 1.0)).max(-1.0);
                        let top = ((p.y.min(q.y).min(r.y) - 0.5).ceil().max(y0 as f32)) as usize;
                        let bottom = (p.y.max(q.y).max(r.y) - 0.5).floor().min((y0 + laid) as f32 - 1.0);
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
                                let s = &mut seen[(y - y0) * w + x];
                                if s.piece == u32::MAX || near > s.near {
                                    *s = Seen { near, piece: id, b1, b2 };
                                }
                            }
                        }
                    }
                    // the skin's light where a pixel sees it, without the grain
                    let light = |x: usize, r: usize| -> Option<V> {
                        let s = seen[r * w + x];
                        if s.piece == u32::MAX {
                            return None;
                        }
                        let y = y0 + r;
                        let (fx, fy) = (x as f32 + 0.5, (h - 1 - y) as f32 + 0.5);
                        let uv = [fx / wf, fy / hf];
                        let ndc = [uv[0] * 2.0 - 1.0, uv[1] * 2.0 - 1.0];
                        let [p, q, rr] = pieces[s.piece as usize].map(|i| corners[i as usize]);
                        let b0 = 1.0 - s.b1 - s.b2;
                        let n = norm(add(add(mul(p.n, b0), mul(q.n, s.b1)), mul(rr.n, s.b2)));
                        let d = sc.ray_at(ndc);
                        Some(sc.colour(ndc, uv, e, 0.0, d, 1.0 / s.near / -d[2], n))
                    };
                    // lit at every other pixel each way, the rest blended from the four round
                    // them (the light changes slowly across the skin); a pixel by the outline,
                    // where one of the four is off the body, lit itself
                    for r in (0..laid).step_by(2) {
                        for cx in 0..cw {
                            lit[(r / 2) * cw + cx] = light(cx * 2, r);
                        }
                    }
                    let at = |cx: usize, cr: usize| if cx < cw && cr * 2 < laid { lit[cr * cw + cx] } else { None };
                    for (r, row) in rows.chunks_mut(w * 4).enumerate() {
                        let y = y0 + r;
                        // (the shader's frame is from the bottom; this one from the top)
                        let fy = (h - 1 - y) as f32 + 0.5;
                        let down = (y as f32 + 0.5) / hf;
                        let fade = 1.0 - smoothstep(sc.look.fade.0, sc.look.fade.1, down);
                        let g = sc.look.ground;
                        for x in 0..w {
                            let c = if seen[r * w + x].piece == u32::MAX {
                                g
                            } else {
                                let (cx, cr, ox, oy) = (x / 2, r / 2, x % 2, r % 2);
                                let blended = match (ox, oy) {
                                    (0, 0) => at(cx, cr),
                                    (1, 0) => at(cx, cr).zip(at(cx + 1, cr)).map(|(a, b)| mul(add(a, b), 0.5)),
                                    (0, _) => at(cx, cr).zip(at(cx, cr + 1)).map(|(a, b)| mul(add(a, b), 0.5)),
                                    _ => at(cx, cr).zip(at(cx + 1, cr)).zip(at(cx, cr + 1).zip(at(cx + 1, cr + 1))).map(|((a, b), (c, d))| mul(add(add(a, b), add(c, d)), 0.25)),
                                };
                                let lit = blended.or_else(|| light(x, r)).unwrap_or(g);
                                let noise = (hash(x as f32 + 0.5, fy) - 0.5) * (2.0 / 255.0);
                                add(lit, [noise, noise, noise])
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
            let b = frame(&look, real, &[], w, h);
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
            for k in 0..10 {
                let _ = frame(&look, k as f32 * 0.033, &[], w, h);
            }
            let wall = start.elapsed().as_secs_f64() * 100.0;
            println!("blob {w}x{h}: {wall:.1} ms a frame, {:.0} ms of the processor's time", (cpu() - used) / 10.0);
        }
    }
}
