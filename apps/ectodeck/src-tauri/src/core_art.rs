//! The Z10 core's art, from its own folder (cores/evga-z10): the drawing and
//! which of its pieces belong to each control and screen, as the core
//! protocol's draft 4 has a device described. Until the Z10 core runs as its
//! own program, Ectodeck reads that folder when it is built.

pub const SVG: &str = include_str!("../../../../cores/evga-z10/art/z10.svg");
pub const DEVICE: &str = include_str!("../../../../cores/evga-z10/device.json");

/// The drawing with only the named layers kept, the others left out. Layers
/// are the drawing's top-level groups marked as Inkscape layers.
pub fn layers(keep: &[&str]) -> String {
    const START: &str = "<g id=\"";
    const MARK: &str = "\" inkscape:groupmode=\"layer\"";
    let mut starts: Vec<(usize, &str)> = Vec::new();
    let mut at = 0;
    while let Some(i) = SVG[at..].find(START) {
        let s = at + i;
        let rest = &SVG[s + START.len()..];
        if let Some(q) = rest.find('"') {
            if rest[q..].starts_with(MARK) {
                starts.push((s, &rest[..q]));
            }
        }
        at = s + START.len();
    }
    let Some(&(first, _)) = starts.first() else { return SVG.to_string() };
    let end = SVG.rfind("</svg>").unwrap_or(SVG.len());
    let mut out = SVG[..first].to_string();
    for (n, &(s, id)) in starts.iter().enumerate() {
        let e = starts.get(n + 1).map_or(end, |&(next, _)| next);
        if keep.contains(&id) {
            out.push_str(&SVG[s..e]);
        }
    }
    out.push_str(&SVG[end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_named_layers() {
        let body = layers(&["body", "legend-body"]);
        assert!(body.contains("id=\"body_1\"") && body.contains("id=\"legend_1\""));
        assert!(!body.contains("id=\"control_1\"") && !body.contains("id=\"overlay_1\""));
        assert!(body.trim_end().ends_with("</svg>"));
        balanced(&body);
    }

    fn balanced(s: &str) {
        // well formed: every layer group closed
        assert_eq!(s.matches("<g").count(), s.matches("</g>").count());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn the_plate_draws() {
        // the HDR layer draws the plate with resvg from these layers; it must come out drawn, not empty
        let svg = layers(&["body", "legend-body"]);
        let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
        let mut pix = resvg::tiny_skia::Pixmap::new(1240, 497).unwrap();
        let k = 1240.0 / tree.size().width();
        resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pix.as_mut());
        let opaque = pix.data().chunks_exact(4).filter(|p| p[3] > 200).count();
        eprintln!("plate: {opaque} of {} pixels drawn, tree {:?}", 1240 * 497, tree.size());
        assert!(opaque > 1240 * 497 / 2, "the plate came out nearly empty: {opaque} pixels");
    }

    #[test]
    fn the_device_names_every_key() {
        let d: serde_json::Value = serde_json::from_str(DEVICE).unwrap();
        assert!(d["controls"].as_array().unwrap().len() >= 116);
    }
}
