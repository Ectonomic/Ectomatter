//! A form, laid out the same on every page that sets things in rows (Ectomatter's
//! Settings, a device's Settings tab, a game's page): a column of sections from
//! the page's margin, at most Size.form wide. Each section is a heading over its
//! box (Space.label between, Space.section after the box before it); in the box,
//! Space.pad in from its edges, rows, each a label in a column Size.label wide and
//! the row's controls Space.pad after it, a rule between rows with Space.row either
//! side. The window draws the box (a tile) and every piece where this puts it.

use crate::lighting::{cell, kind, style, text_cell};
use crate::tokens::{font, ink, size, space};
use crate::Cell;

/// the height a row's first line is given, a control's (a label sits level with it)
pub const LINE: f32 = size::CONTROL;
/// a line of a note, and of words read in a row
pub const NOTE_LINE: f32 = 18.0;
/// a line of words in a row (a fact, a value): the height a label sits level with
pub const TEXT_LINE: f32 = 20.0;

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
        let sw = if aside.is_empty() { 0.0 } else { crate::text::width(font::REGULAR, font::NOTE, aside).ceil() + 2.0 };
        self.cells.push(text_cell(style::HEAD, self.x, self.y, self.w - sw, 18.0, heading));
        if !aside.is_empty() {
            self.cells.push(text_cell(style::NOTE, self.x + self.w - sw, self.y, sw, 18.0, aside));
        }
        self.y += 18.0 + space::LABEL;
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
        self.cells.push(Cell { color: crate::tokens::color(ink::LINE), ..cell(kind::BOX, self.inner_x(), self.y, self.inner_w(), 1.0) });
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
        for (k, line) in lines.iter().enumerate() {
            self.cells.push(text_cell(style::VALUE_BRIGHT, x, y + k as f32 * TEXT_LINE, w, TEXT_LINE, line));
        }
        y + lines.len().max(1) as f32 * TEXT_LINE
    }

    /// A note's lines at `x`, `w` wide, from `y`; where they end.
    pub fn note_at(&mut self, x: f32, y: f32, w: f32, s: &str) -> f32 {
        let lines = crate::text::wrap(font::REGULAR, font::NOTE, s, w);
        for (k, line) in lines.iter().enumerate() {
            self.cells.push(text_cell(style::NOTE, x, y + k as f32 * NOTE_LINE, w, NOTE_LINE, line));
        }
        y + lines.len() as f32 * NOTE_LINE
    }

    /// A note under the row's controls, Space.tight under `y`; where it ends.
    pub fn note(&mut self, y: f32, s: &str) -> f32 {
        let (x, w) = (self.control_x(), self.control_w());
        self.note_at(x, y + space::TIGHT, w, s)
    }

    /// A status in the controls' column: its dot (`tone` as a Dot's state) and its words.
    pub fn status(&mut self, x: f32, y: f32, h: f32, tone: i32, s: &str) {
        self.cells.push(Cell { style: tone, ..cell(kind::DOT, x, y + (h - size::DOT) / 2.0, size::DOT, size::DOT) });
        let wx = x + size::DOT + 8.0;
        self.cells.push(text_cell(style::VALUE_BRIGHT, wx, y, self.x + self.w - space::PAD - wx, h, s));
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
    let widths: Vec<f32> = match across {
        Some(w) => vec![(w - 6.0 - 2.0 * (n - 1.0)) / n; opts.len()],
        None => opts.iter().map(|(o, _)| crate::text::width(font::REGULAR, font::NOTE, o).ceil() + 28.0).collect(),
    };
    let total = widths.iter().sum::<f32>() + 2.0 * (n - 1.0) + 6.0;
    cells.push(cell(kind::SEG, x, y, total, size::CONTROL));
    let mut bx = x + 3.0;
    for (j, ((o, icon), w)) in opts.iter().zip(&widths).enumerate() {
        let (id, index, style) = of(j);
        cells.push(Cell { text: (*o).into(), lines: (*icon).into(), on: Some(j) == chosen, id, index, style, ..cell(kind::CHOICE, bx, y + 3.0, *w, size::CONTROL - 6.0) });
        bx += w + 2.0;
    }
    total
}

/// How wide a switch of these options is, its buttons as wide as their words.
pub fn switch_width(opts: &[(&str, &str)]) -> f32 {
    let n = opts.len().max(1) as f32;
    opts.iter().map(|(o, _)| crate::text::width(font::REGULAR, font::NOTE, o).ceil() + 28.0).sum::<f32>() + 2.0 * (n - 1.0) + 6.0
}

/// How wide a button with words is (a WordButton's, a cell's), and with a glyph before them.
pub fn button_width(words: &str, glyph: bool) -> f32 {
    use crate::tokens::part;
    crate::text::width(font::MEDIUM, font::CONTROL, words).ceil() + 2.0 * part::BUTTON_PAD + if glyph { part::GLYPH + part::GLYPH_GAP } else { 0.0 }
}
