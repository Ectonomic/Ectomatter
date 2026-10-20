//! The mouse's picture for its page: SteelSeries' core's art (Grant's
//! drawing, cores/steelseries/art/sensei-310.svg), drawn once for a size as
//! layers the page puts together: the mouse with its lights off over the
//! shadow every device casts on its stage, each light as a mask the page
//! tints in the light's color of the moment (so an effect plays on the
//! drawing without drawing it again), each button picked out, and which
//! button is at each pixel, for a click on the drawing.
//!
//! A light is groups of white shapes laid over the mouse in the ordinary
//! way, so lit in a color each group is that color laid over the mouse
//! through the white's own coverage. The coverage is measured once, from the
//! drawing with the group white and with it off, and the page lays the
//! light's color through it, group by group in the drawing's order.

use resvg::tiny_skia::{FillRule, Mask, Pixmap, PixmapPaint, Transform};
use resvg::usvg;

/// The drawing's mouse, in its own units: it spans this box.
pub const VIEW: (f32, f32, f32, f32) = (28.0, 30.0, 304.0, 520.0);

/// The surfaces of each button, by its number in the art (the mouse's own
/// button number), for picking it out.
const SURFACES: [&[&str]; 8] = [
    &["path20", "left-middle", "path21"],
    &["path22", "right-middle", "path23"],
    &["rect23"],
    &["path26"],
    &["path25"],
    &["path28"],
    &["path27"],
    &["rect24"],
];

/// The lights' groups in the drawing, each with its light (0 the logo, 1 the
/// wheel), in the drawing's order: what lies under the buttons, then what
/// lies over everything. Each is a layer of its own, in its light's color,
/// so a light dimmed shows as the drawing does with its groups faded.
const GROUPS: [(usize, &str); 4] = [(0, "illumination_9"), (1, "illumination_3"), (0, "illumination_209"), (1, "illumination_203")];

/// How a light shows: its color, or off.
pub type Lit = Option<[u8; 3]>;

/// A piece of the picture, and where its top left corner is on the whole
/// picture, in pixels.
pub struct Patch {
    pub x: u32,
    pub y: u32,
    pub px: Pixmap,
}

/// The picture in layers, for one size.
pub struct Layers {
    /// the mouse with its lights off and no button chosen, over its shadow
    pub base: Pixmap,
    /// how far the shadow reaches past the mouse at each side, in pixels
    pub margin: u32,
    /// each light's group (see GROUPS): how much of it shows at each pixel,
    /// as white
    pub lights: Vec<Patch>,
    /// each button, 1 to 8, picked out as the chosen button is
    pub chosen: Vec<Patch>,
    /// each button's shape where it shows, as white, for the button under the pointer
    pub shapes: Vec<Patch>,
    /// the button, 1 to 8, at each pixel of the whole picture; 0 for none
    hit: Vec<u8>,
}

impl Layers {
    /// The button at a pixel of the whole picture.
    pub fn button_at(&self, x: f32, y: f32) -> Option<u8> {
        let (w, h) = (self.base.width(), self.base.height());
        if x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 {
            return None;
        }
        Some(self.hit[y as usize * w as usize + x as usize]).filter(|b| *b > 0)
    }

    /// The light (0 the logo, 1 the wheel) at a pixel of the whole picture,
    /// where it shows more than a little.
    pub fn light_at(&self, x: f32, y: f32) -> Option<usize> {
        let i = self.lights.iter().position(|p| {
            let (lx, ly) = (x - p.x as f32, y - p.y as f32);
            lx >= 0.0 && ly >= 0.0 && p.px.pixel(lx as u32, ly as u32).is_some_and(|c| c.alpha() > 40)
        })?;
        Some(GROUPS[i].0)
    }

