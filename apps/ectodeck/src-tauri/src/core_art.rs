//! The Z10 core's art, from its own folder (cores/evga-z10): the drawing and
//! which of its pieces belong to each control and screen, as the core
//! protocol's draft 4 has a device described. Until the Z10 core runs as its
//! own program, Ectodeck reads that folder when it is built.

pub const SVG: &str = include_str!("../../../../cores/evga-z10/art/z10.svg");
pub const DEVICE: &str = include_str!("../../../../cores/evga-z10/device.json");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn every_piece_keeps_its_id() {
        let tree = resvg::usvg::Tree::from_str(SVG, &resvg::usvg::Options::default()).unwrap();
        for id in ["body", "legend-body", "control", "legend", "body_1", "legend_1", "control_1", "control_117",
                   "illumination_1", "illumination_201", "illumination_401", "legend_101", "overlay_1", "screen_1"] {
            let node = tree.node_by_id(id);
            let bbox = node.and_then(|n| n.abs_layer_bounding_box());
            eprintln!("{id}: {}", bbox.map_or("MISSING".to_string(), |b| format!("{:.1} {:.1} {:.1} {:.1}", b.x(), b.y(), b.width(), b.height())));
            assert!(node.is_some(), "{id} lost");
        }
    }

    #[test]
    fn the_device_names_every_key() {
        let d: serde_json::Value = serde_json::from_str(DEVICE).unwrap();
        assert!(d["controls"].as_array().unwrap().len() >= 116);
    }
}
