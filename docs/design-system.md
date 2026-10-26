# Ectomatter's design system

Every page in Ectomatter is built from one set of values and one set of parts.
No page sets a gutter, a size, a line's height or width, a corner, a type size,
a shadow or a color of its own: it takes them from the tokens, and it lays itself out with the
system's parts. A new design uses what is here. Where nothing here fits, the new
value or part is added here first, with what it is for, and then every page that
needs it uses it.

## Where it lives

| What | Where |
|---|---|
| The tokens, one file | `design/ectomatter.tokens.json` |
| What writes them out | `crates/ectomatter-tokens` (run by `apps/ectomatter/build.rs`) |
| The window's parts | `apps/ectomatter/ui/system/*.slint` |
| The tokens in Rust | `src/tokens.rs` (`crate::tokens::space::MARGIN`, `line::NOTE`, `ink::LINE`, ...) |
| The pieces Rust lays out with | `src/form.rs` (a form, and the pieces every Rust page shares) |
| A box's settings, top to bottom | `src/lighting.rs`'s `Stack` |
| Boxes laid out in Rust, in a row of boxes | `src/boxes.rs` with `CellBox` |
| A menu's width, height and where it hangs | `src/menu_size.rs` |
| The checks that keep it so | `src/tokens.rs`, `mod system` (`cargo test tokens::system`) |

The tokens file is in the Design Tokens Community Group's format
(designtokens.org): groups of tokens, each with a `$value`, a `$type` and a
`$description` saying what it is for. Colors are hex strings, the form Penpot
reads and writes. The build writes the file out twice: as Slint globals
(`Space`, `Radius`, `Font`, `Line`, `Size`, `Part`, `Stroke`, `Shadow`, `Ink`) and as a
Rust module of constants. Change a token in the file and every page that uses
it changes, in the window and in what Rust lays out.

## The tokens

**Space**, on a 4 px base, each for a role:

