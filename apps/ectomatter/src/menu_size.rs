//! How tall a menu is, as the window's Menu (ui/system/menu.slint) lays its rows out,
//! from the same tokens: Slint lays a list's rows out only as it draws them, so the
//! window works a menu's height out here, once, for every menu.

use crate::tokens::{font, line, part, size, space};
use crate::MenuRow;

/// the room a menu keeps clear over itself (the window's title bar, and a little more),
/// and how much room under what opened it is a fair amount, enough to hang there anyway
/// (the menu then scrolls)
const KEEP_ABOVE: f32 = 70.0;
const FAIR: f32 = 260.0;

/// Where a menu (or the picker) hangs from what opened it, `h` tall in a window `win`
/// tall: its top, and the most it may be before it scrolls. Under what opened it when it
/// fits there, or when there's a fair amount of room there anyway; over it only when
/// under it is cramped; Part.menu-pad off it either way, and Space.inline clear of the
/// window's foot. `at` is what opened it: its top and its height.
pub fn hang(win: f32, at: (f32, f32), h: f32) -> (f32, f32) {
    let (ay, ah) = at;
    let (below, above) = (win - ay - ah - part::MENU_PAD - space::INLINE, ay - KEEP_ABOVE);
    let down = h <= below || below >= h.min(FAIR) || below >= above;
    let room = if down { below } else { above };
    let top = if down { ay + ah + part::MENU_PAD } else { ay - part::MENU_PAD - h.min(room) };
    (top, if h > room { room } else { win * 0.7 })
}

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

/// How wide a menu is: as wide as its rows need, each measured as the window's Menu lays
/// it out from the same tokens, from Size.menu to Size.menu-most, and never narrower than
/// `under`, the field it drops from (none for a menu a button opens). `marks`: whether it
/// keeps a column for ticks. A note on one line where the menu can be as wide as it
/// needs (measured as fit_notes measures it), wrapped where it can't.
pub fn width(rows: &[MenuRow], marks: bool, under: f32) -> f32 {
    let words = |size: f32, s: &str| crate::text::width(font::REGULAR, size, s);
    let lead = if marks { part::MENU_IN + part::GLYPH + space::INLINE } else { space::ROW };
    let need = rows
        .iter()
        .map(|r| match r.kind {
            1 => part::MENU_IN + words(font::CAPTION, &r.label) + part::MENU_IN,
            3 => part::MENU_IN + words(font::CONTROL, &r.label) + part::MENU_IN + part::MENU_PAD,
            // its chip, its tag, its name, its dot, what it says on its right, its pencil
            0 | 4 | 5 => {
                lead + if r.chipped { part::GLYPH + part::CHECK_GAP } else { 0.0 }
                    + if r.tag.is_empty() { 0.0 } else { crate::form::pill_width(1, &r.tag) + space::INLINE }
                    + words(font::TEXT, &r.label)
                    + if r.dot { space::INLINE + size::DOT } else { 0.0 }
                    + if r.sub.is_empty() { 0.0 } else { space::INLINE + words(font::CAPTION, &r.sub) }
                    + if r.edit { space::INLINE + size::QUIET + space::TIGHT } else { part::MENU_IN }
            }
            6 => part::MENU_IN + part::GLYPH + space::INLINE + words(font::TEXT, &r.label) + space::INLINE + part::MENU_NUMBER + space::INLINE + words(font::TEXT, if r.sub.is_empty() { "ms" } else { &r.sub }) + part::MENU_IN,
            7 => lead + words(font::CONTROL, &r.label) + part::MENU_IN,
            _ => 0.0,
        })
        .fold(0.0, f32::max)
        + 2.0 * part::MENU_PAD;
    // (wide enough that what a row says on its right fits its share of the menu)
    let aside = rows.iter().filter(|r| !r.sub.is_empty()).map(|r| words(font::CAPTION, &r.sub) / part::MENU_ASIDE).fold(0.0, f32::max);
    let need = need.max(aside);
    need.ceil().clamp(size::MENU, size::MENU_MOST).max(under)
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
