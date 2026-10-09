//! The Screen tab's page editor: the boxes under the screen while a page is
//! edited (the piece chosen, where it sits, the page), what each of their
//! controls does to the page, and a new piece's first place.

use crate::lcd_page::{Kind, Page, Piece, Readings, Source, COLS, ROWS};
use crate::lighting::{cell, id, kind, style, text_cell, Ctl, Stack};
use crate::Cell;

/// Every box is this wide, as the When-closed box is.
pub const BOX_W: f32 = 320.0;

/// What is being edited: which page, the piece chosen, and whether the
/// screen shows its grid of cells.
#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    pub page: usize,
    pub chosen: Option<usize>,
    pub grid: bool,
}

pub struct Boxes {
    pub piece_title: String,
    pub piece: Vec<Cell>,
    pub piece_h: f32,
    pub place: Vec<Cell>,
    pub place_h: f32,
    pub page_title: String,
    pub page: Vec<Cell>,
    pub page_h: f32,
}

fn dot(c: [u8; 3]) -> slint::Color {
    slint::Color::from_rgb_u8(c[0], c[1], c[2])
}

/// A number with a button either side to make it less or more.
fn stepper(s: &mut Stack, c: Ctl, x: f32, value: &str) {
    s.cells.push(Cell { text: "−".into(), id: id(c), index: 0, ..cell(kind::GHOST, x, s.y, 34.0, 30.0) });
    s.cells.push(Cell { text: value.into(), ..cell(kind::PILL, x + 42.0, s.y + 5.0, 40.0, 20.0) });
    s.cells.push(Cell { text: "+".into(), id: id(c), index: 1, ..cell(kind::GHOST, x + 90.0, s.y, 34.0, 30.0) });
}

/// What a piece looks like now, for its box.
fn looks(p: &Piece, r: &Readings) -> String {
    let page = Page { id: String::new(), name: String::new(), light: None, pieces: vec![Piece { row: 0, col: 0, gap: 0, ..p.clone() }] };
    let row = crate::lcd_page::render(&page, r).remove(0);
    if row.ends_with(" unavailable") || row.is_empty() { "Unavailable".into() } else { row.trim().to_string() }
}

