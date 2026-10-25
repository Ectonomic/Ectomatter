//! How tall a menu is, as the window's Menu (ui/system/menu.slint) lays its rows out,
//! from the same tokens: Slint lays a list's rows out only as it draws them, so the
//! window works a menu's height out here, once, for every menu.

use crate::tokens::{font, line, part, size, space};
use crate::MenuRow;

/// A row's height by its kind: a heading, a line between groups, a note on as many
/// lines as its words take, or a row to choose (a control's height).
pub fn row(r: &MenuRow) -> f32 {
    match r.kind {
        1 => size::MENU_HEAD,
        2 => size::MENU_RULE,
        3 if r.lines > 1 => 2.0 * space::INLINE + r.lines as f32 * line::TEXT,
        _ => size::CONTROL,
    }
}

/// The menu's height: Back and its search box where it has them, then its rows (at most
/// `most` of them, as the menu scrolls the rest), each Part.menu-pad in from its edges.
pub fn height(rows: &[MenuRow], search: bool, back: bool, most: f32) -> f32 {
    let list: f32 = rows.iter().map(row).sum();
    let head = if back { size::CONTROL } else { 0.0 } + if search { size::CONTROL + part::MENU_PAD } else { 0.0 };
    part::MENU_PAD + head + list.min(most) + part::MENU_PAD
}

/// A menu `w` wide's notes set to wrap, each on as many lines as its words take in the
/// room the menu gives them (its padding and the words' own off, measured a little
/// narrower, as the window may break a line sooner).
pub fn fit_notes(rows: &mut [MenuRow], w: f32) {
    let room = w - 2.0 * part::MENU_PAD - 2.0 * part::MENU_IN - part::MENU_PAD;
    for r in rows.iter_mut().filter(|r| r.kind == 3) {
        r.lines = crate::text::wrap(font::REGULAR, font::CONTROL, &r.label, room).len() as i32;
    }
}
