# The lab and the light model

2026-10-03. Approved, and built in this order: the boundary, then one key's own setup. 2026-10-04: the light description written to be read, its values named as the lab names them (the core protocol's draft 6). 2026-10-06: custom setups beside Default, the core's own setup, kept in Ectomatter's own data (section 4). 2026-10-07: a key's legend typed, keyboard glyphs included, and moved anywhere on its key; the caps' colors; each part of a legend sized and placed on its own, and a section's Reset (section 2).

The lab is Ectomatter's, not a device's, and takes the name of the device it tunes: the Keyboard lab, the Mouse lab. It is built to tune any peripheral's light, and the Z10 is only the first device it tunes. This sets out where the boundary runs, so the lab never carries a device and a core never carries the lab.

## 1. Who owns what

**Ectomatter, the platform.** The light model, which is the vocabulary of lit parts and how light moves through them; the engine that simulates the light and draws it in HDR (`crates/ectomatter-draw`); and the lab, the window that tunes it. None of them knows any one device. Each works from what a core describes.

**A core, the device's package.** Its art, its controls, and its light description: which lit parts the device has, their shapes (taken from the art) and heights, and the values that light it, each key's own included. For the Z10 as it ships: Kailh Brown switches with the LED through a slot 5.08 mm north of the stem, shine-through caps, wells 2 mm deep. A core describes its device in data and carries no interface, as the core protocol already rules.

**The person, who uses the device.** Custom setups of their own, kept in Ectomatter's own data: the device as they have made it, with pudding caps on it, say. Each holds only what it changes from the core's light description, so what it leaves alone follows the core.

The lab tunes a core's light description and saves it back into that core, or tunes one of the person's own setups and saves that.

```
  Ectomatter (platform)                  core: evga-z10 (package)
  ───────────────────                  ────────────────────────
  light model: parts, physics    ◀──   art: art/z10.svg
  engine: simulate, draw HDR     ◀──   light description: device.json "light"
  Lab: tunes a device      ──▶   Save to the core writes the light description
           │
           └──▶  the person's own setups, in Ectomatter's data: light/evga-z10/<setup>.json
```

## 2. The light model: parts

A device's parts are what its description has: keys with LEDs under caps, wells round them, a display, small lights behind windows. The lab shows a section for each kind present, and the engine simulates each.

| Part | What it is | Its values | Its sections in the lab |
|---|---|---|---|
| keys | A switch with its LED, under a cap | the switch's kind, beam, LED place, housing; the cap's kind, plastic and its colors (its top's and its sides'), how far down the top's color comes, milkiness, heights and surfaces; one key's own of any of these, and where its switch sits | LEDs, Keycaps |
| legends | What each key's cap has printed on it, which its light comes through | the typeface and its weight, where on the cap it starts and how far it is moved from there, how tall its letters and its words are; one key's own of any of these, and its own words | Legends |
| buttons | A moulded block with no light of its own, standing in the keys' light | how high it stands, and what its outside gives back | Keycaps, for one button |
| body | The deck in the wells and the chassis round them | how much each gives back; the wells' depth | Wells |
| screens | A backlit display | lit dots, unlit dots, the ground between them, dot size, glass | Display |
| windows | An LED behind a diffusing window | how far its light spreads, and how much reaches the window's corners | none yet |

