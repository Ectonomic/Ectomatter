//! A form, laid out the same on every page that sets things in rows (Ectomatter's
//! Settings, a device's Settings tab, a game's page): a column of sections from
//! the page's margin, at most Size.form wide. Each section is a heading over its
//! box (Space.label between, Space.section after the box before it); in the box,
//! Space.pad in from its edges, rows, each a label in a column Size.label wide and
//! the row's controls Space.pad after it, a rule between rows with Space.row either
//! side. The window draws the box (a tile) and every piece where this puts it.

use crate::lighting::{cell, kind, style, text_cell};
use crate::tokens::{font, ink, line, part, size, space};
use crate::Cell;

/// the height a row's first line is given, a control's (a label sits level with it)
pub const LINE: f32 = size::CONTROL;
/// a line of words in a row (a fact, a value): the height a label sits level with
pub const TEXT_LINE: f32 = line::TEXT;

/// A column of sections being laid out, down to `y`.
pub struct Form {
    pub x: f32,
    pub w: f32,
    pub y: f32,
    pub cells: Vec<Cell>,
    sections: usize,
}

impl Form {
    /// A form at `x`, `y`, in `room` across, of which it takes Size.form at most.
    pub fn new(x: f32, y: f32, room: f32) -> Form {
        Form { x, w: room.min(size::FORM).max(0.0), y, cells: Vec::new(), sections: 0 }
    }

    /// A section's heading, with what sits at its right (`aside`, a note, or nothing),
    /// Space.section under the box before it.
    pub fn heading(&mut self, heading: &str, aside: &str) {
        if self.sections > 0 {
            self.y += space::SECTION;
        }
        self.sections += 1;
        self.y = self::heading(&mut self.cells, self.x, self.y, self.w, heading, "", aside);
    }

    /// A section: its heading and its box, whose rows `fill` lays out. The box's top.
    pub fn section(&mut self, heading: &str, aside: &str, fill: impl FnOnce(&mut Card)) -> f32 {
        self.heading(heading, aside);
        self.card(fill)
    }

    /// A box under the heading just laid, its rows laid out by `fill`. Its top.
    pub fn card(&mut self, fill: impl FnOnce(&mut Card)) -> f32 {
        let top = self.y;
        let mut c = Card::new(self.x, top, self.w);
        fill(&mut c);
        let h = c.y + space::PAD - top;
        self.cells.push(cell(kind::TILE, self.x, top, self.w, h));
        self.cells.append(&mut c.cells);
        self.y = top + h;
        top
    }

    /// The form's pieces, and how tall it is with the page's margin under it.
    pub fn end(self) -> (Vec<Cell>, f32) {
        let h = self.y + space::MARGIN;
        (self.cells, h)
    }
}

/// A section's box being filled, row by row, its next row's top at `y`.
pub struct Card {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub cells: Vec<Cell>,
    rows: usize,
}

impl Card {
    fn new(x: f32, top: f32, w: f32) -> Card {
        Card { x, y: top + space::PAD, w, cells: Vec::new(), rows: 0 }
    }

    /// Where what the box holds starts, and how wide it may be.
    pub fn inner_x(&self) -> f32 {
        self.x + space::PAD
    }
    pub fn inner_w(&self) -> f32 {
        self.w - 2.0 * space::PAD
    }

    /// Where a row's controls start, after its label's column, and how wide they may be.
    pub fn control_x(&self) -> f32 {
        self.inner_x() + size::LABEL + space::PAD
    }
    pub fn control_w(&self) -> f32 {
        self.inner_w() - size::LABEL - space::PAD
    }

    /// A new row: a rule after the row before it, Space.row either side.
    pub fn row(&mut self) {
        if self.rows > 0 {
            self.y += space::ROW;
            self.rule();
            self.y += 1.0 + space::ROW;
        }
        self.rows += 1;
    }

    /// A rule across the box at `y`.
    pub fn rule(&mut self) {
        self.cells.push(rule(self.inner_x(), self.y, self.inner_w()));
    }

    /// The row's label, level with a control on the row's first line.
    pub fn label(&mut self, s: &str) {
        self.label_on(s, LINE);
    }

    /// The row's label, level with words on the row's first line (a fact, a value).
    pub fn label_text(&mut self, s: &str) {
        self.label_on(s, TEXT_LINE);
    }

    fn label_on(&mut self, s: &str, h: f32) {
        self.cells.push(text_cell(style::LABEL, self.inner_x(), self.y, size::LABEL, h, s));
    }