/// The boxes for the page, with the piece chosen. `avail` is what this
/// computer can read; `profile_light` the profile's screen light.
pub fn boxes(page: &Page, chosen: Option<usize>, r: &Readings, avail: &[Source], profile_light: [u8; 3], menu_open: bool) -> Boxes {
    let piece = chosen.and_then(|i| page.pieces.get(i));
    // the piece
    let mut s = Stack::new(BOX_W);
    let piece_title = match piece {
        None => {
            s.note(&format!("Select a piece of {} on the screen to change the piece, or add a piece with the buttons above.", page.name));
            "Nothing selected".to_string()
        }
        Some(p) => {
            s.link_right(Ctl::ScrRemove, "Remove", 13.0, "");
            s.y += 4.0;
            match &p.kind {
                Kind::Text(t) => {
                    s.head("Says", None);
                    s.cells.push(Cell { text: t.as_str().into(), style: 7, id: id(Ctl::ScrText), ..cell(kind::FIELD, 16.0, s.y, s.inner(), 34.0) });
                    s.y += 34.0 + 10.0;
                    s.note("Letters, numbers and most symbols show as typed. Other characters show as a question mark.");
                }
                Kind::Data(src, f) => {
                    s.head("Shows", None);
                    s.dropdown(Ctl::ScrSource, src.name(), menu_open);
                    s.gap(14.0);
                    if avail.contains(src) {
                        s.head("From", None);
                        s.y -= 6.0;
                        s.note(src.from());
                    } else {
                        s.note(&format!("Ectodeck can’t read the {} on this computer, so the row shows “unavailable”.", src.name().to_lowercase()));
                    }
                    s.gap(14.0);
                    s.head("Looks like", Some(&looks(p, r)));
                    if src.is_temperature() {
                        s.gap(4.0);
                        s.head("Units", None);
                        s.switch(Ctl::ScrUnits, &[("°C", ""), ("°F", "")], if *f { 1 } else { 0 });
                    }
                }
                Kind::Bar(src) => {
                    s.head("Shows", None);
                    s.dropdown(Ctl::ScrSource, src.name(), menu_open);
                    s.gap(14.0);
                    s.head("Looks like", Some(&looks(p, r)));
                    s.note("The bar fills as the reading rises. A wider bar shows smaller changes.");
                }
                Kind::Clock(twelve) => {
                    s.head("Hours", None);
                    s.switch(Ctl::ScrHours, &[("24-hour", ""), ("12-hour", "")], if *twelve { 1 } else { 0 });
                    s.gap(14.0);
                    s.head("Looks like", Some(&looks(p, r)));
                }
            }
            p.name()
        }
    };
    let piece_h = s.height();
    let piece_cells = s.cells;

    // where it sits
    let mut s = Stack::new(BOX_W);
    match piece {
        None => s.note("Where the selected piece sits on the screen’s 20 columns and 4 rows."),
        Some(p) => {
            s.head("Row", None);
            s.switch(Ctl::ScrRow, &[("1", ""), ("2", ""), ("3", ""), ("4", "")], p.row as usize);
            s.gap(16.0);
            let half = s.inner() / 2.0;
            s.cells.push(text_cell(style::PROP, 16.0, s.y, half, 20.0, "Column"));
            s.cells.push(text_cell(style::PROP, 16.0 + half + 8.0, s.y, half, 20.0, "Width"));
            s.y += 28.0;
            stepper(&mut s, Ctl::ScrCol, 16.0, &(p.col + 1).to_string());
            stepper(&mut s, Ctl::ScrWidth, 16.0 + half + 8.0, &p.width.to_string());
            s.y += 30.0 + 16.0;
            s.head("Align", None);
            s.switch(Ctl::ScrAlign, &[("Left", ""), ("Right", "")], if p.right { 1 } else { 0 });
            s.gap(12.0);
            s.note("Or drag the piece on the screen.");
        }
    }
    let place_h = s.height();
    let place_cells = s.cells;

    // the page
    let mut s = Stack::new(BOX_W);
    s.head("Screen light", None);
    let own = page.light;
    let mut x = 16.0;
    s.cells.push(Cell { color: dot(profile_light), on: own.is_none(), id: id(Ctl::ScrLight), index: 0, ..cell(kind::COLOR_DOT, x - 2.0, s.y, 28.0, 28.0) });
    x += 46.0;
    for (i, l) in crate::display::LIGHTS.iter().enumerate() {
        s.cells.push(Cell { color: dot(*l), on: own == Some(*l), id: id(Ctl::ScrLight), index: i as i32 + 1, ..cell(kind::COLOR_DOT, x - 2.0 + i as f32 * 32.0, s.y, 28.0, 28.0) });
    }
    s.y += 28.0 + 12.0;
    s.note(&match own {
        None => "The first color is the profile’s screen light, which this page uses now.".to_string(),
        Some(_) => format!("{} uses this light. Select the first color to use the profile’s screen light again.", page.name),
    });
    let page_h = s.height();
    Boxes { piece_title, piece: piece_cells, piece_h, place: place_cells, place_h, page_title: page.name.clone(), page: s.cells, page_h }
}

/// The menu of readings for a piece: each group's heading, then its
/// readings, each with what it reads now. The rows, and the reading each
/// row stands for.
pub fn source_menu(current: Source, bars: bool, avail: &[Source], r: &Readings) -> Vec<(i32, String, String, bool, Option<Source>)> {
    let mut rows = Vec::new();
    let mut group = "";
    // each group once, in the menu's order
    let order = ["Processor", "Graphics", "Memory", "More"];
    let mut listed: Vec<Source> = avail.iter().copied().filter(|s| !bars || s.barred()).collect();
    listed.sort_by_key(|s| order.iter().position(|g| *g == s.group()));
    for s in listed {
        if s.group() != group {
            if !group.is_empty() {
                rows.push((2, String::new(), String::new(), false, None));
            }
            group = s.group();
            rows.push((1, group.to_string(), String::new(), false, None));
        }
        let p = Piece { kind: if bars { Kind::Bar(s) } else { Kind::Data(s, false) }, row: 0, col: 0, width: if bars { 10 } else { 1 }, right: false, gap: 0 };
        rows.push((0, s.name().to_string(), looks(&p, r), s == current, Some(s)));
    }
    rows
}

/// Use a control of the boxes on the page.
pub fn act(page: &mut Page, ed: &mut Editor, c: Ctl, index: usize, words: Option<&str>) {
    if c == Ctl::ScrLight {
        page.light = if index == 0 { None } else { crate::display::LIGHTS.get(index - 1).copied() };
        return;
    }
    let Some(i) = ed.chosen.filter(|i| *i < page.pieces.len()) else { return };
    if c == Ctl::ScrRemove {
        page.pieces.remove(i);
        ed.chosen = None;
        return;
    }
    let p = &mut page.pieces[i];
    match c {
        Ctl::ScrText => {
            if let Some(t) = words {
                p.kind = Kind::Text(t.chars().take(COLS as usize).collect());
            }
        }
        Ctl::ScrUnits => {
            if let Kind::Data(s, _) = p.kind {
                p.kind = Kind::Data(s, index == 1);
            }
        }
        Ctl::ScrHours => p.kind = Kind::Clock(index == 1),
        Ctl::ScrRow => p.row = (index as u8).min(ROWS - 1),
        Ctl::ScrCol => p.col = if index == 0 { p.col.saturating_sub(1) } else { (p.col + 1).min(COLS - 1) },
        Ctl::ScrWidth => p.width = if index == 0 { p.width.saturating_sub(1).max(1) } else { (p.width + 1).min(COLS) },
        Ctl::ScrAlign => p.right = index == 1,
        _ => {}
    }
}