    /// The whole picture as one, as the page puts it together: the lights
    /// in their colors and the button `chosen` (1 to 8) picked out.
    #[cfg(test)]
    pub fn flat(&self, lit: [Lit; 2], chosen: Option<u8>) -> Pixmap {
        let mut out = self.base.clone();
        if let Some(p) = chosen.and_then(|n| self.chosen.get((n as usize).wrapping_sub(1))) {
            out.draw_pixmap(p.x as i32, p.y as i32, p.px.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        for (p, (light, _)) in self.lights.iter().zip(GROUPS) {
            let Some(v) = lit[light] else { continue };
            let (c, k) = level(v);
            let mut tinted = p.px.clone();
            for px in tinted.data_mut().chunks_exact_mut(4) {
                let a = px[3] as f32 / 255.0 * k;
                for (j, ch) in c.iter().enumerate() {
                    px[j] = (*ch as f32 * a).round() as u8;
                }
                px[3] = (a * 255.0).round() as u8;
            }
            out.draw_pixmap(p.x as i32, p.y as i32, tinted.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        out
    }
}

/// A light's color as the page lays it through the light's mask: its hue at
/// full strength, and how strongly (the mouse's red at a quarter of full is
/// full red, a quarter laid on).
pub fn level(v: [u8; 3]) -> ([u8; 3], f32) {
    let top = v.into_iter().max().unwrap_or(0);
    if top == 0 {
        return ([0; 3], 0.0);
    }
    let k = top as f32 / 255.0;
    (v.map(|c| (c as f32 / k).round().min(255.0) as u8), k)
}

/// The picture's layers, the mouse `w` pixels wide (its height as the
/// drawing has it), over its shadow. Each of the drawings it takes is drawn
/// on a thread of its own.
pub fn layers(art: &str, w: u32, shadow: &[ectodeck_draw::hdr::ShadowLayer], opacity: f32) -> Result<Layers, String> {
    // the mouse with its lights off, then with each light's group alone white, then each button chosen
    let mut svgs = vec![dressed(art, [None; 4], None)];
    svgs.extend((0..GROUPS.len()).map(|g| dressed(art, std::array::from_fn(|i| (i == g).then_some([255, 255, 255])), None)));
    svgs.extend((1..=8).map(|n| dressed(art, [None; 4], Some(n))));
    let drawn: Vec<Result<(Pixmap, usvg::Tree, Transform), String>> = std::thread::scope(|s| {
        let all: Vec<_> = svgs.iter().map(|svg| s.spawn(move || render(svg, w))).collect();
        all.into_iter().map(|t| t.join().unwrap_or_else(|_| Err("the drawing failed".into()))).collect()
    });
    let mut drawn = drawn.into_iter().collect::<Result<Vec<_>, _>>()?.into_iter();
    let (off, tree, tf) = drawn.next().ok_or("no drawing")?;
    let lit: Vec<Pixmap> = drawn.by_ref().take(GROUPS.len()).map(|d| d.0).collect();
    let picked: Vec<Pixmap> = drawn.map(|d| d.0).collect();
    let (w, h) = (off.width(), off.height());

    // the shadow from the mouse's own outline, under it
    let cover: Vec<f32> = off.data().chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect();
    let s = ectodeck_draw::hdr::Shadow::of(&cover, w as usize, h as usize, shadow, opacity);
    let mut ground = Pixmap::new(s.w, s.h).ok_or("the mouse too small to draw")?;
    for (px, d) in ground.data_mut().chunks_exact_mut(4).zip(&s.dark) {
        px[3] = (d * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    let m = s.margin;
    let over_ground = |mouse: &Pixmap| {
        let mut out = ground.clone();
        out.draw_pixmap(m as i32, m as i32, mouse.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        out
    };
    let base = over_ground(&off);

    // each group's coverage: how much brighter the mouse is with the group
    // white, over how much brighter it could have got
    let lights = lit
        .iter()
        .map(|l| {
            let mut a = vec![0u8; (w * h) as usize];
            for (i, (o, l)) in off.data().chunks_exact(4).zip(l.data().chunks_exact(4)).enumerate() {
                // (measured on the channel with the most room to brighten, the steadiest)
                let c = (0..4).max_by_key(|&c| 255 - o[c] as i32).unwrap_or(0);
                let room = 255.0 - o[c] as f32;
                if room >= 1.0 {
                    a[i] = ((l[c] as f32 - o[c] as f32) / room * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
            crop(&a, w, h, m, |v| [v, v, v, v])
        })
        .collect();

    // each button picked out: where its drawing differs from the mouse's, over the shadow
    let chosen = picked
        .iter()
        .map(|p| {
            let differs: Vec<u8> = off.data().chunks_exact(4).zip(p.data().chunks_exact(4)).map(|(a, b)| u8::from(a != b) * 255).collect();
            let whole = over_ground(p);
            let shape = crop(&differs, w, h, m, |_| [0; 4]);
            let mut px = Pixmap::new(shape.px.width(), shape.px.height()).unwrap();
            px.draw_pixmap(-(shape.x as i32), -(shape.y as i32), whole.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
            Patch { px, ..shape }
        })
        .collect();

    // each button's shapes, filled where the drawing places them; a button
    // shows where no button over it covers it (they are drawn in their
    // numbers' order)
    let (bw, bh) = (base.width(), base.height());
    let at = Transform::from_translate(m as f32, m as f32).pre_concat(tf);
    let masks: Vec<Mask> = SURFACES
        .iter()
        .map(|ids| {
            let mut mask = Mask::new(bw, bh).unwrap();
            for id in *ids {
                if let Some(usvg::Node::Path(p)) = tree.node_by_id(id) {
                    let rule = match p.fill().map(|f| f.rule()) {
                        Some(usvg::FillRule::EvenOdd) => FillRule::EvenOdd,
                        _ => FillRule::Winding,
                    };
                    mask.fill_path(p.data(), rule, true, at.pre_concat(p.abs_transform()));
                }
            }
            mask
        })
        .collect();
    let n = (bw * bh) as usize;
    let mut hit = vec![0u8; n];
    let mut shows = vec![vec![0u8; n]; 8];
    for i in 0..n {
        let mut above = 1.0f32;
        for b in (0..8).rev() {
            let v = masks[b].data()[i] as f32 / 255.0;
            shows[b][i] = (v * above * 255.0).round() as u8;
            above *= 1.0 - v;
            if hit[i] == 0 && masks[b].data()[i] >= 128 {
                hit[i] = b as u8 + 1;
            }
        }
    }
    let shapes = shows.iter().map(|a| crop(a, bw, bh, 0, |v| [v, v, v, v])).collect();
    Ok(Layers { base, margin: m, lights, chosen, shapes, hit })
}

/// The part of a `w` by `h` map that isn't 0, as a patch of the whole
/// picture `m` pixels in from each side, its pixels made by `px`.
fn crop(a: &[u8], w: u32, h: u32, m: u32, px: impl Fn(u8) -> [u8; 4]) -> Patch {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if a[(y * w + x) as usize] > 0 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    if x1 <= x0 {
        return Patch { x: 0, y: 0, px: Pixmap::new(1, 1).unwrap() };
    }
    let mut out = Pixmap::new(x1 - x0, y1 - y0).unwrap();
    let ow = out.width();
    for (i, p) in out.data_mut().chunks_exact_mut(4).enumerate() {
        let (x, y) = (x0 + i as u32 % ow, y0 + i as u32 / ow);
        p.copy_from_slice(&px(a[(y * w + x) as usize]));
    }
    Patch { x: x0 + m, y: y0 + m, px: out }
}

/// The mouse drawn `w` pixels wide, with the tree it was drawn from and the
/// drawing's place on the pixels.
fn render(svg: &str, w: u32) -> Result<(Pixmap, usvg::Tree, Transform), String> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let k = w as f32 / VIEW.2;
    let h = (VIEW.3 * k).round() as u32;
    let mut mouse = Pixmap::new(w, h).ok_or("the mouse too small to draw")?;
    // (the drawing's units to the tree's: its size over its viewBox's)
    let units = tree.size().width() / view_width(svg).unwrap_or(tree.size().width());
    let t = k / units;
    let tf = Transform::from_row(t, 0.0, 0.0, t, -VIEW.0 * k, -VIEW.1 * k);
    resvg::render(&tree, tf, &mut mouse.as_mut());
    Ok((mouse, tree, tf))
}

/// The drawing as the page shows it: without its reference photo and guides,
/// each of the lights' groups (see GROUPS) white in its color, or gone, and
/// the chosen button outlined and a shade lighter.
fn dressed(art: &str, groups: [Lit; 4], chosen: Option<u8>) -> String {
    let mut s = art.to_string();
    for layer in ["reference", "guides"] {
        s = without_group(&s, layer);
    }
    for ((_, id), l) in GROUPS.iter().zip(groups) {
        s = recolor_group(&s, id, l);
    }
    if let Some(n) = chosen.filter(|n| (1..=8).contains(n)) {
        let sel: Vec<String> = SURFACES[n as usize - 1].iter().map(|id| format!("#{id}")).collect();
        let style = format!("<style>{} {{ fill: #5a5a62; stroke: #f4f4f5; stroke-width: 1.8; }}</style>", sel.join(", "));
        if let Some(i) = s.find("<defs") {
            s.insert_str(i, &style);
        }
    }
    s
}

/// How wide the drawing's viewBox is, in its own units.
fn view_width(svg: &str) -> Option<f32> {
    let root = &svg[svg.find("<svg")?..];
    let root = &root[..root.find('>')?];
    let vb = root.split("viewBox=\"").nth(1)?.split('"').next()?;
    vb.split_whitespace().nth(2)?.parse().ok()
}

/// The text of the element with this id, from its start to its end.
fn element(s: &str, id: &str) -> Option<(usize, usize)> {
    let at = s.find(&format!("id=\"{id}\""))?;
    let start = s[..at].rfind('<')?;
    let tag: String = s[start + 1..].chars().take_while(|c| c.is_alphanumeric() || *c == ':').collect();
    // the element's end: its own close, counting nested ones of the same tag
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let head_end = start + s[start..].find('>')? + 1;
    if s[..head_end].ends_with("/>") {
        return Some((start, head_end));
    }
    let mut depth = 1;
    let mut i = head_end;
    while depth > 0 {
        let next_open = s[i..].find(&open).map(|j| i + j);
        let next_close = s[i..].find(&close).map(|j| i + j)?;
        match next_open {
            Some(o) if o < next_close => {
                let e = o + s[o..].find('>')? + 1;
                if !s[..e].ends_with("/>") {
                    depth += 1;
                }
                i = e;
            }
            _ => {
                depth -= 1;
                i = next_close + close.len();
            }
        }
    }
    Some((start, i))
}

fn without_group(s: &str, id: &str) -> String {
    match element(s, id) {
        Some((a, b)) => format!("{}{}", &s[..a], &s[b..]),
        None => s.to_string(),
    }
}

fn recolor_group(s: &str, id: &str, lit: Lit) -> String {
    let Some((a, b)) = element(s, id) else { return s.to_string() };
    let inner = match lit {
        Some([r, g, bl]) => s[a..b].replace("fill=\"#ffffff\"", &format!("fill=\"#{r:02x}{g:02x}{bl:02x}\"")),
        None => s[a..b].replacen('>', " opacity=\"0\">", 1),
    };
    format!("{}{}{}", &s[..a], inner, &s[b..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art() -> String {
        std::fs::read_to_string(crate::cores::dir().join("steelseries/art/sensei-310.svg")).unwrap()
    }

    fn shadow() -> [ectodeck_draw::hdr::ShadowLayer; 1] {
        [ectodeck_draw::hdr::ShadowLayer { down: 8.0, blur: 22.0, strength: 0.6 }]
    }

    /// The mouse drawn: its lights in their colors, the chosen button picked
    /// out, its photo and guides left out, a shadow round it.
    #[test]
    fn the_mouse_is_drawn_lit_and_picked_out() {
        let l = layers(&art(), 240, &shadow(), 0.5).unwrap();
        let (red, off) = (l.flat([Some([255, 0, 0]); 2], None), l.flat([None, None], Some(7)));
        let m = l.margin;
        assert!(m > 0, "a shadow round it");
        let px = |p: &Pixmap, x: u32, y: u32| p.pixel(x, y).map(|c| (c.red(), c.green(), c.blue(), c.alpha())).unwrap();
        // on the logo's ring, in the drawing round (180.5, 482.1)
        let k = 240.0 / VIEW.2;
        let at = |x: f32, y: f32| (m + ((x - VIEW.0) * k) as u32, m + ((y - VIEW.1) * k) as u32);
        let (lx, ly) = at(152.5, 482.1);
        let (r, g, _, _) = px(&red, lx, ly);
        assert!(r > 150 && g < 60, "the logo lit red: {:?}", px(&red, lx, ly));
        let (r2, _, _, _) = px(&off, lx, ly);
        assert!(r2 < 90, "the logo dark when off: {:?}", px(&off, lx, ly));
        // button 7, the right side's front, picked out: lighter than when not
        let (bx, by) = at(304.5, 226.0);
        assert!(px(&off, bx, by).0 > px(&red, bx, by).0 + 20, "the chosen button lighter: {:?} vs {:?}", px(&off, bx, by), px(&red, bx, by));
        // and no photo: the drawing's corner outside the mouse is clear but for the shadow
        assert!(px(&red, 1, 1).3 < 10);
    }

    /// The lights laid through their masks look as the drawing does with its
    /// lights drawn in that color, within a few levels, at full strength and
    /// at a quarter (a quarter as the drawing with the lights' groups a
    /// quarter opaque).
    #[test]
    fn a_light_laid_on_matches_the_drawing_lit() {
        let art = art();
        let l = layers(&art, 240, &shadow(), 0.5).unwrap();
        let m = l.margin;
        for (c, k) in [([255u8, 16, 0], 1.0f32), ([0, 255, 255], 1.0), ([255, 16, 0], 0.25)] {
            let v = c.map(|x| (x as f32 * k).round() as u8);
            let ours = l.flat([Some(v); 2], None);
            // the drawing itself, its lights' groups in the color, at that strength
            let mut svg = dressed(&art, [Some(c); 4], None);
            if k < 1.0 {
                for (_, id) in GROUPS {
                    svg = svg.replacen(&format!("id=\"{id}\""), &format!("id=\"{id}\" opacity=\"{k}\""), 1);
                }
            }
            let (exact, _, _) = render(&svg, 240).unwrap();
            // compared on the mouse, where it is opaque and the shadow doesn't show
            let mut worst = 0i32;
            for y in 0..exact.height() {
                for x in 0..exact.width() {
                    let e = exact.pixel(x, y).unwrap();
                    if e.alpha() < 255 {
                        continue;
                    }
                    let o = ours.pixel(x + m, y + m).unwrap();
                    for (a, b) in [(o.red(), e.red()), (o.green(), e.green()), (o.blue(), e.blue())] {
                        worst = worst.max((a as i32 - b as i32).abs());
                    }
                }
            }
            assert!(worst <= 4, "{c:?} at {k}: {worst} levels off at worst");
        }
    }

    /// A click on the drawing finds the button there: the wheel on the
    /// wheel, the left and right buttons beside it, the CPI button behind
    /// the wheel, a side button, and nothing on the palm; and the logo's
    /// light on the logo.
    #[test]
    fn a_click_on_the_drawing_finds_its_button() {
        let l = layers(&art(), 240, &shadow(), 0.5).unwrap();
        let k = 240.0 / VIEW.2;
        let at = |x: f32, y: f32| (l.margin as f32 + (x - VIEW.0) * k, l.margin as f32 + (y - VIEW.1) * k);
        let b = |x, y| {
            let (px, py) = at(x, y);
            l.button_at(px, py)
        };
        assert_eq!(b(180.0, 120.0), Some(3), "the wheel");
        assert_eq!(b(120.0, 120.0), Some(1), "the left button");
        assert_eq!(b(240.0, 120.0), Some(2), "the right button");
        assert_eq!(b(180.0, 226.0), Some(8), "the CPI button");
        assert_eq!(b(55.0, 226.0), Some(5), "the left side's front");
        assert_eq!(b(180.0, 400.0), None, "the palm");
        let (lx, ly) = at(152.5, 482.1);
        assert_eq!(l.light_at(lx, ly), Some(0), "the logo");
    }
}

#[cfg(test)]
mod timing {
    #[test]
    #[ignore]
    fn how_long_the_layers_take() {
        let art = std::fs::read_to_string(crate::cores::dir().join("steelseries/art/sensei-310.svg")).unwrap();
        let layers = [ectodeck_draw::hdr::ShadowLayer { down: 16.0, blur: 44.0, strength: 0.6 }, ectodeck_draw::hdr::ShadowLayer { down: 4.0, blur: 14.0, strength: 0.35 }];
        for w in [240u32, 480] {
            let t = std::time::Instant::now();
            let _ = super::layers(&art, w, &layers, 0.5).unwrap();
            eprintln!("{w} px wide: the layers in {:?}", t.elapsed());
        }
    }
}

