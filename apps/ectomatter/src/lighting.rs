//! The pieces the app's boxes are built of (a box's stack, its kinds of
//! pieces and words), what each piece does when used, and the Lighting
//! page's own: the effects' small pictures, the brush's ready colors and
//! the colors it painted, and the screen light's color. The Lighting page's
//! boxes are drawn from the keyboard's description (described.rs).
//!
//! Each box is a fixed width, so its pieces are placed from its own top
//! left, and the page places the boxes and wraps them: so chips wrap and
//! notes run on as they should, and a slider being dragged is never rebuilt
//! under the pointer.

use serde_json::json;

use crate::text;
use crate::tokens::{font, line, part, size, space};
use crate::z10::Page;
use crate::Cell;

/// What a piece of the tile does when used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ctl {
    None,
    /// the brush's color, from the picker
    Paint,
    /// Reactive's pretend typing; an effect's settings put back as the factory has them
    Demo,
    Reset,
    Fill,
    Scatter,
    Clear,
    Name,
    LcdColor,
    LcdBrightness,
    LcdApp,
    GameKey,
    SliderBind,
    Control,
    /// a plugin's own Settings section: one of a setting's choices, and a box
    /// to type in (each by its place in the page's PluginMap)
    PluginChoice,
    PluginField,
    HdrMode,
    PeakNits,
    /// one of the brush's ready colors
    Preset,
    /// one of the screen light's ready colors
    LcdPreset,
    /// the screen's layout editor: a block's words, its reading (the menu),
    /// °C or °F, a 24- or 12-hour clock, what its words do when they don't fit
    /// (0 Cut off, 1 Scroll), its alignment, taking it off (it moves and sizes
    /// by dragging); the layout's screen light
    ScrText,
    ScrSource,
    ScrUnits,
    ScrHours,
    ScrFit,
    ScrAlign,
    ScrRemove,
    ScrLight,
    /// how a block showing the date writes it (the menu)
    ScrFormat,
    /// how fast an effect moves
    ScrSpeed,
    /// the Sensor Feed box in Settings: a tab, by its place; a sensor checked or unchecked, and
    /// its pencil (which the window gives the id after the sensor's), by the sensor's place;
    /// a folded run opened or shut; a card made the main card; a sensor's new name typed
    SensorTab,
    Sensor,
    SensorPencil,
    SensorRun,
    MakeMain,
    SensorName,
    /// a link to Ectomatter's Settings, from the screen editor
    OpenSettings,
    /// the mouse's page: the wheel copying the logo
    MouseSameWheel,
    /// a setting a core described (described.rs): which one the piece's id says
    Described,
    /// the color of the pad chosen on Home's desk
    DeskPad,
    /// a game's or app's page in the Library (games_page.rs): a device's
    /// dropdown, by the device's row
    LibChoice,
    /// and in a plugin's section: a setting's dropdown, by its place among the
    /// page's; a script, its Run now, and Add a script
    LibSetting,
    LibScript,
    LibScriptRun,
    LibScriptAdd,
    /// Settings' Devices and Plugins: a package's On and Off, by its place among the
    /// packages; Add a core (0) or a plugin (1); Remove on one added
    PackageOn,
    PackageAdd,
    PackageRemove,
    /// and in a plugin's section, by its place among the page's (games_page.rs, Spot): a
    /// choice's option, an action, a fold, a variable added or taken off; a checkbox (its
    /// place twice, and one for checked); a value to type (given as it's left)
    LibSpot,
    LibCheck,
    LibText,
    /// a dot beside a label on a game's page, by its place among the page's
    /// (games_page.rs, Reset): the row the game changed, put back to Every game's
    LibReset,
    /// the scene form's name, Whole desk or Each device, a wave's way, a color,
    /// a speed, a device's effect, a plugin device's profile
    SceneName,
    SceneLights,
    SceneWay,
    SceneColor,
    SceneSpeed,
    ScenePart,
    ScenePlugin,
}

