# Ectomatter's design system

Every page in Ectomatter is built from one set of values and one set of parts.
No page sets a gutter, a corner, a type size, a shadow or a color of its own:
it takes them from the tokens, and it lays itself out with the system's parts.
A new design uses what is here. Where nothing here fits, the new value or part
is added here first, with what it is for, and then every page that needs it
uses it.

## Where it lives

| What | Where |
|---|---|
| The tokens, one file | `design/ectomatter.tokens.json` |
| What writes them out | `crates/ectomatter-tokens` (run by `apps/ectomatter/build.rs`) |
| The window's parts | `apps/ectomatter/ui/system/*.slint` |
| The tokens in Rust | `src/tokens.rs` (`crate::tokens::space::MARGIN`, `ink::LINE`, ...) |
| Forms laid out in Rust | `src/form.rs` |
| Boxes laid out in Rust, in a row of boxes | `src/boxes.rs` with `CellBox` |
| The check that keeps it so | `src/tokens.rs`, `mod system` (`cargo test tokens::system`) |

The tokens file is in the Design Tokens Community Group's format
(designtokens.org): groups of tokens, each with a `$value`, a `$type` and a
`$description` saying what it is for. Colors are hex strings, the form Penpot
reads and writes. The build writes the file out twice: as Slint globals
(`Space`, `Radius`, `Font`, `Size`, `Part`, `Shadow`, `Ink`) and as a Rust
module of constants. Change a token in the file and every page that uses it
changes, in the window and in what Rust lays out.

## The tokens

**Space**, on a 4 px base:

| Token | px | For |
|---|---|---|
| `margin` | 24 | A page's margin, its bar's too; the gap between cards in a grid. |
| `gap` | 12 | Between boxes, side by side or one above another; under a stage. |
| `section` | 32 | Between a section's box and the next section's heading. |
| `label` | 8 | From a heading down to what it heads. |
| `pad` | 16 | A box's padding; between a form's label column and its controls; between a stacked form's fields. |
| `row` | 12 | Above and below the rule between two rows of a form. |
| `inline` | 8 | Between controls side by side. |
| `tight` | 4 | Between things that belong together (a control and its note). |

**Radius**, by the size of what it rounds: `window` 10, `box` 12 (a box, a
card, a stage, a dialog), `panel` 10 (a menu, a tile, a well, a floating bar),
`control` 8 (a button, a field, a row), `small` 6 (a quiet button, a chip, a
tooltip), `tiny` 4 (a checkbox, a badge). Something round takes half its
height. A part set close inside another (4 px or less) follows its curve: the
outer corner less the inset.

**Font**: the typefaces (`ui` Geist, `mono` JetBrains Mono), the weights
(`regular`, `medium`, `strong`) and the ramp, each size named for what it
sets: `display` 22, `title` 17, `lead` 15, `tab` 14.5, `name` 14, `text` 13.5,
`control` 13, `note` 12.5, `caption` 12, `small` 11.

