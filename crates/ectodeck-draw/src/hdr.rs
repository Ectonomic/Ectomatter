//! The keyboard as one HDR picture, drawn from the Z10 core's art.
//!
//! Everything the drawing shows under and on the keys is flattened into one
//! picture: the body and its print, the caps' shadows, the light each key's
//! LED throws on the deck, the caps, and the legends lit from behind. What
//! never changes is drawn once for the keyboard's size; each key's light is
//! worked out once too, from where its LED sits under the cap and the shape of
//! the cap (see the core's "light"), and filled with the key's colour each time
//! the lighting changes. Light is added in linear light, and the lit legends
//! go brighter than the desktop's white.
//!
//! The picture comes out as the HDR layer wants it: PQ, BT.2020, 16 bits a
//! channel, premultiplied. SDR colours are decoded as KWin decodes the page's
//! own (gamma 2.2 at the reference white), so the keyboard meets the page
//! round it without a seam.

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{self, Tree};
use serde_json::Value;

/// One key's light, as the page has it: its colour (sRGB) and how strongly it
/// is lit (0 to 1), and whether the key is remapped, which steps its legend back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub pos: u32,
    pub rgb: [u8; 3],
    pub strength: f32,
    pub dim: bool,
}

/// A shape on the board: how much of something there is at each pixel, in
/// its own small box.
struct Shape {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    cover: Vec<f32>,
}

/// A lit key's light, worked out from where its LED sits and the shape of its cap.
struct KeyArt {
    pos: u32,
    /// the cap's foot (its skirt's outline) and its face, in the art's units
    foot: Option<[f32; 4]>,
    face: [f32; 4],
    /// the glow its LED throws on the deck round the foot of the cap: 1 where it is brightest
    deck: Option<Shape>,
    /// the deck's glow caught by the foot of the cap's skirt
    rim: Option<Shape>,
    /// the legend's coverage, and how brightly the LED lights each pixel of it (0 to 1)
    legend: Option<Shape>,
    legend_lit: Vec<f32>,
    /// the grey its legend is printed in, in sRGB's encoding
    print: [f32; 3],
}

/// How a key's LED light gets out, from the core's "light" (brightness in
/// nits of luminance for a key lit fully, lengths in the art's units).
#[derive(Clone, Copy, Debug)]
struct Model {
    /// how far north of the key's centre the LED sits
    led: f32,
    /// the legend at its brightest, nearest the LED
    legend_nits: f32,
    /// the display's lit dots
    lcd_nits: f32,
    /// how deep under the legend the LED is, which sets how fast the legend dims away from it
    legend_depth: f32,
    /// the least a legend is lit, however far from the LED, by light bouncing inside the cap
    legend_floor: f32,
    /// the glow on the deck at its brightest, just outside the skirt on the LED's side
    deck_nits: f32,
    /// how high above the deck the light leaves the cap, which sets how much brighter
    /// the skirt's LED side is than its far side
    deck_height: f32,
    /// the least any part of the skirt lets out, by light bouncing inside the cap
    deck_floor: f32,
    /// how far from the skirt the glow on the deck falls to a third
    deck_reach: f32,
    /// the glow the foot of the skirt catches at its brightest
    rim_nits: f32,
    /// how far up the skirt its foot catches the deck's glow
    rim_width: f32,
}

impl Model {
    fn from(l: &Value) -> Model {
        let f = |v: &Value, d: f32| v.as_f64().map_or(d, |v| v as f32);
        let legend_nits = f(&l["legend"]["nits"], 220.0).max(0.0);
        // a part's own nits, or (as the core once had it) a share of the legends'
        let nits = |part: &str, d: f32| l[part]["nits"].as_f64().map(|v| v as f32).or_else(|| l[part]["share"].as_f64().map(|v| v as f32 * legend_nits)).unwrap_or(d).max(0.0);
        Model {
            led: f(&l["led"], 7.0),
            legend_nits,
            lcd_nits: f(&l["lcd"]["nits"], 120.0).max(0.0),
            legend_depth: f(&l["legend"]["depth"], 10.0),
            legend_floor: f(&l["legend"]["floor"], 0.35),
            deck_nits: nits("deck", 18.0),
            deck_height: f(&l["deck"]["height"], 7.0),
            deck_floor: f(&l["deck"]["floor"], 0.15),
            deck_reach: f(&l["deck"]["reach"], 2.0).max(0.05),
            rim_nits: nits("rim", 13.0),
            rim_width: f(&l["rim"]["width"], 0.8).max(0.05),
        }
    }

    /// Each part's brightness at its brightest, in the SDR white's units, up
    /// to Peak nits: the legend's, the deck's, the skirt foot's.
    fn tops(&self, white: f32, ceiling: f32) -> (f32, f32, f32) {
        let t = |nits: f32| (nits / white.max(1.0)).min(ceiling);
        (t(self.legend_nits), t(self.deck_nits), t(self.rim_nits))
    }
}

/// How much of a small source's light falls on a surface `h` below it, at `r`
/// along the surface from the point straight below it: 1 there, falling as
/// (h² / (h² + r²))^1.5, as a lamp's light falls across a floor.
fn falloff(r: f32, h: f32) -> f32 {
    let q = h * h / (h * h + r * r);
    q * q.sqrt()
}