| Token | px | For |
|---|---|---|
| `margin` | 24 | A page's margin, its bar's too; the gap between cards in a grid; between the groups of a line of controls. |
| `gap` | 12 | Between boxes, side by side or one above another; under a stage. |
| `section` | 32 | Between a section's box and the next section's heading; in a box, from one group's last setting to the next group's heading. |
| `label` | 8 | From a heading down to what it heads; from a setting's name down to its control. |
| `pad` | 16 | A box's padding, and under its last piece; between a form's label column and its controls; between two settings stacked in a box, or a stacked form's fields; between a box's two columns. |
| `row` | 12 | Above and below the rule between two rows of a form; under a heading in small capitals; above a note on a whole group. |
| `inline` | 8 | Between controls side by side (or stacked in a row's column); a list's rows in from its box. |
| `tight` | 4 | Between things that belong together: a name and the line under it, a control and its note, a slider and its ends' words. |
| `list` | 2 | Between the rows of a list down a side. |
| `stage` | 48 | The gutter round a device on its stage, across (`margin` above and below it). |

**Line**, a line of words' height by what the words are, wherever Rust lays
words out (words sit at the middle of their line): `title` 22 (a box's title),
`text` 20 (words read on a row: a name, a value, a status), `note` 18 (a note,
a caption, a section's heading), `small` 14 (a heading in small capitals in a
box, a slider's ends).

**Radius**, by the size of what it rounds: `window` 10, `box` 12 (a box, a
card, a stage, a dialog), `panel` 10 (a menu, a tile, a well, a floating bar),
`control` 8 (a button, a field, a row), `small` 6 (a quiet button, a chip, a
tag, a tooltip, a switch's choice), `tiny` 4 (a checkbox, a color's square).
Something round takes half its height. A part set close inside another follows
its curve: a switch's ground is `small` and `Part.inset`, its choices `small`.

**Font**: the typefaces (`ui` Geist, `mono` JetBrains Mono), the weights
(`regular`, `medium`, `strong`), the ramp, each size named for what it sets
(`display` 22, `title` 17, `lead` 15, `tab` 14.5, `name` 14, `text` 13.5,
`control` 13, `note` 12.5, `caption` 12, `small` 11), and three trackings: a
heading in capitals' (`caps-tracking`, with `small` and `strong`, in
`Ink.quiet`), a page's title's, a reading shown large's.

**Size**, the sizes parts share across pages, so a part is the same size
wherever it is. The control heights: `control` 34 (a field, a dropdown, a
switch, in a form or a box), `button` 30 (a button with words; a line of
controls on a hint line or an editor's foot), `compact` 28 (a control on a
floating bar or a toolbar, a chooser at a panel's head), `quiet` 26 (a link, a
checkbox, an icon button), `seg-tall` 50 (a switch with a glyph over each
choice's words). The bands: `title-bar`, `bar`, `head`, `hint`, `box-head`,
`tab`, `row`, `list-head`, `menu-head`, `menu-rule`, `app`, `pick-row`,
`sensor`, `foot`. The pieces: `pill`, `swatch`, `slider`, `chip`, `strip`,
`tile`, `scene-tile`, `preview`, `hero`, `cover-name`, `dot`, `check`,
`round`, `reset`, `rail`, `block-chip`, `app-icon`, `pick-icon`, `spinner`,
`win-button`, `layout-card-h`. The widths: `column`, the boxes' least widths
(`library`, `detail`, `game`, `effects`, `settings`, `places`), `sidebar`,
`rail-open` and `rail-shut`, `form`, `label`, `dialog` and `ask` (a form's
dialog, and a dialog that asks one thing), `find`, `rename`, `picker`,
`empty`, `chooser`, `layout-card`, `edit-screen`, and a menu's: `menu` 220
and `menu-most` 420 (a menu is as wide as its rows need, between them),
`menu-search` 300 (a menu with a search box).

**Part**: a part's own measurements that Rust works sizes out from, so what
Rust places is as big as the window draws it: a button's and a quiet button's
room (`button-pad`, `quiet-pad`), a glyph and its gap (`glyph`, `glyph-gap`),
a box's title's top (`title-top`), a pill's, a chip's, a field's, a tab's, a
switch button's and a sensor chip's room (`pill-pad`, `chip-pad`, `chip-tick`,
`field-pad`, `tab-pad`, `choice-pad`, `choice-gap`, `sensor-pad`), a switch's
inset and gap (`inset`, `seg-gap`), a checkbox's gap to its words
(`check-gap`), a color button's dot (`swatch-words`), a menu's room
(`menu-pad`, `menu-in`), a fold's (`fold-in`), a scrollbar's place
(`scroll-in`), an effect tile's picture (`tile-inset`, `tile-pic`), the scene
preview's gap (`span-gap`), the macro chain's inset (`chain-in`); and how
far a pressed control sinks (`press`), a chevron and the small glyphs a control
carries (`chevron`), a primary button's extra (`primary`), a device bar's tab's
room (`tab-in`), how far a press moves before it carries (`drag`, one for every
carry), a drop ring's reach (`ring-out`), and a menu row's number box and the
share of its width its right side takes (`menu-number`, `menu-aside`).

**Stroke**, how thick a line is, by what it draws: `line` 1 (an edge, a rule,
a border, a grid's line), `chosen` 1.5 (the edge of what's chosen on a
picture), `ring` 2 (a ring round what's carried, dropped onto or selected, the
playhead).

**Shadow**, how far a surface lifts: `lift` (a cover, a tooltip), `float` (a
menu, the picker, a floating bar, something carried), `modal` (a dialog), and
`knob` (a slider's knob on its track).

**Ink**, the colors: the window's shades, each control's four states, the
outlines, the words (`bright`, `dim`, `soft`, `quiet`, `placeholder`, `off`),
and the signals (`warn`, `bad`, `good`, `danger`, `record`, `close`, `media`).
A part's light or dark wash is `white` or `black` at an opacity of the part's
own. Ink can be set while the app runs (`ECTOMATTER_INK`) for a color mock.

## The parts

In `ui/system/`, each drawn once:

- **bars**: `TitleBar`, `PageBar` (a page's name, a dot or the way back before
  it, its tabs, its status, Undo, its buttons), `EditorHeader` (an editor
  window's page bar), `HintRow` (the line under a page's bar), `Dot`.
- **surfaces**: `PageBox`, `ScrollBox`, `Stage`, `Scrim`, `DialogBox`,
  `DialogHead` (every dialog's title and the line under it), `FormDialog`
  (every form in a dialog: its head, the form, Remove at the left where it
  has one, Cancel and the primary), `Confirm`, `FloatBar` (a bar floating over
  the page: Home's edit bar and zoom), `EmptyState`, `TipBox` and `Tip`,
  `FormLabel`, `FormNote` and `FormWarning` (a form's label, the note under a
  control, and what's wrong with what was typed), `CarryChip` (what's carried,
  under the pointer), `ScreenBay` (the bay a device's screen is set into, on
  the Screen tab and in the Screen Editor).
- **controls**: `WordButton` (gray, `primary`, `danger`; `on-box`; `small`
  inside a card; `compact` on a floating bar; with a glyph, `lines`, a color's
  square, or a `menu` chevron), `TextButton` (a toolbar's quiet button),
  `LinkButton` (a box's quiet link), `IconButton`, `MoreButton`,
  `PencilButton`, `BarButton`, `StarButton` (`on-picture` over a cover),
  `RecordButton`, `Tab`, `FolderTab` (a tab over a box), `FieldBox` (every
  line to type in: a find box's lens, `read-only`, `centered`, its `fill` on a
  menu), `RenameField` (a name renamed in place), `DropField` (every dropdown:
  `subtle` at a panel's head, `followed`, `warn`, a chip), `SegSwitch` and its
  pieces `SegGround` and `SegChoice` (a choice's icon, a color's dot or square,
  a `menu` chevron), `CheckBox`, `RadioRow`, `Slider` (the one slider), `Tag`,
  `ColorButton`, `ColorSquare`, `Fold` (a heading that opens and shuts),
  `ScrollBar`, `Spinner`, `PickField`, `SmallCap`, `StripIcon`, `PlayButton`,
  and `Glyph` (the app's own glyphs).
- **lists**: `ListBox`, `ListRow`, `AnchorColumn`, `AnchorRow`, `AnchorHead`
  (every heading in capitals over a list), `RailItem`.
- **menu**: `Menu` (every menu: its rows, Back and its search box on the
  tokens; a tick column only in a menu that ticks; a row's pencil is the
  `PencilButton`), `Picker` (it lays itself out from its own named measures
  and says how tall it is, `natural`, for the window to hang it).
- **tiles**: `EffectTile`.
- **layout**: `BoxRow`.
- **cells**: `CellView`, which draws every piece Rust lays out, each kind with
  the part above (a switch's choice is the `SegChoice`, a dropdown the
  `DropField`, a button the `WordButton`, a slider the `Slider`, a tab the
  `FolderTab`, a fold the `Fold`), and `CellBox`, a box of such pieces in a row
  of boxes.

In Rust: `form::Form` and `form::Card` (a form and its rows); the pieces every
Rust page shares, in `form.rs`: `heading`, `note_lines` and `note_lines_in`,
`rule`, `status`, `pill`, `warning`, `link`, `add_row`, `switch`, and the
widths a part is drawn at (`button_width`, `small_button_width`,
`check_width`, `chip_width`, `drop_width`, `chooser_width`, `choice_w`,
`pill_width`); `lighting::Stack` (a box's settings top to bottom);
`boxes::Row` (boxes laid out in Rust for a row of boxes); `menu_size` (every
menu's width, height and place: `width`, `height`, `fit_notes`, `hang`).

## How a page is put together

**Every page**: the window's `TitleBar`, the page's `PageBar`, then its
`HintRow` where it has a hint (the page's head, `Size.head` tall), then what
it holds, from `Space.margin` at either side, starting at the head's end. A
page with no hint starts `Space.margin` under its bar. Home is the desk, the
devices on it to scale: it has no bar, only its hint row over the desk.

**A device's page**: the stage first, under the head; under it, `Space.gap`
down, the Profiles column (`Size.column`) and beside it the tab's boxes in a
`BoxRow`. In the row each box is at least its least width and grows from it as
the others do, so every line reaches the margin; a line's boxes are as tall as
each other, at most the room left in the window (a box with more scrolls). A
box whose pieces Rust lays out is a `CellBox`: it says how wide the row made
it, and Rust lays its pieces out to that width. A box drawn in Slint says how
tall it needs to be (`need`), and the page gives it that.

**A box of settings** (an effect's, a mouse's, a plugin's): a `Stack`. Its
first piece is at `Size.box-head`; each setting's name is `Space.label` over its
control; settings are `Space.pad` apart; a group's name in capitals is
`Space.row` over its first setting, the next group `Space.section` under the
last setting with a rule `Space.pad` under it; a note on a group is
`Space.row` under it, a control's own note `Space.tight` under it; the box ends
`Space.pad` under its last piece. The Screen Editor's box lays its settings two
to a row the same way.

**A form** (Settings, a device's Settings tab, a game's page): `form::Form`. A
column at most `Size.form` wide from the margin; each section a heading over
its box; in the box rows, each with its label in a column `Size.label` wide and
its controls `Space.pad` after it, a rule between rows. A row whose first piece
is words (a status, a value) sits on a line of words, its note `Space.tight`
under it, as a control's is. A form inside a dialog stacks each label over what
it labels: `FormLabel`, `Space.label`, the control, `Space.pad` to the next.

**A dialog**: a `FormDialog` (a press on the dim, Cancel or Escape cancels):
`Size.dialog` wide for a form, `Size.ask` for one that asks one thing; its
`DialogHead`, `Space.pad`, the form, `Space.margin`, and its buttons. A
question before something is deleted is a `Confirm`. The scene form keeps a
frame of its own (its preview stays put while its pieces scroll), from the
same head, margins and buttons.

**An editor window** (the Macro Editor, the Screen Editor): its `TitleBar`,
its list down the left (`Size.sidebar`, its rows `ListRow`s, `Space.inline` in
from the column), and beside it its `EditorHeader`, what it edits, and its foot
(a line of controls at a button's height).

**A menu**, every one, by one rule (`menu_size`, the Game Configurator spec's
menus): as wide as its rows need, measured from the tokens its rows are drawn
with, from `Size.menu` to `Size.menu-most`, and never narrower than what opened
it (a dropdown's menu is at least its field's width); one with a search box
is `Size.menu-search`, so it keeps its width as it's searched. It hangs
`Part.menu-pad` under what opened it where it fits, or where there's a fair
amount of room anyway, over it only when under is cramped; a menu opened at
the pointer hangs from the point. A press off it shuts it, and the press on
its own button just after doesn't open it again. Its notes wrap inside it.
Each window opens its menus in one place (`hang_menu` in the main window,
`place_menu` in the editors, `show_menus` in the lab), and a press on a cell
or a dropdown says how wide it is, so its menu knows.

## Adding a design

1. Build it from the parts above, with the tokens. A page's structure comes
   from the bars, `BoxRow`, `Form`, `Stack` and the dialog parts; a box is a
   `PageBox`.
2. A value none of the tokens fits goes into `design/ectomatter.tokens.json`
   first, with a `$description` saying what it is for.
3. A part none of these fits goes into `ui/system/`, drawn from the tokens,
   and every page that has a copy of it uses it. A size Rust needs to know (to
   place the part or measure it) is a `Size` or a `Part` token both read.
4. `cargo test tokens::system` fails when:
   - a page or part sets a color, a type size, a typeface, a corner or a
     shadow of its own;
   - a page or part sets a size, a place or a line's width with a number of its
     own (`sizes_are_tokens`; a `1px` that only turns a number into a length
     isn't a size);
   - a page sets a padding or a gap of its own, or draws a line to type in or
     a dropdown's chevron itself rather than using the part;
   - a menu is handed a width of its own (`menus_open_by_one_rule`);
   - Rust measures text or sets a color by hand in a page it lays out, or
     places a piece with a number of its own (a token, a part's measure or a
     named measure said once). The lab is one of those pages now.
   A drawing's own measures are the exception, each saying what it draws: the
   parts that draw a device or a glyph (`KeyBox`, `MouseLayers`, `WayGlyph`,
   `BlockGlyph`, `MacroIcon`), a line marked `// art:` (a knob's ring, a
   glyph's stroke, the mouse's picture from its mock's canvas), and, for sizes,
   an instrument's own marks between `// drawn: begin` and `// drawn: end`.

## Choices made where the copies disagreed

- The bars' sides are the page's margin. A page bar's title is
  `Space.margin` from its tabs.
- A dialog is a box; every dialog's head is one `DialogHead`; one red for
  what deletes; one form rhythm in every dialog.
- Two quiet buttons, each for one job: `LinkButton` for a box's own actions,
  `TextButton` for a toolbar's.
- One checkbox, 18 px, its words 10 px after it, 26 px its line (a sensor's
  chip's 15 px one too).
- One switch: a ground a control's height, its choices `Part.inset` in and
  `Part.seg-gap` apart, their words a note's size (the lab's 36 px switches
  and the Screen Editor's 28 px one are this one now).
- One dropdown and one field, a control's height (34; they were 36 here, 34
  there, 32 and 28 elsewhere), their words `Part.field-pad` in; on a hint line
  or an editor's foot, a button's height.
- One slider (Peak nits had a thin one of its own).
- One gap between settings in every box (it was 14, 16, 18 and 22); a box's
  first piece always at `Size.box-head`, its foot `Space.pad`.
- The game page's controls at the form's column (they were 8 px right of its
  plugin sections'); Game launch's rows level with the playlist's.
- One line a note's lines are apart (the Macro Editor's were 15.6 px).
- One heading in capitals (two sizes and three letter-spacings before).
- One rename field, one tag for a warning, one record button, one fold, one
  floating bar (Home's two had different heights and gaps).
- A menu's rows a control's height (33 before), its height worked out once.
- One width rule for every menu: they were typed at 220, 240, 250, 260, 270,
  272, 280, 290, 300, 330 and 360, or worked out with four different room
  numbers. The lab's setups' menu and lights' list are the shared `Menu` now.
- One distance a press moves before it carries (it was 3, 4 and 5).
- The pages' scrollbars run `Space.inline` short of either end, as a
  `ScrollBox`'s does (they were 2 under the bar and 5 or 2 over the foot).
- Every dialog's Cancel at the middle of its row, beside the taller primary.
- The lab's head is a dialog head's rhythm (its title on a box title's line,
  its intro `Space.tight` under it); its readout's colors are `Ink`'s.
- The Library's covers and Apps start under the head as every page's content
  does.
- Type sizes that disagreed within one role took the ramp's; corners took the
  radius of what they round.

## Not on the system yet

- The rows a plugin describes for Settings and for a game's page are two
  pieces of code (`ecto_settings.rs`'s `plugin_section`, `games_page.rs`'s
  `draw_row`), because the protocol gives them two vocabularies (Settings'
  `show`, `card`, `status`, `credit`; a game page's `grid`, `row`, `sub`,
  `fold`). They share the form's tokens and pieces, not one renderer; one
  renderer needs one vocabulary in the core protocol first.
- The lab's typefaces' list, glyphs' grid and key map are the lab's own
  instruments, drawn as cells by `lab.rs`: their buttons and rows take the
  tokens, their own anatomy is marked art, but they are not shared parts (a
  list with a font's preview on each row has no part yet).
- The drawings keep their own measures, marked: the keyboard and the mouse,
  Home's desk and its grips, the playlist's strip, the macro chain, the Screen
  Editor's screen and blocks, the layout cards, and the lab's cross-section and
  slider rows. Their controls that are controls elsewhere use the parts.
- Inside a part, its own drawing (a glyph's size, a knob's ring, a tooltip's
  padding) is the part's, marked; what Rust measures it by is a token.
- A few boxes start at a height of their own until they say how tall they are
  (the Key detail box, so the mouse's row starts that tall), marked art.
- The Rust check reads the lines that place a piece; a measure worked out in a
  `let` before it is placed is not read.
