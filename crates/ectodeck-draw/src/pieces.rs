//! Pieces of the drawing on their own, at a board's size: what the page draws
//! over the keyboard rather than in it (the slider knobs, which move, and the
//! display's glass, which covers the live display).

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{self, Tree};

/// Pieces drawn at a board `w` x `h` pixels (the drawing fitted into it, as
/// the keyboard is), cropped to where they are: their box on the board, and
/// their pixels, premultiplied RGBA.
pub struct Piece {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// How the drawing sits in a board: its scale and where its corner goes.
pub fn fit(tree: &Tree, w: u32, h: u32) -> (f32, f32, f32) {
    let size = tree.size();
    let k = (w as f32 / size.width()).min(h as f32 / size.height());
    (k, (w as f32 - size.width() * k) / 2.0, (h as f32 - size.height() * k) / 2.0)
}

pub fn draw(tree: &Tree, ids: &[&str], w: u32, h: u32) -> Option<Piece> {
    let (k, tx, ty) = fit(tree, w, h);
    let nodes: Vec<&usvg::Node> = ids.iter().filter_map(|id| tree.node_by_id(id)).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for n in &nodes {
        let bb = n.abs_layer_bounding_box()?;
        x0 = x0.min(bb.x());
        y0 = y0.min(bb.y());
        x1 = x1.max(bb.x() + bb.width());
        y1 = y1.max(bb.y() + bb.height());
    }
    if nodes.is_empty() {
        return None;
    }
    let (x, y) = ((tx + k * x0).floor() as i32, (ty + k * y0).floor() as i32);
    let (pw, ph) = (((tx + k * x1).ceil() as i32 - x).max(1) as u32, ((ty + k * y1).ceil() as i32 - y).max(1) as u32);
    let mut pix = Pixmap::new(pw, ph)?;
    for n in nodes {
        // resvg draws a lone piece with its own transform only, shifted so its
        // box starts at 0,0: give it its groups' transforms and cancel the shift
        let usvg::Node::Group(g) = n else { continue };
        let groups = g.abs_transform().pre_concat(g.transform().invert()?);
        let bb = n.abs_layer_bounding_box()?;
        let t = Transform::from_row(k, 0.0, 0.0, k, tx - x as f32, ty - y as f32).pre_concat(groups).pre_translate(bb.x(), bb.y());
        resvg::render_node(n, t, &mut pix.as_mut());
    }
    Some(Piece { x, y, w: pw, h: ph, rgba: pix.data().to_vec() })
}

/// A piece's box in the drawing's own units.
pub fn bounds(tree: &Tree, id: &str) -> Option<[f32; 4]> {
    let bb = tree.node_by_id(id)?.abs_layer_bounding_box()?;
    Some([bb.x(), bb.y(), bb.width(), bb.height()])
}