/// The keyboard at one size, ready to light.
pub struct Board {
    pub w: u32,
    pub h: u32,
    /// the body and its print over the ground's colour: opaque, sRGB-encoded
    under: Vec<f32>,
    /// the caps' shadows on the body, from the room's light, which the LEDs' light is not under
    shadow: Vec<f32>,
    /// the caps, and the legends of unlit keys: premultiplied, sRGB-encoded
    caps: Vec<f32>,
    keys: Vec<KeyArt>,
    model: Model,
    /// where the deck is open to the keys' light, a pixel's coverage
    wells: Option<Vec<f32>>,
    /// the art's own units (the SVG's, which the core's lengths are in) to the
    /// board's pixels: pixel = offset + scale x unit
    pub(crate) art_scale: f32,
    pub(crate) art_offset: (f32, f32),
}

/// sRGB (as the page's colours are) to linear light, as KWin decodes the page: gamma 2.2.
fn lin(v: u8) -> f32 {
    (v as f32 / 255.0).powf(2.2)
}

/// One key's light, from where its LED sits and the shape of its cap: the
/// glow on the deck, the skirt's foot, and how brightly each pixel of its
/// legend is lit. `ka` and (`ox`, `oy`) take the art's units to the board's pixels.
#[allow(clippy::too_many_arguments)]
fn light_key(key: &mut KeyArt, model: &Model, wells: Option<&[f32]>, w: u32, h: u32, ka: f32, ox: f32, oy: f32) {
    let unit = |x: i32, y: i32| ((x as f32 + 0.5 - ox) / ka, (y as f32 + 0.5 - oy) / ka);
    let Some(foot) = key.foot else {
        key.legend_lit = key.legend.as_ref().map_or(Vec::new(), |s| vec![1.0; s.cover.len()]);
        return;
    };
    let face = key.face;
    let well = |x: i32, y: i32| wells.map_or(1.0, |m| m[(y as u32 * w + x as u32) as usize]);
    let led = ((foot[0] + foot[2]) / 2.0, (foot[1] + foot[3]) / 2.0 - model.led);
    // how much light leaves the skirt at a point on its outline
    let skirt = |px: f32, py: f32| {
        let r = ((px - led.0).powi(2) + (py - led.1).powi(2)).sqrt();
        model.deck_floor + (1.0 - model.deck_floor) * falloff(r, model.deck_height)
    };
    // the outline's point nearest the LED, where the most light gets out: on
    // the top edge for a key as wide as it is tall or wider, on a side for a tall one
    let nearest = [
        (led.0 - foot[0], (foot[0], led.1)),
        (foot[2] - led.0, (foot[2], led.1)),
        (led.1 - foot[1], (led.0, foot[1])),
        (foot[3] - led.1, (led.0, foot[3])),
    ]
    .into_iter()
    .fold((f32::MAX, (led.0, foot[1])), |b, (d, p)| if d.abs() < b.0 { (d.abs(), p) } else { b })
    .1;
    let brightest = skirt(nearest.0, nearest.1);
    // a box of the board's pixels round the cap, `m` units wider each side
    let span = |m: f32| {
        let x0 = (ox + ka * (foot[0] - m)).floor().max(0.0) as i32;
        let y0 = (oy + ka * (foot[1] - m)).floor().max(0.0) as i32;
        let x1 = (ox + ka * (foot[2] + m)).ceil().min(w as f32) as i32;
        let y1 = (oy + ka * (foot[3] + m)).ceil().min(h as f32) as i32;
        (x0, y0, (x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32)
    };

    // the deck: the light out from under the skirt, falling away from it
    let (x0, y0, dw, dh) = span((model.deck_reach * 6.0 + 1.0).min(40.0));
    let mut cover = vec![0f32; (dw * dh) as usize];
    for j in 0..dh as i32 {
        for i in 0..dw as i32 {
            let (x, y) = (x0 + i, y0 + j);
            let (u, v) = unit(x, y);
            let (pu, pv) = (u.clamp(foot[0], foot[2]), v.clamp(foot[1], foot[3]));
            let d = ((u - pu).powi(2) + (v - pv).powi(2)).sqrt();
            let e = (skirt(pu, pv) / brightest).min(1.0) * (-d / model.deck_reach).exp() * well(x, y);
            cover[(j as u32 * dw + i as u32) as usize] = e.max(0.0);
        }
    }
    key.deck = Some(Shape { x: x0, y: y0, w: dw, h: dh, cover });

    // the rim: the foot of the skirt, round the face, catching the deck's glow
    let (x0, y0, rw, rh) = span(0.0);
    let mut cover = vec![0f32; (rw * rh) as usize];
    for j in 0..rh as i32 {
        for i in 0..rw as i32 {
            let (u, v) = unit(x0 + i, y0 + j);
            let sides = [u - foot[0], foot[2] - u, v - foot[1], foot[3] - v];
            let t = sides.iter().copied().fold(f32::MAX, f32::min);
            // outside the face, inside the foot, their edges shared out by coverage
            let off_face = (face[0] - u).max(u - face[2]).max(face[1] - v).max(v - face[3]);
            let a = (off_face * ka + 0.5).clamp(0.0, 1.0) * (t * ka + 0.5).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            // the nearest point of the foot's outline
            let (pu, pv) = match sides.iter().enumerate().fold((0, f32::MAX), |b, (n, s)| if *s < b.1 { (n, *s) } else { b }).0 {
                0 => (foot[0], v),
                1 => (foot[2], v),
                2 => (u, foot[1]),
                _ => (u, foot[3]),
            };
            let e = (skirt(pu, pv) / brightest).min(1.0) * (-t.max(0.0) / model.rim_width).exp();
            cover[(j as u32 * rw + i as u32) as usize] = a * e;
        }
    }
    key.rim = Some(Shape { x: x0, y: y0, w: rw, h: rh, cover });

    // the legend, brightest nearest the LED under it
    key.legend_lit = key.legend.as_ref().map_or(Vec::new(), |s| {
        (0..s.cover.len())
            .map(|i| {
                let (u, v) = unit(s.x + (i as u32 % s.w) as i32, s.y + (i as u32 / s.w) as i32);
                let r = ((u - led.0).powi(2) + (v - led.1).powi(2)).sqrt();
                model.legend_floor + (1.0 - model.legend_floor) * falloff(r, model.legend_depth)
            })
            .collect()
    });
}

/// A key's light: its colour in linear light, its luminance how strongly the
/// key is lit. Nothing holds it back: a part asked for more than the screen
/// can show in that colour (a saturated red's channel passes the screen's
/// peak well before its luminance does) is sent as asked, and the screen
/// clips it there.
fn key_colour(l: &Light) -> [f32; 3] {
    let c = [lin(l.rgb[0]), lin(l.rgb[1]), lin(l.rgb[2])];
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    if y <= 0.0 {
        return [0.0; 3];
    }
    c.map(|v| v / y * l.strength.clamp(0.0, 1.0))
}

/// How bright a key's colour goes, in the SDR white's units, before one of
/// its channels passes `ceiling`: past it the screen clips that channel.
fn clean_limit(l: &Light, ceiling: f32) -> f32 {
    let c = [lin(l.rgb[0]), lin(l.rgb[1]), lin(l.rgb[2])];
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let widest = c[0].max(c[1]).max(c[2]);
    if widest <= 0.0 { 0.0 } else { ceiling * y / widest }
}

/// A clip path somewhere under `g`, by its id, with the group it clips.
fn find_clip<'a>(g: &'a usvg::Group, id: &str) -> Option<(&'a usvg::Group, &'a usvg::ClipPath)> {
    if let Some(c) = g.clip_path().filter(|c| c.id() == id) {
        return Some((g, c));
    }
    g.children().iter().find_map(|n| match n {
        usvg::Node::Group(g) => find_clip(g, id),
        _ => None,
    })
}