**Size**, the sizes parts share: `title-bar` 36, `bar` 56, `head` 54 (the hint
line's band), `column` 240 (a page's first column), the boxes' least widths
(`library` 440, `detail` 460, `game` 340, `effects` 300, `settings` 360,
`places` 320), `sidebar` 290, `form` 860 (a form's column at most), `label` 200
(a form's label column), `control` 34, `button` 30, `dot` 7, `check` 18.

**Part**: a part's own measurements that Rust works widths out from
(`quiet-pad`, `button-pad`, `glyph`, `glyph-gap`), so a button Rust places is as
wide as the window draws it.

**Shadow**, how far a surface lifts: `lift` (a cover, a tooltip), `float` (a
menu, the picker, a popover, a floating bar, something carried), `modal` (a
dialog), and `knob` (a slider's knob on its track).

**Ink**, the colors: the window's shades darkest to lightest (`frame`,
`title`, `base`, `box`, `well`, `stage`), each control's four states
(`hover-well`, `hover-box`, `pick`, `pick-hover`), the outlines (`line`,
`edge`, `edge-hover`, `seam`, `glass`), the words (`bright`, `dim`, `soft`,
`quiet`, `placeholder`, `off`), and the signals (`warn`, `bad`, `good`,
`danger`, `record`, `close`, `media`). A part's light or dark wash is `white` or
`black` at an opacity of the part's own. Ink can be set while the app runs
(`ECTOMATTER_INK`) for a color mock.

## The parts

In `ui/system/`:

- **bars**: `TitleBar` (a window's), `PageBar` (a page's: its name, a dot or
  the way back before it, its tabs, its status, Undo, its buttons),
  `EditorHeader` (an editor window's page bar, its name renamed in place),
  `HintRow` (the line under a page's bar), `Dot`.
- **surfaces**: `PageBox` (a box), `ScrollBox` (a box that scrolls), `Stage`
  (what a device is drawn on), `Scrim`, `DialogBox` and `Confirm` (every
  dialog), `EmptyState` (what an area says while it has nothing to show),
  `TipBox` and `Tip` (every tooltip).
- **tiles**: `EffectTile`, an effect to choose, wherever effects are chosen.
- **layout**: `BoxRow`, the row of boxes on a device's page.
- **lists**: `ListBox` (a list under a heading in capitals: the Profiles
  column, the mouse's lights), `ListRow` (a row in any list down a side: the
  profiles, the lights, the macros, the screen layouts), `AnchorColumn` (the
  Profiles), `AnchorRow`, `AnchorHead`, `RailItem`.
- **controls**: `WordButton` (gray, `primary`, `danger`; `on-box` on a box),
  `TextButton` (a toolbar's quiet button), `LinkButton` (a box's quiet link),
  `BarButton`, `IconButton`, `MoreButton`, `PencilButton`, `Tab`, `CheckBox`,
  `SegSwitch`, `DropField`, `FieldBox`, `ScrollBar`, `Spinner`, and `Glyph` (the
  app's own glyphs).
- **menu**: `Menu`, `Picker`.
- **cells**: `CellView`, which draws every piece Rust lays out, and `CellBox`,
  a box of such pieces in a row of boxes.

In Rust: `form::Form` and `form::Card` (a form and its rows), `form::switch`
(every switch Rust places), `boxes::Row` (boxes laid out in Rust for a row of
boxes, each told the width the row gave it).

## How a page is put together

**Every page**: the window's `TitleBar`, the page's `PageBar`, then its
`HintRow` where it has a hint (the page's head, `Size.head` tall), then what
it holds, from `Space.margin` at either side. A page with no hint starts
`Space.margin` under its bar. Home is the desk, the devices on it to scale:
it has no bar, only its hint row over the desk (with Scenes and Arrange at
its right).

**A device's page**: the stage first, `Space.gap` under the head; under it,
`Space.gap` down, the Profiles column (`Size.column`) and beside it the tab's
boxes in a `BoxRow`. In the row each box is at least its least width and
grows from it as the others do, so every line reaches the margin; a box that
doesn't fit beside the one before starts a line under them; a line's boxes are
as tall as each other, at most the room left in the window (a box with more
scrolls); the row is at least as tall as the Profiles column, and the column
reaches the row's foot. A box whose pieces Rust lays out is a `CellBox`: it
says how wide the row made it, and Rust lays its pieces out to that width.

**A form** (Settings, a device's Settings tab, a game's page): `form::Form`.
A column at most `Size.form` wide from the margin; each section a heading over
its box; in the box rows, each with its label in a column `Size.label` wide
and its controls `Space.pad` after it, a rule between rows. A form inside a
dialog stacks each label over what it labels instead.

**A dialog**: a `Scrim` (a press on it, or Escape, is Cancel) and a
`DialogBox`, its title, the line under it, what it asks, and its buttons at
its foot's right. A question before something is deleted is a `Confirm`.

**An editor window** (the Macro Editor, the Screen Editor): its `TitleBar`,
its list down the left (`Size.sidebar`, its rows `ListRow`s), and beside it
its `EditorHeader`, what it edits, and its foot.

## Adding a design

1. Build it from the parts above, with the tokens. A page's structure comes
   from the bars, `BoxRow`, `Form` and the dialog parts; a box is a `PageBox`.
2. A value none of the tokens fits goes into `design/ectomatter.tokens.json`
   first, with a `$description` saying what it is for.
3. A part none of these fits goes into `ui/system/`, drawn from the tokens,
   and every page that has a copy of it uses it.
4. `cargo test tokens::system` fails when a page sets a color, a type size, a
   typeface, a corner or a shadow of its own, or when Rust measures text or
   sets a color by hand in a page it lays out. A device's own art (the keys,
   the mouse, the screen's ground) is the one exception: the parts that draw it
   (`KeyBox`, `MouseLayers`) and a line marked `// art:`.

## Choices made where the copies disagreed

- The bars' sides are the page's margin (they were 28 while the page under
  them was 24, so the name sat 4 px right of everything below it).
- A dialog is a box (`Ink.box`, a faint outline, `Radius.box`, padded
  `Space.margin`); the confirm dialogs had their own lighter surface and 20 px
  padding. The Screen Editor asked before deleting inside its header; it asks
  in a dialog now, as every window does.
- One red for what deletes (`Ink.danger`, white words); there were three.
- Two quiet buttons, each for one job: `LinkButton` for a box's own actions,
  `TextButton` for a toolbar's.
- One checkbox, 18 px, its words 10 px after it (there were six, 15 to 18 px).
- One switch, `Size.control` tall with 28 px buttons (there were two sizes).
- The boxes in a device's row grow to fill it on every device (the keyboard's
  stopped at fixed widths, the mouse's last box filled). The effects' pictures
  are drawn at their tiles' width, so a wider box doesn't stretch them.
- Headings in capitals (PROFILES, LIGHTS) sit level with a box's title beside
  them.
- One form: a column at most 860, a label column 200, a box padded 16, a rule
  between every row (four widths, four label columns and three rhythms
  before). Settings has the page bar every page has.
- Type sizes that disagreed within one role took the ramp's: 11.5 became 12,
  10.5 became 11, a dialog's 16 became 17, an editor's name 14 and 14.5 became
  the bar's 17, the lab's 20 became 17.
- Corners: 7 became 8, 14 became 12, 5 became 4 or 6 by what it rounds.

## Not on the system yet

- The pieces Rust lays out inside a box (an effect's settings, a plugin's
  rows, the Screen Editor's settings, the sensors' map, Custom Scripts) still
  type their own line heights, insets and piece sizes, and three of them have
  a copy of the switch. They are moving onto shared pieces next.
- The rows a plugin describes for Settings and for a game's page are two
  pieces of code (`ecto_settings.rs`'s `plugin_section`, `games_page.rs`'s
  `draw_row`), because the protocol gives them two vocabularies (Settings'
  `show`, `card`, `status`, `credit`; a game page's `grid`, `row`, `sub`,
  `fold`). They share the form's tokens and its switch, not one renderer;
  one renderer needs one vocabulary in the core protocol first.
- The four page scrollbars are four copies of one `ScrollBar` placement
  (one per page that scrolls, as Slint can't hand a part another part's
  scrolling area).
- Spacing inside a part (a button's padding, a list row's insets) is the
  part's own, set where the part is drawn. The check above covers colors,
  type, corners and shadows everywhere, and the page structure comes from
  the parts, but it doesn't measure gutters; the renders do (the gutter
  measure, every page, every width).
