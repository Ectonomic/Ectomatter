//! Scenes: saved lighting looks for the whole desk, which light every device
//! over the profile it's on, as Corsair iCUE's scenes do. A scene is one
//! effect played across the desk, over where each device's lights sit on
//! Home's desk in millimeters (so a wave crosses the keyboard and then the
//! mouse as it would cross a real desk), or an effect of its own for each
//! device, a device left as it is keeping its profile's lighting. A device a
//! plugin brings in (OpenRGB's lighting) takes one of its own profiles in a
//! scene. Each device is sent its part in its direct mode (the keyboard's
//! keys, the mouse's logo and wheel), which its memory never keeps: with no
//! scene on, each plays its own profile again.
//!
//! Two scenes come with Ectomatter (Wave across the desk, Color shift
//! together) and can't be changed; the person's own are kept in Ectomatter's
//! settings, under `scenes`, and the one on under `scene`.

use serde_json::{json, Value};

/// The effects a scene plays, by the words the scene form has for them, and
/// what each is kept as in Ectomatter's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// one color
    Static,
    /// one color, fading down and up again
    Breathing,
    /// the spectrum, or the scene's colors, sweeping across the desk
    Wave,
    /// the whole desk going round the spectrum together
    Shift,
}

pub const EFFECTS: [(Effect, &str, &str); 4] = [(Effect::Static, "static", "Static"), (Effect::Breathing, "breathing", "Breathing"), (Effect::Wave, "wave", "Wave"), (Effect::Shift, "shift", "Color shift")];

impl Effect {
    pub fn id(self) -> &'static str {
        EFFECTS.iter().find(|(e, ..)| *e == self).map_or("static", |(_, id, _)| id)
    }

    pub fn of(id: &str) -> Option<Effect> {
        EFFECTS.iter().find(|(_, i, _)| *i == id).map(|(e, ..)| *e)
    }

    pub fn label(self) -> &'static str {
        EFFECTS.iter().find(|(e, ..)| *e == self).map_or("Static", |(.., w)| w)
    }

    /// How many colors the scene form gives the effect: a wave of the
    /// person's own runs between two, a color shift goes round the spectrum.
    pub fn colors(self) -> usize {
        match self {
            Effect::Static | Effect::Breathing => 1,
            Effect::Wave => 2,
            Effect::Shift => 0,
        }
    }
}

/// An effect with its colors, how fast it plays (0 to 1, the middle the
/// built-in scenes' pace) and, for a wave, whether it runs right to left.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub effect: Effect,
    pub colors: Vec<[u8; 3]>,
    pub speed: f32,
    pub reverse: bool,
}

/// The colors a new look starts in: an orange and a blue.
pub const START_COLORS: [[u8; 3]; 2] = [[0xff, 0x60, 0x00], [0x1f, 0x8f, 0xff]];

impl Look {
    pub fn new(effect: Effect) -> Look {
        Look { effect, colors: START_COLORS[..effect.colors()].to_vec(), speed: 0.5, reverse: false }
    }

    /// The same look as another effect, keeping its colors where it can.
    pub fn as_effect(&self, effect: Effect) -> Look {
        let mut colors = self.colors.clone();
        colors.truncate(effect.colors());
        while colors.len() < effect.colors() {
            colors.push(START_COLORS[colors.len().min(1)]);
        }
        Look { effect, colors, ..self.clone() }
    }

    fn to_json(&self) -> Value {
        json!({ "effect": self.effect.id(), "colors": self.colors.iter().map(|c| crate::lighting::hex_of(*c)).collect::<Vec<_>>(), "speed": self.speed, "reverse": self.reverse })
    }

    fn of_json(v: &Value) -> Option<Look> {
        let effect = Effect::of(v["effect"].as_str()?)?;
        let colors = v["colors"].as_array().map(|a| a.iter().filter_map(|c| hex(c.as_str()?)).collect()).unwrap_or_default();
        Some(Look { effect, colors, speed: v["speed"].as_f64().map_or(0.5, |s| s.clamp(0.0, 1.0) as f32), reverse: v["reverse"].as_bool().unwrap_or(false) })
    }
}

/// A color from its code, `#RRGGBB` or `RRGGBB`.
pub fn hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// A device's part in a scene that gives each device its own effect.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    /// left as it is: the device plays its profile's lighting
    AsIs,
    Look(Look),
}