impl Board {
    /// Draw what never changes, and work out each lit key's light, for a board
    /// `w` x `h` pixels: the drawing fitted into it, keeping its proportions.
    pub fn new(tree: &Tree, device: &Value, base: [u8; 3], w: u32, h: u32) -> Result<Board, String> {
        let size = tree.size();
        let k = (w as f32 / size.width()).min(h as f32 / size.height());
        let (tx, ty) = ((w as f32 - size.width() * k) / 2.0, (h as f32 - size.height() * k) / 2.0);
        let node = |id: &str| tree.node_by_id(id);
        // Where a piece goes, through `onto` (the drawing's canvas to pixels).
        // resvg draws a lone piece with its own transform only, not those of
        // the groups it sits in, and first shifts it so its box starts at 0,0;
        // this gives it the groups' transforms and cancels the shift.
        let placed = |n: &usvg::Node, onto: Transform| -> Option<Transform> {
            let outer = match n {
                usvg::Node::Group(g) => g.abs_transform().pre_concat(g.transform().invert()?),
                _ => n.abs_transform(),
            };
            let bb = n.abs_layer_bounding_box()?;
            Some(onto.pre_concat(outer).pre_translate(bb.x(), bb.y()))
        };
        let board = Transform::from_row(k, 0.0, 0.0, k, tx, ty);
        let draw_node = |pix: &mut Pixmap, n: &usvg::Node| {
            if let Some(t) = placed(n, board) {
                resvg::render_node(n, t, &mut pix.as_mut());
            }
        };
        // draw one piece onto a pixmap the size of the board
        let draw = |pix: &mut Pixmap, id: &str| {
            if let Some(n) = node(id) {
                draw_node(pix, n);
            }
        };
        // cut one piece's coverage, in its own box
        let shape = |id: &str| -> Option<Shape> {
            let n = node(id)?;
            let bb = n.abs_layer_bounding_box()?;
            let (fx, fy) = (tx + k * bb.x(), ty + k * bb.y());
            let (x, y) = (fx.floor() as i32, fy.floor() as i32);
            let (sw, sh) = ((k * bb.width()).ceil() as u32 + 2, (k * bb.height()).ceil() as u32 + 2);
            let mut pix = Pixmap::new(sw, sh)?;
            let t = placed(n, Transform::from_row(k, 0.0, 0.0, k, tx - x as f32, ty - y as f32))?;
            resvg::render_node(n, t, &mut pix.as_mut());
            let cover = pix.data().chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect();
            Some(Shape { x, y, w: sw, h: sh, cover })
        };
        let children = |id: &str| -> Vec<usvg::Node> {
            match node(id) {
                Some(usvg::Node::Group(g)) => g.children().to_vec(),
                _ => Vec::new(),
            }
        };

        let controls = device["controls"].as_array().ok_or("the device names no controls")?;
        let lit = |c: &Value| c["illumination"].is_array();
        let sliders = |c: &Value| c.get("slider").is_some();
        let model = Model::from(&device["light"]);

        // under the light: the body and its print, over the ground
        let mut pix = Pixmap::new(w, h).ok_or("board too small")?;
        pix.fill(resvg::tiny_skia::Color::from_rgba8(base[0], base[1], base[2], 255));
        draw(&mut pix, "body");
        draw(&mut pix, "legend-body");
        let under = pix.data().chunks_exact(4).flat_map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]).collect();

        // Every cap but the slider knobs (the page moves those): its shadow (its
        // first piece) apart from the rest, since the shadow is the room's light
        // blocked, and the LED's light comes from under the cap; and the legends
        // of keys with no light behind them.
        let mut shade = Pixmap::new(w, h).ok_or("board too small")?;
        let mut pix = Pixmap::new(w, h).ok_or("board too small")?;
        for c in controls.iter().filter(|c| !sliders(c)) {
            let Some(n) = c["control"].as_u64() else { continue };
            let id = format!("control_{n}");
            let parts = children(&id);
            if parts.len() >= 3 {
                draw_node(&mut shade, &parts[0]);
                for p in &parts[1..] {
                    draw_node(&mut pix, p);
                }
            } else {
                draw(&mut pix, &id);
            }
        }
        for c in controls.iter().filter(|c| !lit(c)) {
            if let Some(n) = c["legend"].as_u64() {
                draw(&mut pix, &format!("legend_{n}"));
            }
        }
        let shadow: Vec<f32> = shade.data().chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect();
        // premultiplied, in sRGB's own encoding
        let caps = pix.data().chunks_exact(4).flat_map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0]).collect();

        // The wells, where the deck is open to the keys' light: a clip in the
        // art, in the art's own units, as the group it clips has them. That
        // group's transform takes the art's units to the drawing's; the core's
        // lengths are in the art's units.
        let wells_id = device["light"]["wells"].as_str().unwrap_or("mask_1");
        let found = find_clip(tree.root(), wells_id);
        let art = found.map_or(Transform::identity(), |(g, _)| g.abs_transform());
        let wells: Option<Vec<f32>> = found.and_then(|(g, clip)| {
            let mut m = Pixmap::new(w, h)?;
            let mut paint = resvg::tiny_skia::Paint::default();
            paint.set_color_rgba8(255, 255, 255, 255);
            paint.anti_alias = true;
            for n in clip.root().children() {
                if let usvg::Node::Path(p) = n {
                    let t = board.pre_concat(g.abs_transform()).pre_concat(clip.transform()).pre_concat(p.abs_transform());
                    m.fill_path(p.data(), &paint, resvg::tiny_skia::FillRule::Winding, t, None);
                }
            }
            Some(m.data().chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect())
        });
        // the art's units to the board's pixels
        let ka = k * art.sx;
        let (ox, oy) = (tx + k * art.tx, ty + k * art.ty);
        // each lit key: its legend, and its cap's shape, which its light is worked out from
        let mut keys = Vec::new();
        for c in controls.iter().filter(|c| lit(c)) {
            let Some(pos) = c["pos"].as_u64() else { continue };
            let legend_n = c["legend"].as_u64();
            let print = legend_n.and_then(|n| tree.node_by_id(&format!("legend_{n}"))).and_then(first_fill).unwrap_or([0x9e, 0x9e, 0x9e]);
            let print = [print[0] as f32 / 255.0, print[1] as f32 / 255.0, print[2] as f32 / 255.0];
            let legend = legend_n.and_then(|n| shape(&format!("legend_{n}")));
            let parts = c["control"].as_u64().map(|n| children(&format!("control_{n}"))).unwrap_or_default();
            let rect = |n: &usvg::Node| {
                let b = n.abs_bounding_box();
                [(b.left() - art.tx) / art.sx, (b.top() - art.ty) / art.sy, (b.right() - art.tx) / art.sx, (b.bottom() - art.ty) / art.sy]
            };
            let (foot, face) = match (parts.get(1).map(rect), parts.get(2).map(rect)) {
                (Some(f), Some(a)) => (Some(f), a),
                _ => (None, [0.0; 4]),
            };
            keys.push(KeyArt { pos: pos as u32, foot, face, deck: None, rim: None, legend, legend_lit: Vec::new(), print });
        }
        let mut board = Board { w, h, under, shadow, caps, keys, model, wells, art_scale: ka, art_offset: (ox, oy) };
        board.relight();
        Ok(board)
    }

    /// Light the keyboard by another of the core's "light" descriptions (as the
    /// Lighting lab tunes it): each key's light is worked out again from its
    /// cap; the drawing itself stays as it is.
    pub fn set_light(&mut self, light: &Value) {
        self.model = Model::from(light);
        self.relight();
    }

    /// The legends' brightest luminance, in nits, and the display's lit dots'.
    pub fn legend_nits(&self) -> f32 {
        self.model.legend_nits
    }
    pub fn lcd_nits(&self) -> f32 {
        self.model.lcd_nits
    }

    /// Each key's light from its LED and its cap, on every core at once.
    fn relight(&mut self) {
        let Board { w, h, keys, model, wells, art_scale, art_offset, .. } = self;
        let (w, h, model, wells, ka, (ox, oy)) = (*w, *h, *model, wells.as_deref(), *art_scale, *art_offset);
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
        let per = keys.len().div_ceil(threads).max(1);
        std::thread::scope(|sc| {
            for chunk in keys.chunks_mut(per) {
                sc.spawn(move || {
                    for key in chunk {
                        light_key(key, &model, wells, w, h, ka, ox, oy);
                    }
                });
            }
        });
    }

    /// The brightest light on the keyboard with these lights, in nits, with
    /// the SDR white at `white` nits and Peak nits at `ceiling` times it: the
    /// legends', the deck's, and the brightest the lit keys' colour shows
    /// cleanly, before the screen clips one of its channels.
    pub fn peaks(&self, lights: &[Light], white: f32, ceiling: f32) -> (f32, f32, f32) {
        let (legend, deck, _) = self.model.tops(white, ceiling);
        let lit: Vec<&Light> = lights.iter().filter(|l| self.keys.iter().any(|k| k.pos == l.pos)).collect();
        let strongest = lit.iter().map(|l| l.strength.clamp(0.0, 1.0)).fold(0.0f32, f32::max);
        let clean = lit.iter().map(|l| clean_limit(l, ceiling) * l.strength.clamp(0.0, 1.0)).fold(0.0f32, f32::max);
        (strongest * legend * white, strongest * deck * white, clean * white)
    }

    /// The places of the keys that light.
    pub fn positions(&self) -> impl Iterator<Item = u32> + '_ {
        self.keys.iter().map(|k| k.pos)
    }

    /// The picture with this light: premultiplied linear RGBA, the SDR white
    /// at 1.0 (`white` nits). Each part of the light (the legends, the deck's
    /// glow, the skirts' feet) is at its brightest the core's nits for it, in
    /// luminance, whatever its colour, up to Peak nits (`ceiling` times the
    /// white). Nothing else holds it back: a colour asked for brighter than the
    /// screen can show it goes as asked, and the screen clips it.
    ///
    /// The light is each key's LED, under its cap: it comes out through the
    /// legend, brightest nearest the LED, and escapes under the skirt onto the
    /// deck, brightest at the skirt on the LED's side and fading within a
    /// couple of millimetres; the foot of the skirt catches a little of the
    /// deck's glow. The caps' shadows are the room's light blocked, so the
    /// LEDs' light is not under them. A key's brightness scales its nits,
    /// never its shape.
    pub fn compose(&self, lights: &[Light], white: f32, ceiling: f32) -> Vec<f32> {
        let (w, h) = (self.w as usize, self.h as usize);
        let mut out = vec![0f32; w * h * 4];
        // in bands of rows, one a thread: every step below is the pixel's own
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
        let rows = h.div_ceil(threads).max(1);
        std::thread::scope(|sc| {
            for (b, band) in out.chunks_mut(rows * w * 4).enumerate() {
                let y0 = b * rows;
                sc.spawn(move || self.compose_rows(lights, white, ceiling, y0, band));
            }
        });
        out
    }

    fn compose_rows(&self, lights: &[Light], white: f32, ceiling: f32, y0: usize, out: &mut [f32]) {
        let w = self.w as usize;
        let n = out.len() / 4;
        let y1 = y0 + n / w;
        let dec = |v: f32| v.max(0.0).powf(2.2);
        let (legend_top, deck_top, rim_top) = self.model.tops(white, ceiling);
        // the body and its print, in linear light, opaque, under the caps' shadows
        for (i, px) in out.chunks_exact_mut(4).enumerate() {
            let j = y0 * w + i;
            let lit = 1.0 - self.shadow[j];
            for c in 0..3 {
                px[c] = dec(self.under[j * 3 + c]) * lit;
            }
            px[3] = 1.0;
        }
        let light = |pos: u32| lights.iter().find(|l| l.pos == pos);
        let colour = |l: &Light, _top: f32| key_colour(l);
        // each pixel of a shape in this band: its place here, its index in the shape, and its value
        let each = |s: &Shape, f: &mut dyn FnMut(usize, usize, f32)| {
            for row in 0..s.h as i32 {
                let y = s.y + row;
                if y < y0 as i32 || y >= y1 as i32 {
                    continue;
                }
                for col in 0..s.w as i32 {
                    let x = s.x + col;
                    if x < 0 || x >= w as i32 {
                        continue;
                    }
                    let i = (row * s.w as i32 + col) as usize;
                    let a = s.cover[i];
                    if a > 0.0 {
                        f((y as usize - y0) * w + x as usize, i, a);
                    }
                }
            }
        };
        // light given off, added as it is
        let shine = |out: &mut [f32], em: &mut [[f32; 3]]| {
            for (d, px) in out.chunks_exact_mut(4).enumerate() {
                for i in 0..3 {
                    px[i] += em[d][i];
                }
                em[d] = [0.0; 3];
            }
        };
        let mut em = vec![[0f32; 3]; n];
        let add = |em: &mut [[f32; 3]], s: &Shape, c: [f32; 3], k: f32, lit: Option<&[f32]>| {
            if k <= 0.0 {
                return;
            }
            each(s, &mut |d, i, a| {
                let a = a * k * lit.map_or(1.0, |l| l[i]);
                for ch in 0..3 {
                    em[d][ch] += c[ch] * a;
                }
            });
        };

        // the glow on the deck, from the light escaping under each cap
        for key in &self.keys {
            if let (Some(l), Some(s)) = (light(key.pos), &key.deck) {
                add(&mut em, s, colour(l, deck_top), deck_top, None);
            }
        }
        shine(out, &mut em);
        // the caps over it
        for (i, px) in out.chunks_exact_mut(4).enumerate() {
            let cap = &self.caps[(y0 * w + i) * 4..(y0 * w + i) * 4 + 4];
            let a = cap[3];
            if a <= 0.0 {
                continue;
            }
            for c in 0..3 {
                px[c] = dec(cap[c] / a) * a + px[c] * (1.0 - a);
            }
        }
        // each lit legend's print, which shows less as the light behind it outshines it
        for key in &self.keys {
            if let Some(s) = &key.legend {
                let l = light(key.pos);
                let back = if l.is_some_and(|l| l.dim) { 0.35 } else { 1.0 };
                let lit = l.map_or(0.0, |l| l.strength.clamp(0.0, 1.0));
                let print = [0, 1, 2].map(|c| dec(key.print[c]) * (1.0 - lit));
                each(s, &mut |d, _, a| {
                    let a = a * back;
                    for c in 0..3 {
                        out[d * 4 + c] = out[d * 4 + c] * (1.0 - a) + print[c] * a;
                    }
                });
            }
        }
        // the light through the legends, the brightest thing, and the skirts' feet
        for key in &self.keys {
            let Some(l) = light(key.pos) else { continue };
            if let Some(s) = &key.rim {
                add(&mut em, s, colour(l, rim_top), rim_top, None);
            }
            if let Some(s) = &key.legend {
                add(&mut em, s, colour(l, legend_top), legend_top * if l.dim { 0.35 } else { 1.0 }, Some(&key.legend_lit));
            }
        }
        shine(out, &mut em);
    }
}