const CTLS: [Ctl; 59] = [
    Ctl::None,
    Ctl::Paint,
    Ctl::Demo,
    Ctl::Reset,
    Ctl::Fill,
    Ctl::Scatter,
    Ctl::Clear,
    Ctl::Name,
    Ctl::LcdColor,
    Ctl::LcdBrightness,
    Ctl::LcdApp,
    Ctl::GameKey,
    Ctl::SliderBind,
    Ctl::Control,
    Ctl::PluginChoice,
    Ctl::PluginField,
    Ctl::HdrMode,
    Ctl::PeakNits,
    Ctl::Preset,
    Ctl::LcdPreset,
    Ctl::ScrText,
    Ctl::ScrSource,
    Ctl::ScrUnits,
    Ctl::ScrHours,
    Ctl::ScrFit,
    Ctl::ScrAlign,
    Ctl::ScrRemove,
    Ctl::ScrLight,
    Ctl::ScrFormat,
    Ctl::ScrSpeed,
    Ctl::SensorTab,
    Ctl::Sensor,
    Ctl::SensorPencil,
    Ctl::SensorRun,
    Ctl::MakeMain,
    Ctl::SensorName,
    Ctl::OpenSettings,
    Ctl::MouseSameWheel,
    Ctl::Described,
    Ctl::DeskPad,
    Ctl::LibChoice,
    Ctl::LibSetting,
    Ctl::LibScript,
    Ctl::LibScriptRun,
    Ctl::LibScriptAdd,
    Ctl::PackageOn,
    Ctl::PackageAdd,
    Ctl::PackageRemove,
    Ctl::LibSpot,
    Ctl::LibCheck,
    Ctl::LibText,
    Ctl::LibReset,
    Ctl::SceneName,
    Ctl::SceneLights,
    Ctl::SceneWay,
    Ctl::SceneColor,
    Ctl::SceneSpeed,
    Ctl::ScenePart,
    Ctl::ScenePlugin,
];

pub fn ctl(id: i32) -> Ctl {
    CTLS.get(id as usize).copied().unwrap_or(Ctl::None)
}
pub fn id(c: Ctl) -> i32 {
    CTLS.iter().position(|x| *x == c).unwrap_or(0) as i32
}

/// A slider's run, where the slider sets no light's color: the window's own
/// gray, so the only colors on a page are the lights' it sets.
pub(crate) fn run() -> slint::Color {
    crate::tokens::color(crate::tokens::ink::RUN)
}

/// the pieces' kinds, as ui/system/cells.slint draws them: every kind it draws
/// is named here, used from Rust or not, and none it doesn't draw
/// (tokens::system::cells_placed_are_drawn)
#[allow(dead_code)]
pub mod kind {
    pub const TEXT: i32 = 0;
    pub const SEG: i32 = 4;
    pub const SEG_BUTTON: i32 = 5;
    pub const CHIP: i32 = 6;
    pub const GHOST: i32 = 7;
    pub const LINK: i32 = 8;
    pub const TILE: i32 = 9;
    pub const FIELD: i32 = 10;
    pub const DEVICE: i32 = 12;
    pub const WBOX: i32 = 13;
    pub const DOT: i32 = 14;
    /// a number to type, as the lab's values are (its rows place it themselves)
    pub const NUMBER: i32 = 15;
    /// a box in its color, rounded at its ends
    pub const BOX: i32 = 16;
    /// a place that tells the page when the pointer is over it
    pub const ZONE: i32 = 17;
    /// a link in a sentence, its words brighter than the note round them
    pub const INLINE_LINK: i32 = 18;
    // the Lighting page's own pieces, in the shades of its boxes:
    // a value in a small well; a slider filled to its place; a color's dot;
    // a piece of a run of colors; a switch's choice; an area's chip; a
    // dropdown; a quiet link; a checkbox
    pub const PILL: i32 = 40;
    pub const FILLED: i32 = 41;
    pub const COLOR_DOT: i32 = 42;
    pub const STRIP: i32 = 43;
    pub const CHOICE: i32 = 44;
    pub const AREA: i32 = 45;
    pub const DROP: i32 = 46;
    pub const QUIET: i32 = 47;
    pub const CHECKBOX: i32 = 48;
    /// a thing the firmware adds, a tick before it
    pub const TICKED: i32 = 49;
    /// the Sensor Feed box's: a tab (the Screen tab's own); a sensor's checkbox, its names and
    /// its reading, or a folded run of them
    pub const TAB: i32 = 50;
    pub const SENSOR: i32 = 51;
    /// words that say more under the pointer: a device's whole name, and where its readings come from
    pub const TIP: i32 = 52;
    /// a fold's heading in a plugin's section, which opens and shuts it
    pub const FOLD: i32 = 53;
    /// a dot beside a label on a game's page, where the game changed the row from
    /// Every game's: selecting it puts the row back; what it does under the pointer
    pub const RESET: i32 = 54;
}
/// the text styles
#[allow(dead_code)]
pub mod style {
    pub const LABEL: i32 = 0;
    pub const HEAD: i32 = 1;
    pub const NOTE: i32 = 2;
    pub const EMPTY: i32 = 3;
    pub const VALUE: i32 = 4;
    pub const VALUE_BRIGHT: i32 = 5;
    pub const ITEM: i32 = 6;
    pub const TITLE: i32 = 7;
    pub const NAME: i32 = 8;
    pub const SMALL: i32 = 9;
    pub const BIG: i32 = 10;
    pub const LABEL_SOFT: i32 = 11;
    pub const FACT: i32 = 12;
    pub const FACT_VALUE: i32 = 13;
    pub const CREDIT: i32 = 14;
    /// a note in its own color (the lab's menus), and a menu's
    /// words in their own color
    pub const NOTE_IN: i32 = 19;
    pub const WORDS_IN: i32 = 20;
    /// the lab's small words in their own color, and its
    /// stronger ones (a reading's number, the name of what its line tells of)
    pub const SMALL_IN: i32 = 21;
    pub const LABEL_IN: i32 = 22;
    /// a segmented button that tells the page when the pointer is over it, as a zone does
    pub const TELLS: i32 = 23;
    /// a group's heading, in small capitals; a color's code; a setting's
    /// name; the words at a slider's ends, left and right
    pub const GROUP: i32 = 33;
    pub const CODE: i32 = 34;
    pub const PROP: i32 = 35;
    pub const END_L: i32 = 36;
    pub const END_R: i32 = 37;
    /// a card's title, as a box's
    pub const BOX_TITLE: i32 = 38;
    /// a command as it was typed, in the typeface whose letters are all one width
    pub const COMMAND: i32 = 41;
    /// a heading inside a plugin's section, in capitals; what a status says, beside its dot
    pub const SUB: i32 = 42;
    pub const STATUS: i32 = 43;
    /// a row's value that's only to read, quieter
    pub const VALUE_QUIET: i32 = 44;
}