/// How a scene lights the desk.
#[derive(Clone, Debug, PartialEq)]
pub enum Lights {
    /// one effect across the whole desk
    Desk(Look),
    /// an effect for each device, by the device's key (`z10`, `sensei310`);
    /// a device not named is left as it is
    Each(Vec<(String, Part)>),
}

impl Lights {
    /// A device's part: its own in a scene of parts, the desk's otherwise.
    pub fn part(&self, device: &str) -> Part {
        match self {
            Lights::Desk(l) => Part::Look(l.clone()),
            Lights::Each(parts) => parts.iter().find(|(d, _)| d == device).map_or(Part::AsIs, |(_, p)| p.clone()),
        }
    }

    /// Whether anything in it moves, so its frames have to keep coming.
    pub fn moves(&self) -> bool {
        let moving = |l: &Look| l.effect != Effect::Static;
        match self {
            Lights::Desk(l) => moving(l),
            Lights::Each(parts) => parts.iter().any(|(_, p)| matches!(p, Part::Look(l) if moving(l))),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub lights: Lights,
    /// the profile each device a plugin brought in plays in this scene, by
    /// the device's id (OpenRGB's, one of OpenRGB's profiles); a device not
    /// named is left as it is
    pub plugins: Vec<(String, String)>,
    /// one that comes with Ectomatter, which can't be changed or deleted
    pub built_in: bool,
}

/// The scenes that come with Ectomatter: the wave across the desk and the
/// color shift, as Home played them before there were scenes.
pub fn built_in() -> Vec<Scene> {
    let desk = |id: &str, name: &str, effect: Effect| Scene { id: id.into(), name: name.into(), lights: Lights::Desk(Look { effect, colors: Vec::new(), speed: 0.5, reverse: false }), plugins: Vec::new(), built_in: true };
    vec![desk("wave", "Wave across the desk", Effect::Wave), desk("shift", "Color shift together", Effect::Shift)]
}

/// Every scene: the built-in ones, then the person's own in the order they were made.
pub fn all(kept: &Value) -> Vec<Scene> {
    let mut out = built_in();
    out.extend(kept.as_array().into_iter().flatten().filter_map(of_json));
    out
}

fn of_json(v: &Value) -> Option<Scene> {
    let id = v["id"].as_str()?.to_string();
    let name = v["name"].as_str().unwrap_or("").to_string();
    let lights = if let Some(d) = v["lights"].get("desk") {
        Lights::Desk(Look::of_json(d)?)
    } else {
        let parts = v["lights"]["each"].as_object().map(|m| m.iter().map(|(d, p)| (d.clone(), Look::of_json(p).map_or(Part::AsIs, Part::Look))).collect()).unwrap_or_default();
        Lights::Each(parts)
    };
    let plugins = v["plugins"].as_object().map(|m| m.iter().filter_map(|(d, p)| Some((d.clone(), p.as_str()?.to_string()))).collect()).unwrap_or_default();
    Some(Scene { id, name, lights, plugins, built_in: false })
}

/// A scene as Ectomatter's settings keep it.
pub fn to_json(s: &Scene) -> Value {
    let lights = match &s.lights {
        Lights::Desk(l) => json!({ "desk": l.to_json() }),
        Lights::Each(parts) => {
            let m: serde_json::Map<String, Value> = parts.iter().map(|(d, p)| (d.clone(), match p { Part::AsIs => json!("as-is"), Part::Look(l) => l.to_json() })).collect();
            json!({ "each": m })
        }
    };
    let plugins: serde_json::Map<String, Value> = s.plugins.iter().map(|(d, p)| (d.clone(), json!(p))).collect();
    json!({ "id": s.id, "name": s.name, "lights": lights, "plugins": plugins })
}

/// A scene id's number (`scene-3`, 3), 0 for one with none.
pub fn id_number(id: &str) -> u32 {
    id.strip_prefix("scene-").and_then(|n| n.parse().ok()).unwrap_or(0)
}

/// An id for a new scene, unlike any kept or made before (`made`, the
/// highest number given yet, a deleted scene's among them).
pub fn new_id(kept: &[Scene], made: u32) -> String {
    let n = kept.iter().map(|s| id_number(&s.id)).max().unwrap_or(0).max(made) + 1;
    format!("scene-{n}")
}

/// The scene to have on, from the setting Home kept before there were scenes
/// (`desk.light`: `wave`, `shift` or `off`).
pub fn from_old_setting(old: &str) -> Option<&'static str> {
    match old {
        "wave" => Some("wave"),
        "shift" => Some("shift"),
        _ => None,
    }
}

// ------------------------------------------------------------ the light

/// How wide one pass of a wave is across the desk, and how long each effect
/// takes at the middle speed: a pass of the wave, a breath, a round of the
/// spectrum.
pub const WAVE_MM: f32 = 420.0;
const WAVE_S: f32 = 4.0;
const BREATH_S: f32 = 3.0;
const SHIFT_S: f32 = 6.0;

/// How long the look takes to come round again: twice as long at the
/// slowest, half as long at the fastest.
fn period(l: &Look) -> f32 {
    let base = match l.effect {
        Effect::Static => 1.0,
        Effect::Breathing => BREATH_S,
        Effect::Wave => WAVE_S,
        Effect::Shift => SHIFT_S,
    };
    base * 2f32.powf((0.5 - l.speed.clamp(0.0, 1.0)) * 2.0)
}

/// The words for a speed, as the scene form says it.
pub fn speed_words(speed: f32) -> &'static str {
    if speed < 0.34 {
        "Slow"
    } else if speed < 0.67 {
        "Medium"
    } else {
        "Fast"
    }
}