/// The first fill colour in a piece: the grey a legend is printed in.
fn first_fill(node: &usvg::Node) -> Option<[u8; 3]> {
    match node {
        usvg::Node::Path(p) => match p.fill()?.paint() {
            usvg::Paint::Color(c) => Some([c.red, c.green, c.blue]),
            _ => None,
        },
        usvg::Node::Group(g) => g.children().iter().find_map(first_fill),
        _ => None,
    }
}

/// PQ signal for nits
fn pq(nits: f64) -> f64 {
    let (m1, m2) = (2610.0 / 16384.0, 2523.0 / 4096.0 * 128.0);
    let (c1, c2, c3) = (3424.0 / 4096.0, 2413.0 / 4096.0 * 32.0, 2392.0 / 4096.0 * 32.0);
    let y = (nits.max(0.0) / 10000.0).powf(m1);
    ((c1 + c2 * y) / (1.0 + c3 * y)).powf(m2)
}

/// Linear light to the HDR layer's pixels, quickly: PQ looked up in a table
/// spaced by the fourth root of brightness, which puts most of its entries in
/// the dark, where PQ changes fastest.
pub struct Encoder {
    table: Vec<f32>,
    reference: f32,
}

const STEPS: usize = 8192;

impl Encoder {
    pub fn new(reference: f64) -> Encoder {
        let table = (0..=STEPS).map(|i| pq(10000.0 * (i as f64 / STEPS as f64).powi(4)) as f32).collect();
        Encoder { table, reference: reference as f32 }
    }

