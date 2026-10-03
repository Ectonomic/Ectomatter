# The Light lab and the light model

2026-10-03. Approved, and built in this order: the boundary, then one key's own setup.

The Light lab is Ectodeck's, not a device's. It is built to tune any peripheral's light, and the Z10 is only the first device it tunes. This sets out where the boundary runs, so the lab never carries a device and a core never carries the lab.

## 1. Who owns what

**Ectodeck, the platform.** The light model, which is the vocabulary of lit parts and how light moves through them; the engine that simulates the light and draws it in HDR (`crates/ectodeck-draw`); and the Light lab, the window that tunes it. None of them knows any one device. Each works from what a core describes.

**A core, the device's package.** Its art, its controls, and its light description: which lit parts the device has, their shapes (taken from the art) and heights, and the values that light it, each key's own included. For the Z10 as it ships: Kailh Brown switches with the LED through a slot 5.08 mm north of the stem, shine-through caps, wells 2 mm deep. A core describes its device in data and carries no interface, as the core protocol already rules.

The lab tunes a core's light description and saves it back into that core.

```
  Ectodeck (platform)                  core: evga-z10 (package)
  ───────────────────                  ────────────────────────
  light model: parts, physics    ◀──   art: art/z10.svg
  engine: simulate, draw HDR     ◀──   light description: device.json "light"
  Light lab: tunes a device      ──▶   Save writes the light description
```

## 2. The light model: parts

A device's parts are what its description has: keys with LEDs under caps, wells round them, a display, small lights behind windows. The lab shows a section for each kind present, and the engine simulates each.

| Part | What it is | Its values | Its sections in the lab |
|---|---|---|---|
| keys | A switch with its LED, under a cap | the switch's kind, beam, LED place, housing; the cap's kind, plastic, black, milkiness, heights and surfaces; one key's own of any of these, and where its switch sits | LED, Keycaps |
| body | The deck in the wells and the chassis round them | how much each gives back; the wells' depth | Wells |
| screens | A backlit display | lit dots, unlit dots, the ground between them, dot size, glass | Display |
| windows | An LED behind a diffusing window | how far its light spreads, and how much reaches the window's corners | none yet |

Windows are the Z10's lock lights today, and the logos, light bars and rings of mice and headsets later. The lab has no section for them yet, so they keep the values the core gives; one comes with the first device whose windows need tuning. Any further kind enters the model the same way, with the first core that needs it. A part's place and shape always come from the core's art, so the lab's drawings come from the device too.

## 3. The light description

The `light` section of a core's device description, `cores/<core>/device.json`, written into the core protocol as its own section. It holds:

- the art pieces its parts use (the Z10's wells are `mask_1`);
- the typical key the lab draws when no key is chosen (the Z10's 2 key);
- how many of the art's units make a millimetre (1.38 for the Z10);
- each part's values;
- under `keys`, a key's own values, by the control's stable id, so they stay on the right key across versions of the core. The Z10's file still uses its key number (`pos`) as that id.

For example, a Z10 with pudding caps on all but a stock spacebar, and keypad +'s switch in the top half of its cap:

```json
"light": {
  "sim": { "caps": "pudding", "skirts": 0.3, "black": 0.4, "milky": 1.0 },
  "keys": {
    "64": { "place": { "east": 0.0, "north": 9.5 } },
    "106": { "sim": { "caps": "shine_through" } }
  }
}
```

A key's entry holds only what differs from all keys.

## 4. How the lab follows the device

- It opens on the device whose page it is opened from (Ctrl+Shift+L), named in its title bar, and saves into that device's core.
- Its words name the device and its parts from the description. None of its text names a particular device.
- Its sections are the parts the device has.
- Its drawings take their key, wells and legends from the device's art: the typical key for all keys, or the key chosen.

## 5. What changes in the code

- **`crates/ectodeck-draw`** loses its built-in Z10 (`z10::SVG`, `z10::DEVICE`). The board and the simulation are built from any core's art and light description. The parts are types of their own, and every key's value can be its own.
- **`apps/ectodeck-native`** loads a core's art and light description by the core's id from its folder, not from a fixed path, and hands the lab a device: its name, its parts, its art and its light description. Save writes into the core it came from.
- **`cores/evga-z10`** gains its typical key and its art's millimetre in its `light` section.
- **`docs/core-protocol.md`** gains the light section, as draft 5.

## 6. Order of work

1. **The boundary.** Cores loaded by id, the engine with no built-in device, and the lab's words, sections and drawings from the device. The only visible change is in the words: they name the device from its description rather than the Z10.
2. **One key's own setup,** as in the approved mock (`apps/ectodeck/public/mock/light-lab-keys.html`): saved under `keys` in the core's light description, and lit and drawn per key by the engine.