/// The look's color at a place on the desk (millimeters from the desk's
/// left), `t` seconds in.
pub fn color(l: &Look, x_mm: f32, t: f32) -> [u8; 3] {
    let first = l.colors.first().copied().unwrap_or([0xff; 3]);
    match l.effect {
        Effect::Static => first,
        Effect::Breathing => {
            let b = 0.12 + 0.88 * (0.5 - 0.5 * (t / period(l) * std::f32::consts::TAU).cos());
            first.map(|c| (c as f32 * b).round() as u8)
        }
        Effect::Wave => {
            let dir = if l.reverse { -1.0 } else { 1.0 };
            let phase = (x_mm / WAVE_MM - dir * t / period(l)).rem_euclid(1.0);
            if l.colors.len() < 2 { hue(phase) } else { round(&l.colors, phase) }
        }
        Effect::Shift => {
            let phase = (t / period(l)).rem_euclid(1.0);
            if l.colors.len() < 2 { hue(phase) } else { round(&l.colors, phase) }
        }
    }
}

/// The way round a run of colors at a place on it (0 to 1), from each
/// color to the next and from the last back to the first.
fn round(colors: &[[u8; 3]], phase: f32) -> [u8; 3] {
    let n = colors.len() as f32;
    let at = phase * n;
    let i = at.floor() as usize % colors.len();
    let (a, b) = (colors[i], colors[(i + 1) % colors.len()]);
    let f = at - at.floor();
    std::array::from_fn(|k| (a[k] as f32 + (b[k] as f32 - a[k] as f32) * f).round() as u8)
}

/// A hue (0 to 1 round the spectrum) at its full strength, as an LED shows it.
pub fn hue(h: f32) -> [u8; 3] {
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

// ------------------------------------------------------------ pictures of a scene

/// A device as a scene's pictures lay it: its key, and where it runs across
/// the desk, from its left to its right in millimeters.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub key: String,
    pub x0: f32,
    pub x1: f32,
}

/// Each device's share of a scene's pictures, in the order given: its share
/// of the desk's width, but never under three tenths with two devices or
/// more, so a mouse beside a keyboard still shows (and its name fits).
pub fn shares(spans: &[Span]) -> Vec<f32> {
    let total: f32 = spans.iter().map(|s| (s.x1 - s.x0).max(1.0)).sum::<f32>().max(1.0);
    let real: Vec<f32> = spans.iter().map(|s| (s.x1 - s.x0).max(1.0) / total).collect();
    let least = if spans.len() >= 2 { 0.3 } else { 0.0 };
    let fixed = real.iter().filter(|r| **r < least).count() as f32 * least;
    if fixed >= 1.0 {
        return vec![1.0 / spans.len() as f32; spans.len()];
    }
    // (the small ones at the least share; the others in proportion, in what's left)
    let rest: f32 = real.iter().filter(|r| **r >= least).sum::<f32>().max(1e-6);
    real.iter().map(|r| if *r < least { least } else { r / rest * (1.0 - fixed) }).collect()
}

/// A part's color at a place across its device (0 to 1 from its left),
/// for the pictures: the device's own run of the desk, or none for a device
/// left as it is.
fn part_color(p: &Part, span: &Span, f: f32, t: f32) -> Option<[u8; 3]> {
    match p {
        Part::AsIs => None,
        Part::Look(l) => Some(color(l, span.x0 + f * (span.x1 - span.x0), t)),
    }
}