pub(crate) fn cell(kind: i32, x: f32, y: f32, w: f32, h: f32) -> Cell {
    Cell { kind, x, y, w, h, text: "".into(), fill: "".into(), lines: "".into(), color: slint::Color::from_rgb_u8(0, 0, 0), on: false, value: 0.0, id: 0, index: 0, style: 0 }
}
pub(crate) fn text_cell(style: i32, x: f32, y: f32, w: f32, h: f32, s: &str) -> Cell {
    Cell { text: s.into(), style, ..cell(kind::TEXT, x, y, w, h) }
}

/// The colors a profile paints its keys with, one a key: kept by the window,
/// since the keyboard stores none.
pub fn custom_colors(n: u8) -> Vec<[u8; 3]> {
    crate::uistate::get(&format!("custom:{n}")).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_else(|| vec![[0; 3]; 116])
}
pub fn save_custom(n: u8, cols: &[[u8; 3]]) {
    crate::uistate::set(&format!("custom:{n}"), json!(cols));
}

/// the brush's ready colors; black turns a key off
pub const PRESETS: [[u8; 3]; 9] = [[0xff, 0x08, 0x00], [0xff, 0x80, 0x00], [0xff, 0xd0, 0x00], [0x3b, 0xdc, 0x6b], [0x3f, 0xa9, 0xff], [0x6a, 0x3b, 0xff], [0xff, 0x3b, 0xd2], [0xff, 0xff, 0xff], [0, 0, 0]];

pub(crate) const RESET_ICON: &str = "M4 12a8 8 0 1 0 2.4-5.7M4 4v4h4";
const CW: &str = "M20 12a8 8 0 1 1-2.6-5.9M20 4v5h-5";
const CCW: &str = "M4 12a8 8 0 1 0 2.6-5.9M4 4v5h5";
const UP: &str = "M12 20V5M7 10l5-5 5 5";
const DOWN: &str = "M12 4v15M7 14l5 5 5-5";
const LEFT: &str = "M20 12H5M10 7l-5 5 5 5";
const RIGHT: &str = "M4 12h15M14 7l5 5-5 5";

/// The drawing of an icon a choice's option names, for its switch.
pub(crate) fn icon_path(name: &str) -> &'static str {
    match name {
        "clockwise" => CW,
        "counterclockwise" => CCW,
        "up" => UP,
        "down" => DOWN,
        "left" => LEFT,
        "right" => RIGHT,
        _ => "",
    }
}

