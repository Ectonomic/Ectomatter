# Ectodeck core protocol

Draft 6, 2026-10-04. Draft 6 writes a device's description to be read by people as well as programs (section 4): each control is tied to the drawing by one number, its other pieces following by rule; the light's values are named as the Light lab names them, every length in millimetres; and the lock lights are described. Draft 5, 2026-10-03, added the device's light (section 4), the description Ectodeck's Light lab tunes and draws the device's light from. Draft 4, 2026-09-29, was for review before anything was built on it. Draft 3 took in the review of the M3 plugin's author. Draft 4 replaces the device picture with one drawing in named, numbered pieces (section 4, the device's art), so a core brings its own art and the frontend draws it with nothing added by hand. It adds a layout to the shared settings language (section 6, Layout), so a core arranges its settings without drawing them, and lets a core's files carry comments.

Ectodeck is one app for every device on a desk. It follows the model RetroArch made work for emulators: one frontend, and the engines that know each device brought in as **cores**. The frontend owns the windows, the tray, profiles, and every feature that spans devices. A core owns the truth about the devices it handles: how to find them, what they can do, and how to talk to them.

This document is the contract between the two.

## 1. Words

- **Frontend.** The Ectodeck app itself: the interface, and the features that work across every device.
- **Core.** A separate program that handles one family of devices, such as the EVGA Z10, the MagTran M3, SteelSeries mice or everything OpenRGB drives.
- **Device.** One physical thing a core has found and connected to.
- **Control.** One part of a device that you press, turn, slide or look at: a key, a dial, a slider, a screen.
- **Capability.** One kind of thing a device can do, such as reassigning keys, lighting or showing images. The frontend has one screen per capability, so every device that offers a capability is edited the same way.

## 2. Principles

1. **Cores are separate programs.** A core that crashes takes nothing else down, a core can be written in any language, and a core keeps its own licence. OpenRGB is GPL and Ectodeck's own deck code comes from OpenDeck, which is GPL; neither changes the frontend's licence, because nothing is linked together.
2. **The device is the truth.** When a device stores settings in its own memory, as the Z10 does, the core reads them from the device every time and the frontend never keeps a second copy that can drift. When a device keeps nothing, as the M3 does, the frontend stores its settings and hands them to the core.
3. **The frontend draws everything.** A core never draws interface. It describes what a device can do in data, and the frontend presents it with its own controls. This is what keeps the app coherent as cores are added, and it is the deliberate difference from RetroArch, where every core's options pile into one long menu.
4. **Nothing is written to a device without a reason the user can see.** A preview is shown without storing anything. A save writes, then reads back to confirm. Before a core first writes to a device's memory in a session, it hands the frontend a backup.
5. **Firmware updates are not part of this protocol.** Writing firmware can leave a device unusable, so it stays with dedicated tools.

## 3. Transport and life cycle

### A core on disk

Each core is a folder with a manifest, `core.json`. Every file of data in a core's folder is JSON, with comments allowed (`//` to the end of a line, and `/* */`), so a core's files can explain themselves; the messages between a core and the frontend are the same JSON without comments (Messages, below).

The manifest:

```json
{
  "id": "evga-z10",
  "name": "EVGA Z10 RGB",
  "version": "1.0.0",
  "protocol": 1,
  "licence": "MIT",
  "run": {
    "linux-x86_64": "bin/evga-z10",
    "windows-x86_64": "bin/evga-z10.exe",
    "macos-aarch64": "bin/evga-z10"
  },
  "matches": [{ "usb": { "vendor": "3842", "product": "2611", "usagePage": "0008", "usage": "004B" } }],
  "uses": []
}
```

`matches` lets the frontend start a core only when one of its devices is plugged in, the way RetroArch picks a core for a file. A device often shows several interfaces and only one of them is the one the core talks to (the Z10's is the LED-indicator collection, usage page 0x08; the M3's is usage page 0xFFA0, usage 1), so a match may name an `interface`, a `usagePage` and a `usage` as well as the vendor and product.

`uses` lists what a core does for itself beyond talking to its devices, so the frontend can tell you before the core first runs: `"sound-capture"` (reading what your computer is playing, which the M3's music backgrounds do) and `"network"` (the M3's weather backgrounds fetch forecasts and alerts). A core that finds its devices another way, such as the OpenRGB core, which asks the OpenRGB server, declares `{ "always": true }` instead.

### Messages

The frontend starts the core and they talk over the core's standard input and output, one JSON-RPC 2.0 message per line. Requests get a response; notifications do not.

The first exchange is `initialize`:

```json
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocol":1,"frontend":"ectodeck 1.0.0","platform":"linux-x86_64","dataDir":"/home/someone/.local/share/ectodeck/cores/evga-z10"}}
← {"jsonrpc":"2.0","id":1,"result":{"protocol":1,"name":"EVGA Z10 RGB","version":"1.0.0"}}
```

The core then reports devices as they come and go, with `device/added` and `device/removed` notifications. Unplugging a device and plugging it back in produces a remove and an add; the frontend keeps the device's place in the interface across the two.

`shutdown` asks the core to hand every device back to its stored state (ending any preview or recording) and exit. If a core exits unexpectedly, the frontend marks its devices unavailable, restarts it with a growing delay, and says so in the interface.

### Errors

Errors use JSON-RPC's error object, with a `message` written for the person using the app, because the frontend shows it to them as it is: "The keyboard refused the write to profile 3", not "status 0xC1".

## 4. Describing a device

`device/added` carries everything the frontend needs to show the device and offer its capabilities:

```json
{
  "device": "z10-1",
  "name": "EVGA Z10 RGB",
  "vendor": "EVGA",
  "model": "Z10 RGB",
  "firmware": { "kind": "custom", "version": "1.171", "build": 1, "features": [], "later": [{ "id": "sliders.read", "since": 2 }] },
  "art": "art/z10.svg",
  "controls": [
    { "id": "esc", "kind": "key", "label": "Esc", "art": 12 },
    { "id": "volume", "kind": "slider", "label": "Volume", "art": 105 }
  ],
  "screens": [
    { "id": "lcd", "label": "Display", "art": 1 }
  ],
  "layout": "layouts/z10.json",
  "capabilities": { "keys": { }, "macros": { }, "lighting": { }, "displays": { }, "profiles": { }, "settings": { } }
}
```

- **art** is the device's drawing, one SVG file shipped in the core's folder. The frontend draws the device from it and nothing else (see the device's art, below).
- **firmware** says whether the device runs its maker's firmware (`"kind": "retail"`), Ectodeck's custom firmware (`"custom"`, with its build), or neither (`"unknown"`). **features** lists what the custom build adds, such as `sliders.read`; a core offers a custom feature, and accepts its methods, only when the device's build has it, so a device on retail firmware is never sent a command it does not know. **later** names features of newer builds, for the frontend to mention. The frontend keeps custom features apart from everything else, labelled as needing Ectodeck's firmware.
- **controls** lists every part of the device a capability can refer to, and which pieces of the drawing belong to it. Controls have stable ids, so settings stay attached to the right key across versions of a core; an id is a word a person can read, such as `esc`, `f1` or `keypad-plus`, and names the control everywhere else in the description. `label` is the control's name in the interface; `short`, where a control has one, is a shorter name for where room is tight, as on the Light lab's map of the keys. What is printed on a control is in the drawing. A core may give a control fields of its own, as the Z10 core gives each key its `pos`, its place in the keyboard's own reports.
- **screens** lists the device's displays and the pieces of the drawing that show them. The displays capability refers to a screen by its id, as it does to a control. A key with its own small screen, as on a Stream Deck, is a control that also names its `screen`.
- **layout** arranges the device's settings on the frontend's pages (section 6, Layout).
- **capabilities** says which capabilities the device offers, each with its own description (section 5). A device offers only what it really has.

### The device's art

A core brings its own art, and the frontend draws it exactly as it is. Everything that makes up the look, shading, chrome, print and glass included, is drawn in the SVG, so what a core's artist draws in Inkscape or any other editor is what the app shows. The frontend adds only what changes while the app runs: each light's colour, a pressed control, the live picture on a screen, and its own interface over the drawing, such as a selection outline or the name of a key's new assignment.

**Pieces.** The drawing is broken into pieces. Each piece is a group or shape whose `id` is its kind and a number, such as `control_12`. The number only tells pieces of the same kind apart: each kind counts from 1 on its own, so a keyboard with 104 controls still has `body_1`. How finely to break the drawing up is the artist's choice, with one rule: anything that changes on its own needs a piece of its own. A key that takes its own colour needs its own illumination, a key that presses needs its own control, and a legend that lights with its key needs its own legend.

The kinds, in the order they usually sit from the bottom of the drawing to the top. The drawing itself decides: pieces are drawn in the order they come in the file, as SVG always is, and a kind can sit at more than one depth. The Z10 has light under its caps (thrown on the deck) and over them (on each cap's top, and round each legend).

| Kind | What it is | What the frontend does with it |
|---|---|---|
| `body` | The shell of the device, with its shading and chrome | Draws it as it is |
| `illumination` | The light a control throws, drawn in white at the strength it has at full light, with any blur in the drawing | Sets its colour (`fill` and `color`) to the control's, and fades it by how strongly the control is lit |
| `mask` | Where an illumination may show, such as the wells a row of keys sits in: a `<clipPath>`, which the illuminations use in the drawing (`clip-path="url(#mask_1)"`), so the drawing looks right in any editor | Nothing more; it is part of the drawing |
| `control` | A key, button, dial or slider, with its shading | Draws it as it is; nudges it down and darkens it while it is pressed |
| `legend` | What is printed, on the body or on a control, in its colour when unlit | Draws it as it is; one tied to a control takes on the control's colour as strongly as the control is lit, and moves with it when pressed |
| `screen` | Where a display sits | Shows the display's live picture inside it |
| `overlay` | Anything in front of everything else, such as the glass over a screen and its glare | Draws it as it is |

**Tying pieces together.** Each control names its piece of the drawing by number, `"art": 12`, and its other pieces follow from that number by rule: its control is `control_12`, its legend `legend_112` (100 more) and its light `illumination_12`, `illumination_212` and `illumination_412` (200 and 400 more), where the drawing has them. A key with no LED of its own says so, `"lit": false`. A screen's `"art": 1` is its `screen_1` and the `overlay_1` over it. A drawing numbered otherwise (a device with 200 controls or more, say) names a control's pieces itself: `"legend": 312`, `"illumination": [12, 212, 412]`. Masks need no tie, because the drawing applies them itself. A piece that nothing ties to, such as the body, the print on the body or an overlay, is drawn as it is and never changes.

**The file.**

- Pieces are found by their `id`. Inkscape's labels are not read, because saving as plain SVG removes them; an id is set in Inkscape's Object Properties.
- Each id is used once. A number that is no longer used is not given to a different piece, so settings saved for other pieces do not move.
- Plain SVG only: no scripts, no animation, and text converted to outlines, so the drawing looks the same wherever it is drawn. Filters such as a blur are fine.
- Positions come from the drawing, in its own units: where a control sits is where its pieces are, so the core lists no positions.
- On a screen with room above its white, the frontend draws the illuminations, and the legends they light, brighter than white. The rest of the drawing stays at ordinary white.

### The device's light

How the device's light gets out, for the frontend to draw it as light: the `light` section of the device description. The frontend simulates it, following each LED through the device's shape as a lamp's light is followed, so the section describes the device physically rather than how it should look. Ectodeck's Light lab tunes it on any device and saves it back here, each value it changes in its place, so the file's comments and lines stay as the core's author wrote them. The lab is the frontend's: it knows no device, and a core carries none of it.

```jsonc
"light": {
  "scale": 1.38,       // the drawing's units to the millimetre
  "typical": "2",      // the key the Light lab draws for all keys
  "switches": { "kind": "led-slot", "top": 5.7, "width": 12, "led": 5.08, "lumens": 0.745, "beam": 3 },
  "caps": { "kind": "shine-through", "foot": 5, "top": 14, "inside": 0.5, "outside": 0.18 },
  "legends": { "opacity": 0.7, "unlit": 0.15 },
  "wells": { "art": "mask_1", "depth": 2, "deck": 0.22, "chassis": 0.4 },
  "display": { "lit": 324, "unlit": 4.1, "background": 0.8, "dot": 0.84, "glass": 1 },
  "keys": {
    "keypad-plus": { "place": { "east": 0, "north": 9.5 } },
    "space": { "caps": { "kind": "shine-through" } }
  }
}
```

- **scale** is how many of the drawing's units make a millimetre. Positions come from the drawing in its own units; every length in the description is in millimetres, every brightness in nits and the LEDs' light in lumens. Every surface is as bright as the light that reaches it makes it, from the LEDs' lumens: nothing is scaled to fit.
- **typical** is the control the Light lab draws in section when no key is chosen, by its id: a key as most of the device's are.
- **switches** is the switch under each key: its housing's `kind` (`led-slot`, black with its LED through a slot in its top; `clear-top`; or `clear`), its `top` above the deck and its `width`, where its LED sits (`led`, north of the switch's middle, south below 0), the light it gives off at full white (`lumens`), how tight the LED's `beam` is, and how a clear housing glows (`scattered`, `sides`, `glow`).
- **caps** is the cap over it: its `kind` (`shine-through`, `pudding` or `translucent`), its skirt's `foot` and its `top` above the deck, how opaque its walls' plastic is (`opacity`, 0 letting the light inside through, 1 opaque; a translucent cap's top too), how far down a pudding cap's sides its `black` comes, how `milky` the plastic is (its haze: the share of the light it takes in that it scatters and carries round the cap, 0 clear to 1 milky white; the rest goes straight through), and the shares of the light its `inside` and `outside` reflect.
- **legends**, **wells** and **display** are the legends' `opacity` (how much of the light behind them they hold back, 0 letting it all through, 1 as dark as the cap's black) and how a legend looks with its LED off (`unlit`, a share of the drawing's grey); the wells (the mask piece of the drawing that holds them, their `depth`, and the shares of the light the `deck` and the `chassis` reflect); and the display's backlit dots, in nits, with how much of its cell a dot fills and how strongly the glass shows its sheen.
- Each value is one of the Light lab's sliders or choices, named for it: `inside` is the lab's Inside reflection, and `milky` its Haze. The Z10's file says what each one is in a comment beside it.
- **keys** holds a key's own values, where they differ from all keys', by the control's id. An entry takes any of the sections above that belong to a key, `switches`, `caps` and `legends`, and only what differs: a stock cap among pudding caps, say. It can also say where the key's switch sits under its cap: **place**, how far east and north of the cap's middle, as on a long key whose switch is off its middle. The LED goes with its switch. A key with nothing of its own has no entry.
- A button with no light of its own stands in the keys' light too: the drawing shows it as a block (its control's first piece, with no shadow drawn under it as a cap has; its face the next piece where that lies inside it). Its entry in **keys** says where it stands and how high, its caps' `foot` and `top`, as the Z10's G/PC and media keys stand on its chassis.

The lock lights, small windows with an LED behind each, are the description's `lock_lights`: the piece of the drawing for each of `num`, `caps` and `scroll`, their `colour` and `brightness`, how far from the LED a window stays bright (`spread`), how lit its corners stay (`floor`), and the little light out of its edges (`spill`, its brightness and how far it reaches).

```jsonc
"lock_lights": {
  "num": "num-lock-light", "caps": "caps-lock-light", "scroll": "scroll-lock-light",
  "colour": "#ff6000",
  "brightness": 300,
  "spread": 1.45,
  "floor": 0.5,
  "spill": { "brightness": 6, "reach": 0.72 }
}
```

What a device has decides what the Light lab tunes: keys with LEDs under caps, wells round them, a display, small lights behind windows. Anything the description leaves out is the frontend's ordinary default.

## 5. Capabilities

Each capability has a description, sent in `device/added`, and methods named after it. Where a device keeps settings in its own memory, the description says `"stored": "device"`; where the frontend has to keep them, it says `"stored": "host"`.

### keys: what each control does

```json
"keys": {
  "stored": "device",
  "layers": [{ "id": "base", "label": "When pressed" }, { "id": "fn", "label": "When Fn is held" }],
  "actions": ["key", "shortcut", "media", "mouse", "macro", "profile", "fn", "off"],
  "factory": true
}
```

- `keys/get {device, profile}` returns every control's assignment in every layer.
- `keys/set {device, profile, layer, control, action}` changes one; `keys/reset` returns one to its factory assignment where `factory` is true.

An **action** is one of a fixed set the frontend knows how to present: a key (`{"key": "F13"}`), a shortcut (`{"shortcut": ["LeftCtrl", "C"]}`), a media key, a mouse button, a stored macro, a profile switch, the Fn layer, off, or `{"host": ...}`, which tells the frontend to run one of its own actions when the control is pressed (section 7). Key names are the USB HID usage names, so every core means the same thing by `F13`.

### macros: recorded sequences

```json
"macros": {
  "stored": "device",
  "slots": 100,
  "budget": { "bytes": 967, "cost": { "key": 4, "media": 6 } },
  "steps": ["key", "media", "wait"],
  "plays": ["once", "times", "while-held", "until-pressed-again"],
  "record": true
}
```

- `macros/list`, `macros/get`, `macros/set`, `macros/delete`.
- Steps are one shared shape across cores: `{"key": "LeftCtrl", "down": true, "after": 120}` (`after` is the wait before the next step, in milliseconds), `{"media": "Mute", "down": true, "after": 0}`.
- `budget` lets the frontend show how much room a macro has left before the device refuses it.
- `record` means the core can record from the device itself. `macros/record/start` takes the device's input for the recording alone (so its keys reach nothing else), `macros/record/status` reports progress, and `macros/record/stop` returns the steps. A device without `record` can still have macros; the frontend records them from the system's keyboard.
- A device with `"stored": "host"` gets its macros played by the frontend when its controls send input.

### lighting: effects, previews and live colour

```json
"lighting": {
  "stored": "device",
  "effects": [
    { "id": "wave", "label": "Wave", "params": {
        "colors": { "type": "colors", "max": 7 },
        "speed": { "type": "duration", "min": 200, "max": 20000, "label": "Speed" },
        "direction": { "type": "choice", "options": ["up", "down", "left", "right"] },
        "zone": { "type": "choice", "options": ["all", "fkeys", "numbers", "wasd", "arrows", "numpad"] } } }
  ],
  "preview": true,
  "direct": { "controls": 116, "rate": 25 }
}
```

- Effects are the device's own built-in animations, each with parameters in the shared settings language (section 6), so the frontend builds every effect's editor from data.
- `lighting/get` and `lighting/set` read and write the stored lighting of a profile.
- `preview` means the device can show an effect without storing it. `lighting/preview {device, effect, params}` starts or updates a preview, and `lighting/preview/stop` hands the lighting back to the stored profile. The core runs whatever loop the device needs (the Z10 needs a frame every 40 ms) and stops by itself if the frontend stops refreshing.
- `direct` means the frontend can set every lit control's colour itself, up to `rate` times a second, with `lighting/direct {device, colors}`. This is what makes lighting that spans the whole desk possible: the frontend draws one effect across every device and streams each its part.

### displays: screens and image keys

```json
"displays": [
  { "control": "lcd", "kind": "text", "cols": 20, "rows": 4, "charset": "cp1252" }
]
```

A deck whose keys each have their own small screen offers one display per key: `{"control": "k0", "kind": "image", "width": 72, "height": 72}`. A deck like the M3, whose keys are windows onto one screen behind them, offers that screen: `{"control": "panel", "kind": "image", "width": 854, "height": 480, "windows": [...]}`, where `windows` is the geometry Ectodeck already uses to place each key's image and to line a background up behind the keys.

A core draws its own displays. The M3 is a whole screen with magnetic keys over it, and its core composes that screen (key images, labels, animated backgrounds) the way Ectodeck's M3 plugin does today. The frontend asks for what it needs to show you:

- **Live feed.** `display/feed/start {device, control, rate}` starts a feed of what the display is showing right now, at no more than `rate` frames a second (the frontend asks for about 30; the app needs no more), until `display/feed/stop`. The frontend shows it in the device's picture, so the screen on your desk and the one in the app match.
- **Preview.** Nearly everything on a screen like the M3's moves, so one still frame would misrepresent it. A preview is a short feed of its own: `display/preview/start {device, control, content, rate}` draws how some content would look, such as a new background, without sending it to the device, until `display/preview/stop`. A preview running beside the live screen roughly doubles the core's drawing work, so the frontend runs one only while it is on screen.
- **Content.** `display/show {device, control, content}` applies it. Content is what to show, such as text, an image, a key's label and icon, or a background by name.

**Frames travel on their own channel.** Text displays send their rows in the feed's notifications. Image frames are too large for the message line (60 frames a second of JPEG is several megabytes a second), so `display/feed/start` and `display/preview/start` answer with a frame channel: a local socket (a named pipe on Windows) on which each frame arrives as a small header (`type`, `width`, `height`, `length`) and then its bytes. A frame is whatever the core already makes, JPEG or PNG, as its header says; the M3 core already compresses every frame it sends the deck as a JPEG, so the feed costs it nothing extra.

**What a display can show is a catalogue.** An image display lists the content it accepts in its description:

```json
{ "control": "panel", "kind": "image", "width": 854, "height": 480, "windows": [...],
  "content": {
    "backgrounds": [
      { "id": "aquarium", "label": "Aquarium", "group": "Scenes", "thumbnail": "thumbs/aquarium.jpg",
        "settings": { "fish": { "type": "int", "min": 1, "max": 30, "label": "Fish" } } },
      { "id": "weather", "label": "Weather", "group": "Scenes", "thumbnail": "thumbs/weather.jpg",
        "settings": { "place": { "type": "place", "label": "Place" }, "intensity": { "type": "number", "min": 0, "max": 1, "step": 0.05 } } }
    ],
    "keys": { "image": true, "label": true }
  } }
```

Each background carries its own settings in the shared settings language, and the catalogue can change: when you add a shader file of your own, the core sends `display/catalog/changed` and the frontend reads the list again.

### input: presses, turns and slides

Notifications from the core whenever a control is used: `input/down`, `input/up`, `input/turn {ticks}`, `input/slide {value}`, `input/touch {x, y}`. Devices run by the host, such as the M3, depend on these entirely. Devices with their own memory, such as the Z10, send them only for controls assigned to `{"host": ...}`, so a keyboard key can trigger a frontend action without the keyboard losing its own assignments.

A core acts on its own input first and tells the frontend second. A press has to show on the device within a frame (the M3's backgrounds answer every press with a ripple, a splash or a fish feeding), and a round trip through the frontend would be too slow for that.

`input/simulate {device, control, kind}` goes the other way: the frontend presses a control as if it had been pressed on the device, which is what happens when you click a key in the app's picture of the device.

### profiles

```json
"profiles": { "stored": "device", "count": 5, "factory": true }
```

- `profiles/list` and `profiles/active` read them; `profiles/activate` switches.
- For devices with `"stored": "host"`, the frontend keeps profiles itself and hands the core what to apply.
- Either way, the frontend can switch profiles when an app or game comes to the front, for every device at once.

### settings: everything else a device has

Device settings that belong to no capability, such as the Z10's game-mode keys or its display's backlight colour, described in the shared settings language (section 6): `settings/get` and `settings/set`.

### status

`device/status` notifications carry anything that changes: connection, battery, a warning such as "Firmware 1.171 is not one EVGA released", or an error the frontend should show.

### backup

For a device with its own memory, `backup/export` returns everything the device holds as opaque files, which the frontend saves before the first write of a session, and `backup/import` writes them back. The frontend never needs to understand a backup to keep it.

## 6. The shared settings language

Every parameter or setting a core offers is described with one of a small set of types, so the frontend draws it with its own standard control and every core looks like part of the same app:

| Type | Drawn as | Example |
|---|---|---|
| `bool` | a switch | Game mode disables the Windows key |
| `int` | a slider or number, with `min`, `max`, `step`, `unit` | Brightness, 0 to 100 percent |
| `number` | a slider for fractions, with `min`, `max`, `step` | A background's intensity, 0 to 1 |
| `duration` | a slider in seconds, stored in milliseconds | Wave speed |
| `choice` | a segmented control or menu, with `options` | Wave direction |
| `color` | a colour swatch | Static colour |
| `colors` | a row of swatches, up to `max` | Wave colours |
| `text` | a text field, with `max` length | Profile name |
| `place` | a search for a place by name, stored as its name with its latitude and longitude | The city a weather background shows |
| `image` | a picture you choose | A background photo |

A core cannot send its own interface. If a device needs something this language cannot describe, the language grows in a new protocol version, so every core gains the new control at once.

### Layout

The types above say what a setting is. A core's layout says where each one goes, so a device's settings read in a sensible order without the core drawing anything. The pages stay the frontend's: Keys, Lighting, Macros, Screen and Profiles look the same for every device, and a layout only arranges what goes inside them, plus the device's own settings on its Device page.

The device description carries the layout, written out or as the name of a file in the core's folder (`"layout": "layouts/z10.json"`):

```jsonc
"layout": {
  "lighting": {
    "wave": [
      { "section": "Colours", "show": ["colors"] },
      { "section": "Motion", "show": [{ "row": ["speed", "direction"] }, "zone"] }
    ]
  },
  "device": [
    { "section": "Game mode",
      "show": ["game_mode", { "id": "disable_windows_key", "when": { "game_mode": true } }] },
    { "section": "Display", "show": ["backlight"], "when": { "feature": "display" } }
  ]
}
```

- A layout has one entry per page, named after the capability it arranges; `device` arranges the settings capability. Lighting is arranged per effect, by the effect's id.
- A page is a list of **sections**, drawn in order, each with a heading and the settings it `show`s, by their ids.
- An item in `show` is a setting's id; `{"row": [...]}` for settings drawn side by side; or `{"id": ..., "when": ...}` for a setting shown only sometimes.
- **when** is a condition, on a setting or a whole section: every setting it names must have the value given, or one of the values in a list (`{"effect": ["wave", "ripple"]}`). `{"feature": ...}` holds when the device's firmware has that feature (section 4). A setting hidden by its condition keeps its value.
- A setting the layout does not place is drawn after the placed ones on its page, in the order the core listed it, so a missing or out-of-date layout never hides anything. A layout is optional.

The layout is the whole of it: headings, order, rows and conditions. Sizes, colours, spacing and styles stay the frontend's, so every core looks like the rest of the app.

## 7. What the frontend adds across every device

These are why the platform is more than a set of separate apps:

- **Profiles that follow the app in front,** switching every device together.
- **One Macro Editor,** for devices that store macros and for devices whose macros the frontend plays.
- **Lighting across the whole desk,** drawn once by the frontend and streamed to every device that offers `direct`.
- **Live pictures of every display,** from each core's feed, with previews before anything is applied.
- **Content for any display:** system stats, clocks, images, key labels.
- **Actions.** A control assigned to `{"host": ...}` runs an action: open an app, send a shortcut, control media, or run a Stream Deck plugin's action. The Stream Deck plugin host is OpenDeck's code, which is GPL, so it runs as its own service beside the frontend rather than inside it.
- **Backups** before any write, for every device with its own memory.

## 8. How the first cores fit

| | EVGA Z10 | MagTran M3 | SteelSeries | OpenRGB |
|---|---|---|---|---|
| Stored where | on the device | on the host | mostly on the device (mice), some host | host |
| keys | 116 keys, two layers | 15 keys and 3 dials, host actions | mouse buttons | none |
| macros | 100 slots, recorded on the device | played by the frontend | varies by model | none |
| lighting | 8 effects, preview, direct at 25 per second | none | effects and direct | effects and direct, for hundreds of devices |
| displays | 20 by 4 text | one 854 by 480 screen behind the keys, seen through each key | OLED on Apex keyboards | none |
| input | host-assigned keys, the volume slider | every key and dial | host-assigned buttons | none |
| profiles | 5 on the device | the frontend's | on the device (some mice) | the frontend's |
| Built from | the Rust core already written | Ectodeck's current device code | rivalcfg, OpenRGB and new reverse-engineering | the OpenRGB server's network protocol |

The Z10 and the M3 sit at opposite ends: one keeps everything on the device, the other keeps nothing. If both fit this protocol without special cases, it will hold for most of what is in between.

The M3 as its core will describe it: 15 keys in three rows of five; three dials down the right edge that turn and press, with no screens of their own; no touch; one 854 by 480 screen seen through a 110-pixel window over each key; nothing stored on the device, so the core sends everything again on reconnect, with a watchdog that reconnects within 15 seconds.

**Never offered.** Some commands a device accepts do harm, and a core must not offer them as settings, however tempting:

- **MagTran M3: key sensitivity and actuation.** Its vendor commands for these (QUCMD and SENS) make keys press on their own and miss presses until the deck is unplugged. The deck core offers no key sensitivity at all.
- **EVGA Z10: the firmware updater.** Entering update mode cannot be undone without a complete write, so the Z10 core has no path to it (section 2, principle 5).

## 9. Versions

`protocol` is one whole number. A frontend supports a range of versions and says which it speaks in `initialize`; a core that cannot speak it declines. Within a version, new optional fields may appear and must be ignored by anyone who does not know them.

## 10. Open questions

Settled in review: recording on Windows uses the system's keyboard hook, as Unleash does, and holds keys back from other apps while recording (only Ctrl+Alt+Del and Windows+L cannot be held); cores draw their own displays and the frontend gets live feeds and previews (section 5, displays); cores will be offered from a catalogue later, signed so a core's source is known before it runs.

1. **System permissions.** Each system asks once before an app may do certain things: on macOS, Input Monitoring (to read a keyboard's presses, which recording needs) and Accessibility (to send keystrokes, which shortcut actions need); on Linux, a rule installed once with admin rights so the app can reach the devices without running as administrator; on Windows, usually nothing. A core reports that it needs one (`device/status` with `"needs": "input-monitoring"`), and the frontend explains the step in plain words at the moment it is needed. What a core does for itself, such as capturing sound or using the network, is declared in its manifest's `uses` instead (section 3). The exact list grows as cores find what they need.