/// A picture of a scene's light across the desk, `w` by `h` pixels: each
/// device in its share of the width in the order they sit on the desk,
/// `gap` pixels apart, each column the light at that place `t` seconds in;
/// a device left as it is in dark stripes. Straight RGBA.
pub fn strip(lights: &Lights, spans: &[Span], w: u32, h: u32, gap: u32, t: f32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    let share = shares(spans);
    let room = w.saturating_sub(gap * spans.len().saturating_sub(1) as u32) as f32;
    let mut x = 0u32;
    for (i, s) in spans.iter().enumerate() {
        let sw = if i + 1 == spans.len() { w.saturating_sub(x) } else { (share[i] * room).round() as u32 };
        let part = lights.part(&s.key);
        for cx in 0..sw {
            let f = (cx as f32 + 0.5) / sw.max(1) as f32;
            let c = part_color(&part, s, f, t);
            for cy in 0..h {
                let at = (((cy * w) + x + cx) * 4) as usize;
                if at + 4 > out.len() {
                    continue;
                }
                let px = c.unwrap_or(if ((x + cx + cy) / 6) % 2 == 0 { [0x2a, 0x2a, 0x30] } else { [0x23, 0x23, 0x28] });
                out[at..at + 4].copy_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        x += sw + gap;
        if x >= w {
            break;
        }
    }
    out
}

/// A scene's chip, `px` pixels square with rounded corners, for menus and
/// buttons: one look across the chip (a color shift round its middle), or
/// each device's color side by side as they sit on the desk; none, an empty
/// outline. Premultiplied RGBA.
pub fn chip(scene: Option<&Scene>, spans: &[Span], px: u32) -> Vec<u8> {
    let n = px as f32;
    let r = n * 0.28;
    let mut out = vec![0u8; (px * px * 4) as usize];
    for y in 0..px {
        for x in 0..px {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // (inside the rounded square, softened over its edge's last pixel)
            let (dx, dy) = ((r - fx).max(fx - (n - r)).max(0.0), (r - fy).max(fy - (n - r)).max(0.0));
            let d = (dx * dx + dy * dy).sqrt() - r;
            let a = (0.5 - d).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let (rgb, alpha) = match scene {
                None => {
                    // an outline in the menu's gray, a pixel and a half wide at 14
                    let (stroke, inset) = (1.4 * n / 14.0, -d);
                    if inset > stroke + 0.5 {
                        continue;
                    }
                    ([0x5c, 0x5c, 0x64], a * (stroke + 0.5 - inset).clamp(0.0, 1.0))
                }
                Some(s) => (chip_color(s, spans, fx / n, fy / n), a),
            };
            let at = ((y * px + x) * 4) as usize;
            out[at..at + 4].copy_from_slice(&[(rgb[0] as f32 * alpha) as u8, (rgb[1] as f32 * alpha) as u8, (rgb[2] as f32 * alpha) as u8, (alpha * 255.0) as u8]);
        }
    }
    out
}

/// A chip's color at a place on it (0 to 1 across and down).
fn chip_color(s: &Scene, spans: &[Span], fx: f32, fy: f32) -> [u8; 3] {
    match &s.lights {
        Lights::Desk(l) => match l.effect {
            // (a color shift round the chip's middle, the spectrum it goes round)
            Effect::Shift if l.colors.len() < 2 => hue(((fy - 0.5).atan2(fx - 0.5) / std::f32::consts::TAU).rem_euclid(1.0)),
            Effect::Shift => round(&l.colors, ((fy - 0.5).atan2(fx - 0.5) / std::f32::consts::TAU).rem_euclid(1.0)),
            Effect::Wave if l.colors.len() < 2 => hue(fx * 0.85),
            Effect::Wave => round(&l.colors, fx * 0.5),
            Effect::Static | Effect::Breathing => l.colors.first().copied().unwrap_or([0xff; 3]),
        },
        Lights::Each(_) => {
            let shares = shares(spans);
            let mut at = 0.0;
            for (sp, share) in spans.iter().zip(shares) {
                if fx <= at + share || std::ptr::eq(sp, spans.last().unwrap()) {
                    let f = ((fx - at) / share).clamp(0.0, 1.0);
                    return match s.lights.part(&sp.key) {
                        Part::AsIs => [0x3a, 0x3a, 0x41],
                        // (a breath shown at its brightest; a wave's colors across the device)
                        Part::Look(l) if l.effect == Effect::Wave && l.colors.len() >= 2 => round(&l.colors, f * 0.5),
                        Part::Look(l) if l.effect == Effect::Wave || l.effect == Effect::Shift => hue(f * 0.85),
                        Part::Look(l) => l.colors.first().copied().unwrap_or([0xff; 3]),
                    };
                }
                at += share;
            }
            [0x3a, 0x3a, 0x41]
        }
    }
}

/// An effect tile's picture for the scene form, `w` by `h` pixels, in the
/// look's colors: Static its color, Breathing its color between dark ends, a
/// wave running across it (twice the tile's width, to roll along), a color
/// shift round its middle. Straight RGBA.
pub fn tile(l: &Look, w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    let first = l.colors.first().copied().unwrap_or([0xff; 3]);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            let c = match l.effect {
                Effect::Static => first,
                Effect::Breathing => {
                    let b = 0.2 + 0.8 * (1.0 - ((fx - 0.6).abs() / 0.6).min(1.0));
                    first.map(|c| (c as f32 * b) as u8)
                }
                // (two passes of the wave across the doubled picture, so it rolls round seamlessly)
                Effect::Wave if l.colors.len() >= 2 => round(&l.colors, (fx * 2.0).rem_euclid(1.0)),
                Effect::Wave => hue((fx * 2.0).rem_euclid(1.0)),
                Effect::Shift => {
                    let aspect = w as f32 / h as f32;
                    hue((((fy - 0.5)).atan2((fx - 0.5) * aspect) / std::f32::consts::TAU).rem_euclid(1.0))
                }
            };
            let at = ((y * w + x) * 4) as usize;
            out[at..at + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave() -> Look {
        Look { effect: Effect::Wave, colors: Vec::new(), speed: 0.5, reverse: false }
    }

    /// The built-in wave crosses the desk: two places a
    /// pass apart show one color, a place half a pass on the opposite one,
    /// and the wave moves on in time; turned round, it moves the other way.
    #[test]
    fn the_wave_crosses_the_desk() {
        let l = wave();
        let w = |x: f32, t: f32| color(&l, x, t);
        assert_eq!(w(0.0, 0.0), w(WAVE_MM, 0.0));
        assert_ne!(w(0.0, 0.0), w(WAVE_MM / 2.0, 0.0));
        assert_eq!(w(WAVE_MM / 4.0, WAVE_S / 4.0), w(0.0, 0.0), "the same color a quarter on, a quarter later");
        let back = Look { reverse: true, ..wave() };
        assert_eq!(color(&back, 0.0, WAVE_S / 4.0), w(WAVE_MM / 4.0, 0.0), "right to left: what was a quarter pass to the right");
        assert_eq!(hue(0.0), [255, 0, 0]);
        assert_eq!(hue(1.0 / 3.0), [0, 255, 0]);
        // (a drawing's middle at its place's middle, in millimeters)
        let (x, y) = on_desk((0.0, 0.0, 100.0, 50.0), [760.0, 0.0, 760.0, 380.0], 50.0, 25.0);
        assert!((x - 470.0 * 1.5).abs() < 0.01 && (y - 470.0 / 4.0).abs() < 0.01, "{x} {y}");
    }

    /// A wave of two colors runs from one to the other and back; a color
    /// shift is one color everywhere; Static is its color; Breathing fades
    /// to an eighth or so and back up; faster takes less time.
    #[test]
    fn each_effect_plays_its_colors() {
        let two = Look { colors: vec![[255, 0, 0], [0, 0, 255]], ..wave() };
        assert_eq!(color(&two, 0.0, 0.0), [255, 0, 0]);
        assert_eq!(color(&two, WAVE_MM / 2.0, 0.0), [0, 0, 255]);
        let shift = Look::new(Effect::Shift);
        assert_eq!(color(&shift, 0.0, 1.0), color(&shift, 300.0, 1.0));
        let stat = Look { colors: vec![[1, 2, 3]], ..Look::new(Effect::Static) };
        assert_eq!(color(&stat, 99.0, 7.0), [1, 2, 3]);
        let breath = Look { colors: vec![[200, 100, 0]], ..Look::new(Effect::Breathing) };
        assert_eq!(color(&breath, 0.0, 0.0), [24, 12, 0], "at its dimmest as it starts");
        assert_eq!(color(&breath, 0.0, BREATH_S / 2.0), [200, 100, 0], "at its brightest half a breath on");
        let fast = Look { speed: 1.0, ..breath.clone() };
        assert_eq!(color(&fast, 0.0, BREATH_S / 4.0), [200, 100, 0], "twice as fast at the fastest");
        assert_eq!(speed_words(0.5), "Medium");
    }

    /// The built-in scenes come first; a kept scene comes back as it
    /// was saved, after them, and a new one takes the next id.
    #[test]
    fn scenes_are_kept_as_saved() {
        let s = Scene {
            id: "scene-3".into(),
            name: "DOOM".into(),
            lights: Lights::Each(vec![("z10".into(), Part::Look(Look { colors: vec![[0xe0, 0x30, 0x1e]], ..Look::new(Effect::Static) })), ("sensei310".into(), Part::AsIs)]),
            plugins: vec![("openrgb".into(), "ARC Raiders".into())],
            built_in: false,
        };
        let all = all(&json!([to_json(&s)]));
        assert_eq!(all.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["wave", "shift", "scene-3"]);
        assert_eq!(all[2], s);
        assert!(all[0].built_in && !all[2].built_in);
        assert_eq!(new_id(&all, 0), "scene-4");
        assert_eq!(new_id(&all, 7), "scene-8", "never the id of a scene made and deleted since");
        assert_eq!(s.lights.part("sensei310"), Part::AsIs);
        assert_eq!(s.lights.part("a new mouse"), Part::AsIs, "a device the scene doesn't name is left as it is");
        assert!(!s.lights.moves(), "a static keyboard and a mouse as it is: no frames to keep sending");
        assert_eq!(from_old_setting("wave"), Some("wave"));
        assert_eq!(from_old_setting("off"), None);
        assert_eq!(hex("#E0301E"), Some([0xe0, 0x30, 0x1e]));
    }

    /// A look changed to another effect keeps the colors it can and gets
    /// the ones it lacks.
    #[test]
    fn a_look_keeps_its_colors_as_its_effect_changes() {
        let red = Look { colors: vec![[255, 0, 0]], ..Look::new(Effect::Static) };
        let w = red.as_effect(Effect::Wave);
        assert_eq!(w.colors, vec![[255, 0, 0], START_COLORS[1]]);
        assert_eq!(w.as_effect(Effect::Shift).colors, Vec::<[u8; 3]>::new());
        assert_eq!(w.as_effect(Effect::Breathing).colors, vec![[255, 0, 0]]);
    }

    /// A chip is its scene's color inside and clear at its corners; no
    /// scene's is an outline, clear in its middle. The strip shows each
    /// device in its share, a device left as it is in dark stripes.
    #[test]
    fn pictures_show_the_scene() {
        let red = Scene { id: "r".into(), name: "Red".into(), lights: Lights::Desk(Look { colors: vec![[255, 0, 0]], ..Look::new(Effect::Static) }), plugins: Vec::new(), built_in: false };
        let px = 28;
        let c = chip(Some(&red), &[], px);
        let at = |x: u32, y: u32| ((y * px + x) * 4) as usize;
        assert_eq!(&c[at(14, 14)..at(14, 14) + 4], &[255, 0, 0, 255]);
        assert_eq!(c[at(0, 0) + 3], 0, "a rounded corner");
        let none = chip(None, &[], px);
        assert_eq!(none[at(14, 14) + 3], 0, "an empty middle");
        assert!(none[at(14, 0) + 3] > 0, "an outline");
        let spans = [Span { key: "sensei310".into(), x0: 0.0, x1: 100.0 }, Span { key: "z10".into(), x0: 120.0, x1: 320.0 }];
        let each = Lights::Each(vec![("z10".into(), Part::Look(Look { colors: vec![[0, 255, 0]], ..Look::new(Effect::Static) }))]);
        let s = strip(&each, &spans, 303, 2, 3, 0.0);
        let px_at = |x: u32| &s[(x * 4) as usize..(x * 4 + 3) as usize];
        assert_eq!(px_at(300), &[0, 255, 0], "the keyboard's two thirds");
        let small = [Span { key: "sensei310".into(), x0: 0.0, x1: 60.0 }, Span { key: "z10".into(), x0: 80.0, x1: 520.0 }];
        assert!((shares(&small)[0] - 0.3).abs() < 1e-6, "a mouse beside a keyboard keeps three tenths: {:?}", shares(&small));
        assert!(px_at(10)[1] < 60 && px_at(10)[0] < 60, "the mouse left as it is, dark");
    }
}