    fn signal(&self, nits: f32) -> f32 {
        let u = (nits.max(0.0) / 10000.0).sqrt().sqrt().min(1.0) * STEPS as f32;
        let i = (u as usize).min(STEPS - 1);
        let f = u - i as f32;
        self.table[i] * (1.0 - f) + self.table[i + 1] * f
    }

    /// One pixel: premultiplied linear RGBA (SDR white 1.0, BT.709) to
    /// premultiplied PQ, BT.2020, as the channels of ABGR16161616
    pub fn pixel(&self, px: [f32; 4]) -> [u16; 4] {
        const M: [[f32; 3]; 3] = [[0.6274, 0.3293, 0.0433], [0.0691, 0.9195, 0.0114], [0.0164, 0.0880, 0.8956]];
        let a = px[3];
        if a <= 0.0 {
            return [0; 4];
        }
        let c = [px[0] / a, px[1] / a, px[2] / a];
        let mut out = [0u16; 4];
        for (o, row) in out.iter_mut().zip(M) {
            let v = row[0] * c[0] + row[1] * c[1] + row[2] * c[2];
            *o = (self.signal(v * self.reference) * a * 65535.0).round().clamp(0.0, 65535.0) as u16;
        }
        out[3] = (a.min(1.0) * 65535.0).round() as u16;
        out
    }

