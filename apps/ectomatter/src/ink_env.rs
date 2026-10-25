//! ECTOMATTER_INK="base=#1c1c20;box=#242429": the page's colors, by the names
//! design/ectomatter.tokens.json gives its Ink tokens, set otherwise for a color mock drawn by
//! Ectomatter itself. Unset, nothing changes. The grounds the keyboard and the
//! mouse are drawn on follow the tokens they match (stage, and base on Home),
//! so a changed page still meets its pictures without a seam.

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
        if !crate::tokens::ink::set(&ink, name, Color::from_rgb_u8(c[0], c[1], c[2])) {
            eprintln!("ectomatter: ECTOMATTER_INK names no token {name}");
        }
    }
}
