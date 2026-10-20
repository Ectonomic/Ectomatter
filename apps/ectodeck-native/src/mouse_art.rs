//! The mouse's picture for its page: SteelSeries' core's art (Grant's
//! drawing, cores/steelseries/art/sensei-310.svg), drawn as the page's image,
//! with its lights in the colors they are set to, the chosen button picked
//! out, and the shadow every device casts on its stage.

use resvg::tiny_skia::{Pixmap, Transform};
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

/// How a light shows: its color, or off.
pub type Lit = Option<[u8; 3]>;

/// The picture: the mouse `w` pixels wide (its height as the drawing has
/// it), its logo and wheel lit as `logo` and `wheel`, button `chosen` (1 to 8)
/// picked out, over its shadow; and how far the shadow reaches past the
/// mouse at each side, in pixels.
pub fn picture(art: &str, w: u32, logo: Lit, wheel: Lit, chosen: Option<u8>, shadow: &[ectodeck_draw::hdr::ShadowLayer], opacity: f32) -> Result<(Pixmap, u32), String> {
    let svg = dressed(art, logo, wheel, chosen);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let k = w as f32 / VIEW.2;
    let h = (VIEW.3 * k).round() as u32;
    let mut mouse = Pixmap::new(w, h).ok_or("the mouse too small to draw")?;
    // (the drawing's units to the tree's: its size over its viewBox's)
    let units = tree.size().width() / view_width(&svg).unwrap_or(tree.size().width());
    let t = k / units;
    resvg::render(&tree, Transform::from_row(t, 0.0, 0.0, t, -VIEW.0 * k, -VIEW.1 * k), &mut mouse.as_mut());
    // the shadow from the mouse's own outline, under it
    let cover: Vec<f32> = mouse.data().chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect();
    let s = ectodeck_draw::hdr::Shadow::of(&cover, w as usize, h as usize, shadow, opacity);
    let mut out = Pixmap::new(s.w, s.h).ok_or("the mouse too small to draw")?;
    for (px, d) in out.data_mut().chunks_exact_mut(4).zip(&s.dark) {
        px[3] = (d * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out.draw_pixmap(s.margin as i32, s.margin as i32, mouse.as_ref(), &resvg::tiny_skia::PixmapPaint::default(), Transform::identity(), None);
    Ok((out, s.margin))
}

/// The drawing as the page shows it: without its reference photo and guides,
/// its lights' white in their colors (or gone, for a light off), and the
/// chosen button outlined and a shade lighter.
fn dressed(art: &str, logo: Lit, wheel: Lit, chosen: Option<u8>) -> String {
    let mut s = art.to_string();
    for layer in ["reference", "guides"] {
        s = without_group(&s, layer);
    }
    for (ids, lit) in [(&["illumination_9", "illumination_209"][..], logo), (&["illumination_3", "illumination_203"][..], wheel)] {
        for id in ids {
            s = recolor_group(&s, id, lit);
        }
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

    /// The mouse drawn: its lights in their colors, the chosen button picked
    /// out, its photo and guides left out, a shadow round it.
    #[test]
    fn the_mouse_is_drawn_lit_and_picked_out() {
        let layers = [ectodeck_draw::hdr::ShadowLayer { down: 8.0, blur: 22.0, strength: 0.6 }];
        let (red, m) = picture(&art(), 240, Some([255, 0, 0]), Some([255, 0, 0]), None, &layers, 0.5).unwrap();
        let (off, _) = picture(&art(), 240, None, None, Some(7), &layers, 0.5).unwrap();
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
}
