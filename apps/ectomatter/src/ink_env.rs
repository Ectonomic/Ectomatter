//! ECTOMATTER_INK="base=#1c1c20;box=#242429": the page's colors, by the names
//! ui/app.slint gives its Ink tokens, set otherwise for a color mock drawn by
//! Ectomatter itself. Unset, nothing changes. The grounds the keyboard and the
//! mouse are drawn on follow the tokens they match (well, base and stage), so
//! a changed page still meets its pictures without a seam.

use std::collections::HashMap;
use std::sync::OnceLock;

use slint::{Color, ComponentHandle};

fn table() -> &'static HashMap<String, [u8; 3]> {
    static T: OnceLock<HashMap<String, [u8; 3]>> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = HashMap::new();
        for pair in std::env::var("ECTOMATTER_INK").unwrap_or_default().split(';') {
            let Some((name, hex)) = pair.split_once('=') else { continue };
            let hex = hex.trim().trim_start_matches('#');
            let Ok(v) = u32::from_str_radix(hex, 16) else { continue };
            if hex.len() == 6 {
                t.insert(name.trim().to_string(), [(v >> 16) as u8, (v >> 8) as u8, v as u8]);
            }
        }
        t
    })
}

/// A token's color as ECTOMATTER_INK sets it, or as the app has it.
pub fn rgb(name: &str, otherwise: [u8; 3]) -> [u8; 3] {
    table().get(name).copied().unwrap_or(otherwise)
}

/// The tokens ECTOMATTER_INK names, set on the main window.
pub fn apply(ui: &crate::AppWindow) {
    let ink = ui.global::<crate::Ink>();
    for (name, c) in table() {
        let c = Color::from_rgb_u8(c[0], c[1], c[2]);
        match name.as_str() {
            "frame" => ink.set_frame(c),
            "title" => ink.set_title(c),
            "base" => ink.set_base(c),
            "box" => ink.set_box(c),
            "well" => ink.set_well(c),
            "stage" => ink.set_stage(c),
            "hover-well" => ink.set_hover_well(c),
            "hover-box" => ink.set_hover_box(c),
            "pick" => ink.set_pick(c),
            "pick-hover" => ink.set_pick_hover(c),
            "pop" => ink.set_pop(c),
            "line" => ink.set_line(c),
            "ink" => ink.set_ink(c),
            "bright" => ink.set_bright(c),
            "dim" => ink.set_dim(c),
            "soft" => ink.set_soft(c),
            "quiet" => ink.set_quiet(c),
            "warn" => ink.set_warn(c),
            "bad" => ink.set_bad(c),
            "good" => ink.set_good(c),
            "shade1" => ink.set_shade1(c),
            "shade2" => ink.set_shade2(c),
            "shade3" => ink.set_shade3(c),
            "shade4" => ink.set_shade4(c),
            "track" => ink.set_track(c),
            other => eprintln!("ectomatter: ECTOMATTER_INK names no token {other}"),
        }
    }
}