    /// Words to read in the row's controls' column, a line each, from `y`; where they end.
    pub fn value_lines(&mut self, y: f32, s: &str) -> f32 {
        let (x, w) = (self.control_x(), self.control_w());
        let lines = crate::text::wrap(font::REGULAR, font::TEXT, s, w);
        for (k, l) in lines.iter().enumerate() {
            self.cells.push(text_cell(style::VALUE_BRIGHT, x, y + k as f32 * line::TEXT, w, line::TEXT, l));
        }
        y + lines.len().max(1) as f32 * line::TEXT
    }

    /// A note's lines at `x`, `w` wide, from `y`; where they end.
    pub fn note_at(&mut self, x: f32, y: f32, w: f32, s: &str) -> f32 {
        note_lines(&mut self.cells, x, y, w, style::NOTE, s)
    }

    /// A note under the row's controls, Space.tight under `y`; where it ends.
    pub fn note(&mut self, y: f32, s: &str) -> f32 {
        let (x, w) = (self.control_x(), self.control_w());
        self.note_at(x, y + space::TIGHT, w, s)
    }

    /// A status in the controls' column: its dot (`tone` as a Dot's state) and its words.
    pub fn status(&mut self, x: f32, y: f32, h: f32, tone: i32, s: &str) {
        let end = self.x + self.w - space::PAD;
        status(&mut self.cells, x, y, h, end, tone, style::VALUE_BRIGHT, s);
    }

    /// A row of a fact: its label, what it is, and a note under that where it has one.
    pub fn fact(&mut self, label: &str, value: &str, note: &str) {
        self.row();
        self.label_text(label);
        let top = self.y;
        let mut end = self.value_lines(top, value);
        if !note.is_empty() {
            end = self.note(end, note);
        }
        self.next(top, end - top);
    }

    /// A row of how something is, across the box: its dot and its words.
    pub fn status_row(&mut self, tone: i32, s: &str) {
        self.row();
        let (x, top) = (self.inner_x(), self.y);
        self.status(x, top, TEXT_LINE, tone, s);
        self.next(top, TEXT_LINE);
    }

    /// The row ends `h` under its top (at least a line's height): the next one starts there.
    pub fn next(&mut self, top: f32, h: f32) {
        self.y = top + h.max(TEXT_LINE);
    }
}

/// A switch, the same wherever one is (a form's row, a box's): its ground,
/// Size.control tall, and a button for each option (its words, and its icon's
/// lines where it has one), each as wide as its words or, given `across`, all
/// of them sharing that width; the chosen one lit. Each button's id, index and
/// style (1, not to be chosen now) from `of`. How wide it is.
pub fn switch(cells: &mut Vec<Cell>, x: f32, y: f32, across: Option<f32>, opts: &[(&str, &str)], chosen: Option<usize>, of: impl Fn(usize) -> (i32, i32, i32)) -> f32 {
    let n = opts.len().max(1) as f32;
    let (inset, gap) = (part::INSET, part::SEG_GAP);
    let widths: Vec<f32> = match across {
        Some(w) => vec![(w - 2.0 * inset - gap * (n - 1.0)) / n; opts.len()],
        None => opts.iter().map(|(o, _)| choice_width(o)).collect(),
    };
    let total = widths.iter().sum::<f32>() + gap * (n - 1.0) + 2.0 * inset;
    cells.push(cell(kind::SEG, x, y, total, size::CONTROL));
    let mut bx = x + inset;
    for (j, ((o, icon), w)) in opts.iter().zip(&widths).enumerate() {
        let (id, index, style) = of(j);
        cells.push(Cell { text: (*o).into(), lines: (*icon).into(), on: Some(j) == chosen, id, index, style, ..cell(kind::CHOICE, bx, y + inset, *w, size::CONTROL - 2.0 * inset) });
        bx += w + gap;
    }
    total
}

/// a switch's button as wide as its words
fn choice_width(o: &str) -> f32 {
    crate::text::width(font::REGULAR, font::NOTE, o).ceil() + 2.0 * part::CHOICE_PAD
}

/// How wide a switch's button is with these words, and a chevron after them where it
/// opens a list (`menu`), or a color's square before them too (`square`).
pub fn choice_w(words: &str, menu: bool, square: bool) -> f32 {
    choice_width(words) + if menu { part::CHOICE_GAP + CHEVRON } else { 0.0 } + if square { part::GLYPH + part::CHOICE_GAP } else { 0.0 }
}