A key's legend can be typed: its own legend, its shifted sign and its second function, each in place of what the core's print has, with a list of 48 keyboard glyphs to add (⌘ ⇧ ⎋, arrows of three kinds, the triangles many arrow keys print, the volume keys' speakers, and the like), and the keyboard's own icons, such as the Z10's mute, which a box shows as [mute]. Ectomatter carries its own fonts for those glyphs and the rest of their Unicode blocks (`crates/ectomatter-draw/fonts`, cut from DejaVu Sans Bold, Noto Sans Symbols and Noto Sans Symbols 2), so they show on any computer, in the lab and on the keys; where the computer has fonts of its own for them, the lab's text uses those first.

Legends are set only in typefaces Ectomatter carries, so a setup looks the same on any computer. Its own library (`crates/ectomatter-draw/typefaces`, each under the SIL Open Font License or MIT) holds the faces keyboards are printed in: Orbitron (the Z10's default, the open look-alike of its own Conthrax, which may not be shipped in an app), Bruno Ace, Audiowide, Michroma and Saira (the squared, Eurostile looks of gaming keyboards), Rajdhani, Barlow (DIN), Arimo (Helvetica and Arial), Inter (laptops' clean modern look), Nunito (VAG Rounded), Jost (Futura), Open Gorton (the engraved classic), JetBrains Mono, Courier Prime (the typewriter), and Noto Sans JP and KR for Japanese and Korean keyboards' kana, jamo and key words, which also fill in for a typeface that lacks them, as Arimo does Greek and Cyrillic. A variable typeface offers every weight it spans. "Add a typeface…" at the foot of the list copies a .ttf or .otf file into Ectomatter's data (`typefaces/`), for a keyboard's own face, such as Conthrax, that Ectomatter may not carry. The typefaces installed on the computer are not listed; one a setup already names is copied in once when Ectomatter starts. A legend moves anywhere on its key, from where its place sets it: by the Left–right and Up–down sliders, or by dragging it on the drawing. Each part of it (Main, Shifted, Bottom) also has a size and a place of its own, chosen in the Legends section's Adjust row: a keypad's arrows smaller than its numbers, or a period raised to the top of its cap.

A key's legend can also be a picture of the person's own: "Choose a picture…" under the glyph list opens the desktop's file picker (through the desktop portal) for an SVG file, and the picture takes the place of that part's text, lit as a legend is, in one color. Ectomatter keeps a clean copy beside the setups (`light/<core>/images/`, named in the setup by its file's name), so the key keeps its picture if the file moves.

A key's homing bump, the raised bar or dot that lets a typist find their place by touch, is the Keycaps section's to choose for one key: None, Bar or Dot. Each key starts with the bump the core's drawing gives it (the Z10's F and J a bar, its keypad 5 a dot); the engine takes the drawing's bump away cleanly and draws one as the drawing does on any cap, in the cap's colors.

Each section has a Reset button while a setup of the person's own is tuned: it puts the section's settings back as Default has them (keys with settings of their own keep them), or, with one key chosen, takes out that key's own settings in the section. Default itself has none; Revert gives back its last save.

A cap's colors are its plastic's, its top's and its sides', any color, for all keys or one key: a white top among black caps, say. The engine lights them as plastic, not paint: a pale opaque top lets a little of the key's light through round its legend, tinted by its color; light through colored clear plastic takes its color; and what is printed on a pale top reads in dark ink, the art's own print and Ectomatter's labels for a key's new action alike. With no colors given, the caps are the art's black, exactly as before.

Windows are the Z10's lock lights today, and the logos, light bars and rings of mice and headsets later. The lab has no section for them yet, so they keep the values the core gives; one comes with the first device whose windows need tuning. Any further kind enters the model the same way, with the first core that needs it. A part's place and shape always come from the core's art, so the lab's drawings come from the device too.

## 3. The light description

The `light` section of a core's device description, `cores/<core>/device.json` (JSON with comments), written into the core protocol as its own section. It holds:

- how many of the art's units make a millimeter (`scale`, 1.38 for the Z10);
- the typical key the lab draws when no key is chosen (the Z10's 2 key);
- each part's values, in sections named as the lab's are (`switches`, `caps`, `legends`, `wells`, `display`), each value named for its slider or choice (`milky` is the lab's Haze), every length in millimeters;
- the art pieces its parts use (the Z10's wells are `mask_1`);
- under `keys`, a key's own values, by the control's stable id (`esc`, `space`, `keypad-plus`), so they stay on the right key across versions of the core.

For example, a Z10 with pudding caps on all but a stock spacebar, and keypad +'s switch in the top half of its cap:

```jsonc
"light": {
  "caps": { "kind": "pudding", "opacity": 0.7, "black": 0.4, "milky": 1 },
  "keys": {
    "keypad-plus": { "place": { "east": 0, "north": 9.5 } },
    "space": { "caps": { "kind": "shine-through" } }
  }
}
```

A key's entry holds only what differs from all keys. Save writes each value the lab changed in its place in the file, adds what is new and takes out what was given back; the file's comments and lines stay as they were.

## 4. Custom setups

A custom setup is the device as the person has it where it differs from the core's description: pudding caps on a Z10 whose core describes shine-through caps. The lab's head has a button for the setup it tunes, first in its row (the setup, the keys, the section); its menu lists Default (the core's own setup) and, under Custom, the person's custom setups, and makes, renames and deletes them. Ectomatter lights the device by the setup chosen, and keeps the choice in its settings (`devices.z10.light`, the setup's id; none is the core's), so it starts lit by it.

Each setup is a file in Ectomatter's own data, `light/<core>/<id>.json` (JSON with comments), holding its name and what its light changes from the core's, as a JSON merge patch (RFC 7386): a value takes the place of the core's, an object is merged into the core's, null takes the core's out. The lab writes it to be read: a head saying what the file is and how to edit it, and each value on a line of its own with its name and its value in the lab beside it.

```jsonc
{
  "name": "Pudding caps",  // the name shown in the setup menu
  "light": {               // the settings Pudding caps changes from Default
    "caps": {
      "kind": "pudding",  // Keycaps: Pudding
      "opacity": 0.8,     // Keycaps › Wall opacity: 80%
      "black": 0.4,       // Keycaps › Black down the sides: 40%
      "milky": 0.8        // Keycaps › Haze: 80%
    }
  }
}
```

- **Only what it changes,** so what it leaves alone follows the core as the core is tuned: the Z10's LEDs are the same LEDs under any caps.
- **And what only its kinds use.** A value its kind of switch or cap uses that the core's kind does not (a pudding cap's Haze, where the core's caps are shine-through) is the setup's own even where it equals the core's, since the core holds it unused and may change it with no thought of the setup.
- **Save follows the setup in use,** "Save to Default" (into the core) or "Save to" the custom setup's name. Save as a new setup keeps the light as it is, changes not saved and all, under a new name; the setup it came from stays as saved. Switching setups waits until changes are saved or reverted.
- **For all keys in a setup of the person's own, a dot** marks a value of its own, where it differs from the core's, as one key's dot marks the key's own; the dot gives the core's value back.

## 5. How the lab follows the device

- It opens on the device whose page it is opened from (Ctrl+Shift+K), named in its title bar, and on the setup lighting it, and saves into that setup: the device's core, or one of the person's own.
- Its words name the device and its parts from the description. None of its text names a particular device.
- Its sections are the parts the device has.
- Its drawings take their key, wells and legends from the device's art: the typical key for all keys, or the key chosen.

## 6. What changes in the code

- **`crates/ectomatter-draw`** loses its built-in Z10 (`z10::SVG`, `z10::DEVICE`). The board and the simulation are built from any core's art and light description. The parts are types of their own, and every key's value can be its own.
- **`apps/ectomatter`** loads a core's art and light description by the core's id from its folder, not from a fixed path, and hands the lab a device: its name, its parts, its art and its light description. Save writes into the core it came from.
- **`cores/evga-z10`** gains its typical key and its art's millimeter in its `light` section.
- **`docs/core-protocol.md`** gains the light section, as draft 5.

## 7. Order of work

1. **The boundary.** Cores loaded by id, the engine with no built-in device, and the lab's words, sections and drawings from the device. The only visible change is in the words: they name the device from its description rather than the Z10.
2. **One key's own setup,** as in the approved mock (`apps/ectomatter/public/mock/light-lab-keys.html`): saved under `keys` in the core's light description, and lit and drawn per key by the engine.
3. **Setups of the person's own,** as in the approved mock (`apps/ectomatter/public/mock/light-lab-setups.html`): the setup button and its menu, the setups' files, and the keyboard lit by the setup in use.
4. **A key's legend typed and moved,** as in the approved mock (`apps/ectomatter/public/mock/light-lab-legend-text.html`): a box for each part of the chosen key's legend with the glyph list, the glyph fonts built in, and Left–right and Up–down in place of Inset, on every key and with every placement.
