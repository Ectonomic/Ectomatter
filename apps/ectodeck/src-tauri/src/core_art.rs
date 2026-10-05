//! The Z10 core's art, from its own folder (cores/evga-z10): the drawing and
//! the device description. The description (JSON with comments, as the core
//! protocol's draft 6 has it) ties each control to its pieces of the drawing
//! by number; this app's drawing and page take the ties named, as draft 4 had
//! them, which device() adds. Until the Z10 core runs as its own program,
//! Ectodeck reads that folder when it is built.

use serde_json::{json, Value};

pub const SVG: &str = include_str!("../../../../cores/evga-z10/art/z10.svg");
/// the device description as the core's file has it
const DEVICE: &str = include_str!("../../../../cores/evga-z10/device.json");

/// The device description, each control's pieces named as draft 4 had them:
/// its "control" (control_<art>), its "legend" (legend_<100 + art>, where the
/// drawing has one) and, for a key with an LED, its "illumination"
/// (illumination_<art>, <200 + art> and <400 + art>), unless it names them
/// itself; each screen's "screen" and "overlay" (screen_<art>, overlay_<art>).
pub fn device() -> Value {
    let mut d: Value = serde_json::from_str(&uncommented(DEVICE)).expect("the Z10 core's device description");
    for c in d["controls"].as_array_mut().into_iter().flatten() {
        let Some(n) = c["art"].as_u64() else { continue };
        c["control"] = json!(n);
        if c.get("legend").is_none() && SVG.contains(&format!("id=\"legend_{}\"", 100 + n)) {
            c["legend"] = json!(100 + n);
        }
        if c.get("illumination").is_none() && c.get("slider").is_none() && c["lit"].as_bool() != Some(false) {
            c["illumination"] = json!([n, 200 + n, 400 + n]);
        }
    }
    for s in d["screens"].as_array_mut().into_iter().flatten() {
        let n = s["art"].clone();
        (s["screen"], s["overlay"]) = (n.clone(), n);
    }
    d
}

/// JSON with its comments left out: `//` to the end of a line, and `/* */`,
/// outside its strings.
fn uncommented(text: &str) -> String {
    let (mut out, mut chars) = (String::with_capacity(text.len()), text.chars().peekable());
    let mut string = false;
    while let Some(c) = chars.next() {
        match (string, c, chars.peek()) {
            (true, '\\', _) => {
                out.push(c);
                out.extend(chars.next());
                continue;
            }
            (true, '"', _) => string = false,
            (false, '"', _) => string = true,
            (false, '/', Some('/')) => {
                while chars.peek().is_some_and(|c| *c != '\n') {
                    chars.next();
                }
                continue;
            }
            (false, '/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for c in chars.by_ref() {
                    if last == '*' && c == '/' {
                        break;
                    }
                    last = c;
                }
                continue;
            }
            _ => {}
        }
        out.push(c);
    }
    out
}

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
        let d = device();
        assert!(d["controls"].as_array().unwrap().len() >= 116);
    }

    /// Every piece a control is tied to is in the drawing, and every key
    /// with an LED has its lights; the comments are read past, a string's
    /// slashes kept.
    #[test]
    #[cfg(target_os = "linux")]
    fn every_tie_is_a_piece_of_the_drawing() {
        let tree = resvg::usvg::Tree::from_str(SVG, &resvg::usvg::Options::default()).unwrap();
        let d = device();
        let has = |id: String| tree.node_by_id(&id).is_some();
        let mut lit = 0;
        for c in d["controls"].as_array().unwrap() {
            assert!(has(format!("control_{}", c["control"])), "{c}");
            assert!(c["legend"].as_u64().is_none_or(|n| has(format!("legend_{n}"))), "{c}");
            for n in c["illumination"].as_array().into_iter().flatten() {
                assert!(has(format!("illumination_{n}")), "{c}");
            }
            lit += c["illumination"].is_array() as usize;
        }
        assert_eq!(lit, d["controls"].as_array().unwrap().len() - 8, "all but G/PC, the media keys, Space and the two rollers");
        assert_eq!(uncommented("{ \"a\": \"//b\", // c\n \"d\": /* e */ 1 }"), "{ \"a\": \"//b\", \n \"d\":  1 }");
    }
}