/// How wide a switch of these options is, its buttons as wide as their words.
pub fn switch_width(opts: &[(&str, &str)]) -> f32 {
    let n = opts.len().max(1) as f32;
    opts.iter().map(|(o, _)| choice_width(o)).sum::<f32>() + part::SEG_GAP * (n - 1.0) + 2.0 * part::INSET
}

/// How wide a button with words is (a WordButton's, a cell's), and with a glyph before them.
pub fn button_width(words: &str, glyph: bool) -> f32 {
    crate::text::width(font::MEDIUM, font::CONTROL, words).ceil() + 2.0 * part::BUTTON_PAD + if glyph { part::GLYPH + part::GLYPH_GAP } else { 0.0 }
}

/// How wide a dropdown with these words is: its words, its padding at either end, and
/// its chevron (12 across) Space.row after its words.
pub fn drop_width(words: &str) -> f32 {
    crate::text::width(font::REGULAR, font::CONTROL, words).ceil() + 2.0 * part::FIELD_PAD + space::ROW + CHEVRON
}

/// How wide a chooser at a panel's head is (a DropField with no well, its words a step
/// heavier): as a dropdown is.
pub fn chooser_width(words: &str) -> f32 {
    crate::text::width(font::MEDIUM, font::CONTROL, words).ceil() + 2.0 * part::FIELD_PAD + space::ROW + CHEVRON
}

/// a dropdown's chevron, across (ui/system/controls.slint's DropField)
const CHEVRON: f32 = 12.0;

/// the widest a note's or a credit's lines run, for reading
pub const READ_MOST: f32 = 640.0;

/// How wide the small button inside a card is (Make main).
pub fn small_button_width(words: &str) -> f32 {
    crate::text::width(font::REGULAR, font::CAPTION, words).ceil() + 2.0 * part::PILL_PAD
}

/// A section's heading at `x`, `y`, `w` across: its words, a tag after them where it has
/// one (`tag`, a plugin's section's "Plugin", in the quiet gray), and what sits at its
/// right (`aside`, a note, or nothing). Where the section's box starts, Space.label under it.
pub fn heading(cells: &mut Vec<Cell>, x: f32, y: f32, w: f32, heading: &str, tag: &str, aside: &str) -> f32 {
    let sw = if aside.is_empty() { 0.0 } else { crate::text::width(font::REGULAR, font::NOTE, aside).ceil() + 2.0 };
    if tag.is_empty() {
        cells.push(text_cell(style::HEAD, x, y, w - sw, line::NOTE, heading));
    } else {
        let hw = crate::text::width(font::MEDIUM, font::CONTROL, heading).ceil() + 2.0;
        cells.push(text_cell(style::HEAD, x, y, hw, line::NOTE, heading));
        let tw = crate::text::width(font::REGULAR, font::CAPTION, tag).ceil() + 2.0;
        cells.push(Cell { color: crate::tokens::color(ink::QUIET), ..text_cell(style::SMALL_IN, x + hw + space::INLINE, y, tw, line::NOTE, tag) });
    }
    if !aside.is_empty() {
        cells.push(text_cell(style::NOTE, x + w - sw, y, sw, line::NOTE, aside));
    }
    y + line::NOTE + space::LABEL
}

/// A note's lines, `w` wide from `x`, `y`, in `style` (a note's gray, or `NOTE_IN` in a color
/// of its own: give the cells it pushes their color); where they end.
pub fn note_lines(cells: &mut Vec<Cell>, x: f32, y: f32, w: f32, style: i32, s: &str) -> f32 {
    let lines = crate::text::wrap(font::REGULAR, font::NOTE, s, w);
    for (k, l) in lines.iter().enumerate() {
        cells.push(text_cell(style, x, y + k as f32 * line::NOTE, w, line::NOTE, l));
    }
    y + lines.len() as f32 * line::NOTE
}

/// A note's lines in a color of their own (a warning's amber, a credit's gray); where they end.
pub fn note_lines_in(cells: &mut Vec<Cell>, x: f32, y: f32, w: f32, color: [u8; 3], s: &str) -> f32 {
    let from = cells.len();
    let end = note_lines(cells, x, y, w, style::NOTE_IN, s);
    for c in &mut cells[from..] {
        c.color = crate::tokens::color(color);
    }
    end
}

/// A rule across a box: a hairline, `w` long from `x`, at `y`.
pub fn rule(x: f32, y: f32, w: f32) -> Cell {
    Cell { color: crate::tokens::color(ink::LINE), ..cell(kind::BOX, x, y, w, 1.0) }
}