pub(crate) fn hex_of(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}
fn color(c: [u8; 3]) -> slint::Color {
    slint::Color::from_rgb_u8(c[0], c[1], c[2])
}

/// A box's pieces, top to bottom, Space.pad in from its sides: a stacked form.
/// Each setting's name is Space.label over its control, settings are Space.pad
/// apart (`next`), what belongs to the setting before it Space.row under it
/// (`then`), and a control's own note Space.tight under it (`close`); the first
/// piece starts under the box's title, and the box ends Space.pad under the last.
pub(crate) struct Stack {
    /// where what the box holds starts, and how wide it is
    pub(crate) x: f32,
    pub(crate) iw: f32,
    pub(crate) y: f32,
    pub(crate) cells: Vec<Cell>,
    groups: usize,
    /// nothing laid yet: the first piece takes no gap before it
    empty: bool,
}

impl Stack {
    pub(crate) fn new(w: f32) -> Stack {
        // under the box's title
        Stack { x: space::PAD, iw: (w - 2.0 * space::PAD).max(0.0), y: size::BOX_HEAD, cells: Vec::new(), groups: 0, empty: true }
    }
    fn gap(&mut self, h: f32) {
        if !self.empty {
            self.y += h;
        }
    }
    /// Space.pad down to the next setting
    pub(crate) fn next(&mut self) {
        self.gap(space::PAD);
    }
    /// Space.row down to what belongs to the setting before it (its note, its zones)
    pub(crate) fn then(&mut self) {
        self.gap(space::ROW);
    }
    /// Space.tight down to a control's own note, or a note's next paragraph
    pub(crate) fn close(&mut self) {
        self.gap(space::TIGHT);
    }
    fn laid(&mut self) {
        self.empty = false;
    }
    pub(crate) fn note(&mut self, s: &str) {
        self.y = crate::form::note_lines(&mut self.cells, self.x, self.y, self.iw, style::NOTE, s);
        self.laid();
    }
    /// a note under a checkbox, under its words
    pub(crate) fn check_note(&mut self, s: &str) {
        let indent = size::CHECK + part::CHECK_GAP;
        self.y = crate::form::note_lines(&mut self.cells, self.x + indent, self.y, self.iw - indent, style::NOTE, s);
        self.laid();
    }
    /// a group: a hairline over all but the first, then its name in small capitals; the next
    /// group's name Space.section under the last setting, its rule Space.pad under that
    pub(crate) fn group(&mut self, name: &str) {
        if self.groups > 0 {
            self.cells.push(crate::form::rule(self.x, self.y + space::PAD, self.iw));
            self.y += space::SECTION;
        } else {
            self.then();
        }
        self.groups += 1;
        self.caps(name);
    }
    /// a hairline across the box, Space.pad either side
    pub(crate) fn rule(&mut self) {
        self.cells.push(crate::form::rule(self.x, self.y + space::PAD, self.iw));
        self.y += 2.0 * space::PAD + 1.0;
    }
    /// a name in small capitals over what it names, Space.row under it
    pub(crate) fn caps(&mut self, name: &str) {
        self.cells.push(text_cell(style::GROUP, self.x, self.y, self.iw, line::SMALL, &name.to_uppercase()));
        self.y += line::SMALL + space::ROW;
        self.empty = true;
    }
    /// a setting's name, and its value in a pill at the right; its control Space.label under it
    pub(crate) fn head(&mut self, label: &str, value: Option<&str>) {
        self.cells.push(text_cell(style::PROP, self.x, self.y, self.iw, line::TEXT, label));
        if let Some(v) = value {
            let w = crate::form::pill_width(0, v);
            crate::form::pill(&mut self.cells, 0, self.x + self.iw - w, self.y, v);
        }
        self.y += line::TEXT + space::LABEL;
        self.empty = true;
    }
    /// a slider, run to its place in `fill` (run(), or the light's color it sets), its ends named under it
    pub(crate) fn slider_at(&mut self, id: i32, frac: f32, fill: slint::Color, ends: Option<(&str, &str)>) {
        self.cells.push(Cell { value: frac, color: fill, id, ..cell(kind::FILLED, self.x, self.y, self.iw, size::SLIDER) });
        self.y += size::SLIDER;
        if let Some((l, r)) = ends {
            self.y += space::TIGHT;
            self.cells.push(text_cell(style::END_L, self.x, self.y, self.iw / 2.0, line::SMALL, l));
            self.cells.push(text_cell(style::END_R, self.x + self.iw / 2.0, self.y, self.iw / 2.0, line::SMALL, r));
            self.y += line::SMALL;
        }
        self.laid();
    }
    /// a run of colors, as the effect blends them, Space.row over its colors' dots
    pub(crate) fn strip(&mut self, cols: &[[u8; 3]]) {
        let n = cols.len().max(2) - 1;
        let w = self.iw / n as f32;
        for k in 0..n {
            let (a, b) = (cols[k], cols[(k + 1).min(cols.len() - 1)]);
            let ends = (if k == 0 { 1 } else { 0 }) | (if k == n - 1 { 2 } else { 0 });
            self.cells.push(Cell { color: color(a), value: (b[0] as u32 * 65536 + b[1] as u32 * 256 + b[2] as u32) as f32, style: ends, ..cell(kind::STRIP, self.x + k as f32 * w, self.y, w + 0.5, size::STRIP) });
        }
        self.y += size::STRIP + space::ROW;
        self.empty = true;
    }
    /// colors as dots, Space.inline apart, wrapping; the one the picker is open on ringed
    pub(crate) fn dots_at(&mut self, id: i32, cols: &[[u8; 3]], open: Option<usize>, code: bool) {
        let dots: Vec<Cell> = cols.iter().enumerate().map(|(j, col)| Cell { color: color(*col), on: open == Some(j), id, index: j as i32, ..cell(kind::COLOR_DOT, 0.0, 0.0, size::SWATCH, size::SWATCH) }).collect();
        let (end_x, top) = self.dots(dots);
        if code && cols.len() == 1 {
            let hex = hex_of(cols[0]);
            let cw = text::width(font::MEDIUM, font::CAPTION, &hex).ceil() + 2.0;
            self.cells.push(text_cell(style::CODE, end_x + space::TIGHT, top, cw, size::SWATCH, &hex));
        }
    }
    /// dots (each a swatch's size), Space.inline apart, wrapping at the box's padding; where
    /// the last one ends across, and its row's top
    pub(crate) fn dots(&mut self, dots: Vec<Cell>) -> (f32, f32) {
        let pitch = size::SWATCH + space::INLINE;
        let (mut x, mut y) = (self.x, self.y);
        for d in dots {
            if x > self.x && x + size::SWATCH > self.x + self.iw {
                x = self.x;
                y += pitch;
            }
            self.cells.push(Cell { x, y, ..d });
            x += pitch;
        }
        self.y = y + size::SWATCH;
        self.laid();
        (x - space::INLINE, y)
    }
    /// a switch across the box: each choice its share, with its icon if it has one
    /// (`chosen` none, past the last, while the device can't say which)
    pub(crate) fn switch_at(&mut self, id: i32, opts: &[(&str, &str)], chosen: usize) {
        crate::form::switch(&mut self.cells, self.x, self.y, Some(self.iw), opts, Some(chosen).filter(|c| *c < opts.len()), |j| (id, j as i32, 0));
        self.y += size::CONTROL;
        self.laid();
    }
    pub(crate) fn check(&mut self, c: Ctl, on: bool, label: &str) {
        self.check_at(id(c), on, label);
    }
    pub(crate) fn check_at(&mut self, id: i32, on: bool, label: &str) {
        self.cells.push(Cell { on, id, text: label.into(), ..cell(kind::CHECKBOX, self.x, self.y, crate::form::check_width(label).min(self.iw), size::QUIET) });
        self.y += size::QUIET;
        self.laid();
    }
    /// a dropdown showing the choice it is on
    pub(crate) fn dropdown_at(&mut self, id: i32, shown: &str, open: bool) {
        self.cells.push(Cell { text: shown.into(), on: open, id, ..cell(kind::DROP, self.x, self.y, self.iw, size::CONTROL) });
        self.y += size::CONTROL;
        self.laid();
    }
    /// zones as chips, Space.inline apart, wrapping, a chosen one ticked; `index` is each
    /// one's place in the light's zones. Faded (style 2) while the whole
    /// keyboard is lit; `hot` is the one the pointer is over on the keyboard's picture.
    pub(crate) fn chips(&mut self, c: Ctl, chips: &[(usize, &str, bool)], faded: bool, hot: Option<usize>) {
        self.chips_at(id(c), chips, faded, hot);
    }
    pub(crate) fn chips_at(&mut self, id: i32, chips: &[(usize, &str, bool)], faded: bool, hot: Option<usize>) {
        let (mut x, mut y) = (0.0f32, self.y);
        for (index, name, on) in chips {
            let w = crate::form::chip_width(name, *on);
            if x > 0.0 && x + w > self.iw {
                x = 0.0;
                y += size::CHIP + space::INLINE;
            }
            let hot = if hot == Some(*index) { 1.0 } else { 0.0 };
            self.cells.push(Cell { text: (*name).into(), on: *on, style: if faded { 2 } else { 1 }, value: hot, id, index: *index as i32, ..cell(kind::AREA, self.x + x, y, w, size::CHIP) });
            x += w + space::INLINE;
        }
        self.y = y + size::CHIP;
        self.laid();
    }
    /// quiet links in a row, Space.tight apart, the first one's words at the box's padding
    pub(crate) fn links(&mut self, links: &[(Ctl, &str)]) {
        let mut x = self.x - part::QUIET_PAD;
        for (c, label) in links {
            let w = quiet_width(label, false);
            self.cells.push(Cell { text: (*label).into(), id: id(*c), ..cell(kind::QUIET, x, self.y, w, size::QUIET) });
            x += w + space::TIGHT;
        }
        self.y += size::QUIET;
        self.laid();
    }
    /// a quiet link at the box's top right, level with its title, its words ending at the
    /// box's padding
    pub(crate) fn title_link(&mut self, c: Ctl, label: &str, icon: &str) {
        let w = quiet_width(label, !icon.is_empty());
        let y = part::TITLE_TOP + (line::TITLE - size::QUIET) / 2.0;
        self.cells.push(Cell { text: label.into(), lines: icon.into(), id: id(c), ..cell(kind::QUIET, self.x + self.iw - w + part::QUIET_PAD, y, w, size::QUIET) });
    }
    /// how tall the box is: Space.pad under its last piece
    pub(crate) fn height(&self) -> f32 {
        self.y + space::PAD
    }
}