    /// premultiplied linear RGBA (SDR white 1.0, BT.709) to premultiplied
    /// ABGR16161616, PQ, BT.2020
    pub fn encode(&self, frame: &[f32]) -> Vec<u8> {
        let mut out = Vec::with_capacity(frame.len() * 2);
        for px in frame.chunks_exact(4) {
            for v in self.pixel([px[0], px[1], px[2], px[3]]) {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Z10's board lit by a light of the tests' own, whatever the core is
    /// tuned to: legends at 600 nits, the deck's glow 90, the skirts' feet 36.
    fn board() -> (Tree, Value, Board) {
        let tree = Tree::from_str(crate::z10::SVG, &usvg::Options::default()).unwrap();
        let mut device: Value = serde_json::from_str(crate::z10::DEVICE).unwrap();
        device["light"] = serde_json::json!({
            "led": 7.0,
            "legend": { "nits": 600, "depth": 10.0, "floor": 0.35 },
            "deck": { "nits": 90, "height": 7.0, "floor": 0.15, "reach": 2.0 },
            "rim": { "nits": 36, "width": 0.8 },
            "lcd": { "nits": 400 },
            "wells": "mask_1"
        });
        let b = Board::new(&tree, &device, [0x20, 0x20, 0x20], 1264, 506).unwrap();
        (tree, device, b)
    }

    #[test]
    fn the_keyboard_comes_out_whole_and_lit() {
        let t = std::time::Instant::now();
        let (_, _, b) = board();
        let made = t.elapsed();
        assert!(b.keys.len() >= 100, "lit keys: {}", b.keys.len());
        let stat = |f: &dyn Fn(&KeyArt) -> Option<&Shape>| {
            let v: Vec<&Shape> = b.keys.iter().filter_map(|k| f(k)).collect();
            let max = v.iter().flat_map(|s| s.cover.iter()).fold(0f32, |m, c| m.max(*c));
            (v.len(), max)
        };
        eprintln!("shapes: deck {:?} rim {:?} legend {:?}", stat(&|k| k.deck.as_ref()), stat(&|k| k.rim.as_ref()), stat(&|k| k.legend.as_ref()));
        assert!(b.keys.iter().all(|k| k.deck.is_some() && k.rim.is_some()), "every lit key has its deck glow and rim");
        let lights: Vec<Light> = b.keys.iter().map(|k| Light { pos: k.pos, rgb: [255, 45, 0], strength: 1.0, dim: false }).collect();
        let t = std::time::Instant::now();
        let frame = b.compose(&lights, 254.0, 1000.0 / 254.0);
        let composed = t.elapsed();
        let enc = Encoder::new(254.0);
        let t = std::time::Instant::now();
        let px = enc.encode(&frame);
        let encoded = t.elapsed();
        assert_eq!(px.len(), (1264 * 506 * 8) as usize);
        eprintln!("made in {made:?}, lit in {composed:?}, encoded in {encoded:?}");
        // the brightest part of the lit keyboard goes beyond the desktop's white
        let brightest = frame.chunks_exact(4).map(|p| p[0] / p[3].max(1e-6)).fold(0f32, f32::max);
        assert!(brightest > 1.2, "the light should go past SDR white: {brightest}");
        if let Some(dir) = std::env::var_os("ECTODECK_HDR_DUMP") {
            std::fs::write(std::path::Path::new(&dir).join("hdr-keyboard-1264x506.raw"), &px).unwrap();
        }
    }

    /// The value of a shape at a board pixel, or 0 outside it.
    fn at(s: &Shape, x: i32, y: i32) -> f32 {
        if x < s.x || y < s.y || x >= s.x + s.w as i32 || y >= s.y + s.h as i32 {
            return 0.0;
        }
        s.cover[((y - s.y) as u32 * s.w + (x - s.x) as u32) as usize]
    }

    #[test]
    fn the_light_comes_out_as_an_led_s_does() {
        let (tree, device, b) = board();
        let px = |u: f32, v: f32| ((b.art_offset.0 + b.art_scale * u) as i32, (b.art_offset.1 + b.art_scale * v) as i32);
        // W (position of the W key): its cap's foot, from the art
        let c = device["controls"].as_array().unwrap().iter().find(|c| c["label"] == "W").unwrap();
        let key = b.keys.iter().find(|k| k.pos as u64 == c["pos"].as_u64().unwrap()).unwrap();
        let Some(usvg::Node::Group(g)) = tree.node_by_id(&format!("control_{}", c["control"])) else { panic!() };
        let Some(usvg::Node::Group(layer)) = tree.node_by_id("illumination_1") else { panic!() };
        let art = layer.abs_transform();
        let b0 = g.children()[1].abs_bounding_box();
        let f = usvg::Rect::from_ltrb((b0.left() - art.tx) / art.sx, (b0.top() - art.ty) / art.sy, (b0.right() - art.tx) / art.sx, (b0.bottom() - art.ty) / art.sy).unwrap();
        let (cx, cy) = ((f.left() + f.right()) / 2.0, (f.top() + f.bottom()) / 2.0);
        let deck = key.deck.as_ref().unwrap();
        // the deck just outside the skirt: brightest on the LED's (north) side
        let (n, s_, e) = (px(cx, f.top() - 0.5), px(cx, f.bottom() + 0.5), px(f.right() + 0.5, cy));
        let (vn, vs, ve) = (at(deck, n.0, n.1), at(deck, s_.0, s_.1), at(deck, e.0, e.1));
        eprintln!("deck by the skirt: north {vn:.3}, east {ve:.3}, south {vs:.3}");
        assert!(vn > ve && ve > vs * 0.9 && vn > 2.0 * vs, "the LED's side of the skirt lets out the most light");
        // and falling away from it
        let far = px(cx, f.top() - 1.8);
        assert!(at(deck, far.0, far.1) < vn * 0.6, "the glow fades within a couple of millimetres");
        // nothing outside the wells: the chassis above the function row
        let chassis = px(cx, 50.0);
        assert_eq!(at(deck, chassis.0, chassis.1), 0.0, "the deck glow stays in the wells");
        // the legend: its top (nearest the LED) lit more than its bottom
        let l = key.legend.as_ref().unwrap();
        let rows: Vec<f32> = (0..l.h).map(|r| (0..l.w).map(|c| key.legend_lit[(r * l.w + c) as usize] * l.cover[(r * l.w + c) as usize]).sum::<f32>()).collect();
        let lit_rows: Vec<(u32, f32)> = (0..l.h).filter(|r| rows[*r as usize] > 0.0).map(|r| (r, (0..l.w).filter(|c| l.cover[(r * l.w + c) as usize] > 0.5).map(|c| key.legend_lit[(r * l.w + c) as usize]).fold(0.0f32, f32::max))).filter(|(_, v)| *v > 0.0).collect();
        let (top_row, bottom_row) = (lit_rows.first().unwrap().1, lit_rows.last().unwrap().1);
        eprintln!("legend W lit: top {top_row:.3}, bottom {bottom_row:.3}");
        assert!(top_row >= bottom_row, "the legend is brightest nearest the LED");

        // Composed, the shadow does not darken the LED's light: the glow just
        // below the skirt, in the cap's shadow, still shows.
        let lights: Vec<Light> = b.keys.iter().map(|k| Light { pos: k.pos, rgb: [255, 255, 255], strength: 1.0, dim: false }).collect();
        let lit = b.compose(&lights, 254.0, 1000.0 / 254.0);
        let dark = b.compose(&[], 254.0, 1000.0 / 254.0);
        let i = ((s_.1 as u32 * b.w + s_.0 as u32) * 4) as usize;
        eprintln!("below the W's skirt: lit {:.4}, unlit {:.4}", lit[i], dark[i]);
        assert!(lit[i] > dark[i] + 0.02, "the glow shows in the cap's shadow");
        // and the brightest light on the board is a legend's, at the top
        let brightest = lit.chunks_exact(4).map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).fold(0f32, f32::max);
        assert!((brightest - 600.0 / 254.0).abs() < 0.05, "the brightest light is the top: {brightest}");
    }

    #[test]
    fn each_part_s_brightness_is_its_own() {
        let (_, device, mut b) = board();
        let lights: Vec<Light> = b.keys.iter().map(|k| Light { pos: k.pos, rgb: [255, 255, 255], strength: 1.0, dim: false }).collect();
        let (legend, deck, _) = b.peaks(&lights, 254.0, 1000.0 / 254.0);
        eprintln!("legends {legend:.0} nits, deck {deck:.0}");
        assert!((legend - 600.0).abs() < 1.0 && (deck - 90.0).abs() < 1.0);
        // the legends turned down leave the deck as it was
        let mut light = device["light"].clone();
        light["legend"]["nits"] = serde_json::json!(200);
        b.set_light(&light);
        let (legend, deck, _) = b.peaks(&lights, 254.0, 1000.0 / 254.0);
        eprintln!("legends turned down: legends {legend:.0} nits, deck {deck:.0}");
        assert!((legend - 200.0).abs() < 1.0 && (deck - 90.0).abs() < 1.0, "the deck keeps its own brightness");
        // a red the screen cannot show cleanly at 200 nits is still sent at 200;
        // its clean limit is only told
        let red: Vec<Light> = b.keys.iter().map(|k| Light { pos: k.pos, rgb: [255, 0, 0], strength: 1.0, dim: false }).collect();
        let (legend, deck, clean) = b.peaks(&red, 254.0, 600.0 / 254.0);
        eprintln!("red at a 600-nit peak: legends {legend:.0} nits (clean up to {clean:.0}), deck {deck:.0}");
        assert!((legend - 200.0).abs() < 1.0 && clean < 200.0 && (deck - 90.0).abs() < 1.0);
        let lit = b.compose(&red, 254.0, 600.0 / 254.0);
        let hottest = lit.chunks_exact(4).map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).fold(0f32, f32::max) * 254.0;
        eprintln!("red legends sent at {hottest:.0} nits");
        assert!((hottest - 200.0).abs() < 2.0, "the legends go as bright as asked");
        // and a key's brightness dims every part of it together
        let half: Vec<Light> = lights.iter().map(|l| Light { strength: 0.5, ..*l }).collect();
        let (legend, deck, _) = b.peaks(&half, 254.0, 1000.0 / 254.0);
        assert!((legend - 100.0).abs() < 1.0 && (deck - 45.0).abs() < 1.0, "half lit: {legend} {deck}");
    }

    #[test]
    fn the_table_matches_pq() {
        let enc = Encoder::new(254.0);
        for nits in [0.0f32, 0.05, 0.5, 2.7, 10.0, 100.0, 254.0, 1000.0] {
            let exact = pq(nits as f64) as f32;
            assert!((enc.signal(nits) - exact).abs() < 0.0005, "{nits} nits: {} vs {exact}", enc.signal(nits));
        }
    }
}