/// A status: its dot (`tone` as a Dot's state: 0 unknown, 1 good, 2 bad, 3 waiting) at
/// the middle of a line `h` tall from `x`, `y`, and its words (in `style`) after it, to `end`.
pub fn status(cells: &mut Vec<Cell>, x: f32, y: f32, h: f32, end: f32, tone: i32, style: i32, s: &str) {
    cells.push(Cell { style: tone, ..cell(kind::DOT, x, y + (h - size::DOT) / 2.0, size::DOT, size::DOT) });
    let wx = x + size::DOT + space::INLINE;
    cells.push(text_cell(style, wx, y, (end - wx).max(1.0), h, s));
}

/// The words of a pill of `style` (as the window draws it: 1, a warning's tag, small; the
/// others a caption's, 0 the value of a setting a step heavier).
fn pill_words(style: i32, s: &str) -> f32 {
    let (weight, size) = match style {
        0 => (font::MEDIUM, font::CAPTION),
        1 => (font::REGULAR, font::SMALL),
        _ => (font::REGULAR, font::CAPTION),
    };
    crate::text::width(weight, size, s).ceil()
}

/// How wide a pill of `style` with these words is.
pub fn pill_width(style: i32, s: &str) -> f32 {
    pill_words(style, s) + 2.0 * part::PILL_PAD
}

/// A pill: a value in a small well, or a tag (`style` as the window's), from `x` at the
/// middle of the line of words whose top is `y`; how wide it is.
pub fn pill(cells: &mut Vec<Cell>, style: i32, x: f32, y: f32, s: &str) -> f32 {
    let w = pill_width(style, s);
    cells.push(Cell { text: s.into(), style, ..cell(kind::PILL, x, y + (line::TEXT - size::PILL) / 2.0, w, size::PILL) });
    w
}

/// A warning under what it's about: an amber dot at the middle of its first line, and its
/// lines in amber after the dot, `w` across from `x`, `y`; where it ends.
pub fn warning(cells: &mut Vec<Cell>, x: f32, y: f32, w: f32, s: &str) -> f32 {
    cells.push(Cell { style: 3, ..cell(kind::DOT, x, y + (line::NOTE - size::DOT) / 2.0, size::DOT, size::DOT) });
    let indent = size::DOT + space::INLINE;
    note_lines_in(cells, x + indent, y, w - indent, ink::WARN, s)
}

/// A button that adds (its + and its words) with a note after it, on a row from `x`, `y`,
/// `w` across: Add a core, Add a plugin, Add a script. Its id, index and how tall the row is.
pub fn add_row(cells: &mut Vec<Cell>, x: f32, y: f32, w: f32, words: &str, note: &str, id: i32, index: i32) -> f32 {
    let aw = button_width(words, true);
    cells.push(Cell { text: words.into(), lines: ADD.into(), style: 35, id, index, ..cell(kind::GHOST, x, y, aw, size::BUTTON) });
    let nx = x + aw + space::ROW;
    let lines = crate::text::wrap(font::REGULAR, font::NOTE, note, w - aw - space::ROW);
    // (a line beside the button at its middle; more lines from its top)
    let top = if lines.len() <= 1 { y + (size::BUTTON - line::NOTE) / 2.0 } else { y };
    let end = note_lines(cells, nx, top, w - aw - space::ROW, style::NOTE, note);
    (end - y).max(size::BUTTON)
}

/// the + on a button that adds, on 16 units
pub const ADD: &str = "M8 3v10M3 8h10";

/// A quiet link (Remove, Run) from `x`, its words at its middle on a line `h` tall from `y`.
pub fn link(cells: &mut Vec<Cell>, x: f32, y: f32, h: f32, words: &str, id: i32, index: i32) -> f32 {
    let w = crate::lighting::quiet_width(words, false);
    cells.push(Cell { text: words.into(), id, index, ..cell(kind::QUIET, x, y + (h - size::QUIET) / 2.0, w, size::QUIET) });
    w
}

/// How wide a checkbox and its words are.
pub fn check_width(label: &str) -> f32 {
    size::CHECK + part::CHECK_GAP + crate::text::width(font::REGULAR, font::CONTROL, label).ceil() + 2.0
}

/// How wide a chip (a zone's) is, with its tick when it's chosen.
pub fn chip_width(name: &str, ticked: bool) -> f32 {
    crate::text::width(font::REGULAR, font::NOTE, name).ceil() + 2.0 * part::CHIP_PAD + if ticked { part::CHIP_TICK } else { 0.0 }
}