/// Set the screen light's color the picker chose.
pub fn set_color(page: &mut Page, c: Ctl, rgb: [u8; 3]) {
    let n = page.editing;
    let Some(d) = page.drafts.get_mut(&n) else { return };
    if c == Ctl::LcdColor {
        d["lighting"]["screen"]["color"]["color"] = json!(rgb);
    }
}

/// The color a swatch shows now: the screen light's, or the brush's.
pub fn color_of(page: &Page, c: Ctl, paint: [u8; 3]) -> [u8; 3] {
    if c != Ctl::LcdColor {
        return paint;
    }
    let v = &page.draft()["lighting"]["screen"]["color"]["color"];
    [v[0].as_u64().unwrap_or(0) as u8, v[1].as_u64().unwrap_or(0) as u8, v[2].as_u64().unwrap_or(0) as u8]
}

/// Each effect's small picture, for its tile on the Lighting page, at `scale`:
/// 72 by 40, and Wave's twice as wide, its colors round twice, to roll along.
pub fn icon(mode: &str, scale: f32, wide: f32) -> slint::Image {
    // (as wide as its tile shows it, so nothing is stretched to fit)
    #[allow(non_snake_case)]
    let W: f32 = wide.max(1.0);
    const H: f32 = part::TILE_PIC;
    // (Wave rolls along a picture twice as wide; Pulse is five tiles wide, a color each)
    let across = if mode == "wave" { 2.0 } else if mode == "pulse" { 5.0 } else { 1.0 };
    let (w, h) = ((W * across * scale).round() as u32, (H * scale).round() as u32);
    let hex = |s: &str| -> [f32; 3] {
        let s = s.trim_start_matches('#');
        let s: String = if s.len() == 3 { s.chars().flat_map(|c| [c, c]).collect() } else { s.into() };
        let v = u32::from_str_radix(&s, 16).unwrap_or(0);
        [((v >> 16) & 255) as f32, ((v >> 8) & 255) as f32, (v & 255) as f32]
    };
    // an even run of colors, as a CSS gradient with no stops given lays them
    let run = |cols: &[&str], t: f32| -> [f32; 3] {
        let c: Vec<[f32; 3]> = cols.iter().map(|s| hex(s)).collect();
        let x = t.clamp(0.0, 1.0) * (c.len() - 1) as f32;
        let i = (x.floor() as usize).min(c.len() - 2);
        let f = x - i as f32;
        [0, 1, 2].map(|k| c[i][k] + (c[i + 1][k] - c[i][k]) * f)
    };
    let dark = hex("#0c0e10");
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
    let (fw, fh) = (w as f32, h as f32);
    // (square: the tile rounds its corners)
    let r = 0.0f32;
    for (i, p) in buf.make_mut_slice().iter_mut().enumerate() {
        let (px, py) = ((i as u32 % w) as f32 + 0.5, (i as u32 / w) as f32 + 0.5);
        let u = px / fw;
        // in the picture's own CSS pixels
        let (cx, cy) = (px / scale, py / scale);
        let c = match mode {
            "static" => hex("#d8421e"),
            "breathing" => run(&["#140806", "#d8421e", "#140806"], u),
            // five colors side by side, hard against each other: the keys take
            // each in turn, with nothing between (the tile, chosen, shows one
            // at a time)
            "pulse" => hex(["#f33", "#fd3", "#3f6", "#3cf", "#94f"][((u * 5.0) as usize).min(4)]),
            "spiral" => {
                // conic, from 90deg: 0 at the right, going round clockwise
                let a = (cy - H / 2.0).atan2(cx - W / 2.0).to_degrees();
                let t = ((a + 360.0) % 360.0) / 360.0;
                run(&["#f33", "#fd3", "#3f6", "#3cf", "#94f", "#f33"], t)
            }
            "wave" => run(&["#f33", "#fd3", "#3f6", "#3cf", "#94f", "#f33"], (u * across) % 1.0),
            "stars" => {
                let mut c = dark;
                for (sx, sy, col) in [(0.2, 0.4, "#fff"), (0.7, 0.65, "#9cf"), (0.85, 0.25, "#fc9"), (0.42, 0.78, "#fff")] {
                    let d = ((cx - sx * W).powi(2) + (cy - sy * H).powi(2)).sqrt();
                    let a = (3.2 - d).clamp(0.0, 1.0);
                    let s = hex(col);
                    c = [0, 1, 2].map(|k| c[k] * (1.0 - a) + s[k] * a);
                }
                c
            }
            "trigger" => {
                let d = ((cx - W / 2.0).powi(2) + (cy - H / 2.0).powi(2)).sqrt();
                if d <= 4.0 {
                    hex("#7fe3ff")
                } else if d <= 10.0 {
                    run(&["#7fe3ff", "#1a7dc4"], (d - 4.0) / 6.0)
                } else {
                    run(&["#1a7dc4", "#0e0e12"], ((d - 10.0) / 10.0).min(1.0))
                }
            }
            "custom" => {
                // 135deg, as CSS lays it: along (sin, -cos) of the angle, the line
                // as long as the box's projection onto it
                let (sn, cs) = (0.70710677f32, 0.70710677f32);
                let len = W * sn + H * cs;
                let t = ((cx - W / 2.0) * sn + (cy - H / 2.0) * cs) / len + 0.5;
                match t {
                    t if t < 0.25 => hex("#f33"),
                    t if t < 0.5 => hex("#3cf"),
                    t if t < 0.75 => hex("#fd3"),
                    _ => hex("#8f5"),
                }
            }
            _ => dark,
        };
        // the rounded corners
        let qx = (r - px).max(px - (fw - r)).max(0.0);
        let qy = (r - py).max(py - (fh - r)).max(0.0);
        let a = if qx > 0.0 && qy > 0.0 { (r - (qx * qx + qy * qy).sqrt() + 0.5).clamp(0.0, 1.0) } else { 1.0 };
        *p = slint::Rgba8Pixel { r: (c[0] * a).round() as u8, g: (c[1] * a).round() as u8, b: (c[2] * a).round() as u8, a: (a * 255.0).round() as u8 };
    }
    slint::Image::from_rgba8_premultiplied(buf)
}

/// How wide a quiet link is (a LinkButton, ui/system/controls.slint): its words,
/// its room each side, and its glyph before them where it has one.
pub(crate) fn quiet_width(label: &str, glyph: bool) -> f32 {
    text::width(font::MEDIUM, font::NOTE, label) + 2.0 * part::QUIET_PAD + if glyph { part::GLYPH + part::GLYPH_GAP } else { 0.0 }
}