/// The reading chosen from the menu, for the piece chosen.
pub fn set_source(page: &mut Page, ed: &Editor, s: Source) {
    let Some(p) = ed.chosen.and_then(|i| page.pieces.get_mut(i)) else { return };
    p.kind = match p.kind {
        Kind::Data(_, f) => Kind::Data(s, f && s.is_temperature()),
        Kind::Bar(_) => Kind::Bar(s),
        ref k => k.clone(),
    };
}

/// A piece moved on the screen to a cell.
pub fn move_to(page: &mut Page, i: usize, row: i32, col: i32) {
    if let Some(p) = page.pieces.get_mut(i) {
        p.row = row.clamp(0, ROWS as i32 - 1) as u8;
        p.col = col.clamp(0, COLS as i32 - p.width.min(COLS) as i32) as u8;
    }
}

/// A new piece, `what` 0 words, 1 a reading, 2 a bar, 3 a clock, put in the
/// first run of empty cells it fits, and chosen. `first` is a reading this
/// computer has.
pub fn add(page: &mut Page, ed: &mut Editor, what: usize, first: Source) {
    let (kind, width, right) = match what {
        0 => (Kind::Text("TEXT".into()), 4, false),
        1 => (Kind::Data(first, false), 7, true),
        2 => (Kind::Bar(if first.barred() { first } else { Source::CpuLoad }), 10, false),
        _ => (Kind::Clock(false), 5, false),
    };
    let taken = |row: u8, col: u8| page.pieces.iter().any(|p| p.row == row && col >= p.col && col < p.col + p.width);
    let spot = (0..ROWS).find_map(|row| (0..=COLS - width).find(|&c| (c..c + width).all(|k| !taken(row, k))).map(|c| (row, c)));
    let (row, col) = spot.unwrap_or((0, 0));
    page.pieces.push(Piece { kind, row, col, width, right, gap: 1 });
    ed.chosen = Some(page.pieces.len() - 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_piece_goes_where_there_is_room_and_is_chosen() {
        let mut p = Page::stats();
        let mut ed = Editor { page: 0, chosen: None, grid: true };
        // Stats has no run of five empty cells: the clock starts at the top left
        add(&mut p, &mut ed, 3, Source::CpuTemp);
        assert_eq!(ed.chosen, Some(12));
        let mut empty = Page { pieces: Vec::new(), ..Page::stats() };
        add(&mut empty, &mut ed, 1, Source::GpuTemp);
        assert_eq!((empty.pieces[0].row, empty.pieces[0].col), (0, 0));
        add(&mut empty, &mut ed, 0, Source::GpuTemp);
        assert_eq!((empty.pieces[1].row, empty.pieces[1].col), (0, 7));
    }

    #[test]
    fn the_controls_change_the_chosen_piece() {
        let mut p = Page::stats();
        let mut ed = Editor { page: 0, chosen: Some(1), grid: true };
        act(&mut p, &mut ed, Ctl::ScrUnits, 1, None);
        assert_eq!(p.pieces[1].kind, Kind::Data(Source::CpuTemp, true));
        act(&mut p, &mut ed, Ctl::ScrCol, 1, None);
        act(&mut p, &mut ed, Ctl::ScrWidth, 0, None);
        act(&mut p, &mut ed, Ctl::ScrRow, 3, None);
        act(&mut p, &mut ed, Ctl::ScrAlign, 0, None);
        let q = &p.pieces[1];
        assert_eq!((q.row, q.col, q.width, q.right), (3, 5, 2, false));
        set_source(&mut p, &ed, Source::GpuLoad);
        assert_eq!(p.pieces[1].kind, Kind::Data(Source::GpuLoad, false), "°F goes with a reading that isn't a temperature");
        move_to(&mut p, 1, 9, 25);
        assert_eq!((p.pieces[1].row, p.pieces[1].col), (3, 18));
        act(&mut p, &mut ed, Ctl::ScrLight, 2, None);
        assert_eq!(p.light, Some(crate::display::LIGHTS[1]));
        act(&mut p, &mut ed, Ctl::ScrRemove, 0, None);
        assert_eq!((p.pieces.len(), ed.chosen), (11, None));
    }
}
