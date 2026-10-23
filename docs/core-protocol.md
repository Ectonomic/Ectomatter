# Ectomatter core protocol

Draft 17, 2026-10-22, protocol 4. Draft 17 adds plugins (section 11): packages that add a section to each game's and app's page in the Library or bring in what another program drives, as OpenRGB's brings in its lighting, packaged and run as a core is but kept apart from the cores, in Ectomatter's source, its data folder and Settings. It adds the manifest's `kind`, `description`, `glyph`, `repository`, `adds` and `systems`, and `uses` for the monitors, an address on the network, web hosts, files to read and NVIDIA's driver (section 3); a device's `sub` (section 4); `profiles/restore` for a device that can't tell which profile it's on (section 5); the system interface's `monitors` and the `net` interface (`wit/core.wit`); the calls `game/describe` and `game/changes`, and the layout's `grid` (section 6); a plugin's own Settings section and its readings, which it tells as they change (section 11); Custom Scripts, the person's own commands, which Ectomatter runs itself (section 7); and the record Ectomatter keeps of every change a plugin asks for and every command it runs, put back as the game quits or the next time Ectomatter starts after it stopped short. Every addition is optional, so no core written to draft 16 needs changing. Draft 16, 2026-10-21, protocol 3. Draft 16 lets a core give each control a `group`, the heading the frontend lists it under among the device's controls (section 4); it is optional. Draft 15, 2026-10-21. Draft 15 lets a core describe everything a keyboard's Lighting page shows, so the frontend draws it from the description and has no page of its own for any device: where a profile a device writes whole keeps a light's effect and each effect's settings (the lighting's `kept`) and where each setting is within them (`at`, `also`, `item` and `count`, `fields`, `form`, section 6); a light's zones and the `zones` type; a level shown as a percent with its slider in the effect's first color (`percent`, `tint`); a speed by any name (`speed`); a choice's options with words and an icon of their own, and a choice drawn as a menu (`menu`, `rule`, `other`); an effect that resets to the factory's, plays on key presses or is painted key by key (`reset`, `reactive`, `paint`) and a note naming the profile (`{profile}`, section 5, lighting); and a layout section in a box of its own for where and when an effect plays (`box`, section 6, Layout). Every addition is optional. Draft 14, 2026-10-21. Draft 14 says how `known` is given for a device that cannot tell back what it is set to: once for all of a device's controls when it takes them in one report, for each light, and for each setting; and that the frontend never writes a value that isn't known on its own (section 5, keys). A core that gave one word for all its lights is read as before. Draft 13, 2026-10-21. Draft 13 moves the last of what the frontend did beside the Z10's core into it: the core hears the keyboard's own key reports and tells each key on a host slot, and while the frontend asks (`input/watch`), every press, each by what it `sends` (section 5, input); it tells `device/added` and `device/removed` as the keyboard comes and goes; and the display's frames go through it, with `displays/hand-back` to leave the screen to the keyboard's own screens, sent between a long call's exchanges like a preview (section 5, displays). Every addition is optional, so no core written to draft 12 needs changing. Draft 12, 2026-10-20. Draft 12 lets a core say all the frontend needs to draw a device's lighting and settings from its description alone, every addition optional: the device's `kind` (section 4), an effect's `note` and `start` (section 5, lighting), and a setting's `default` and `short` and a section's `note` (section 6). It also describes lights that are windows of their own, a mouse's logo and wheel, so the frontend lights them as it lights keys, and lets a core keep its light in a file of its own (section 4, the device's light). Draft 11, 2026-10-20. Draft 11 puts every core on the same calls and shapes, so each of Ectomatter's features is built once for every device that offers it. The Z10's core now answers the protocol's calls (section 5), not the old editor's own: `device/read` for everything a device's page shows at once, `profiles/get` and `profiles/set` for a device that writes a whole profile at a time, `keys/get` with every layer and the keys as the device was made, `macros/set` and `macros/delete`, `lighting/preview`, `lighting/direct`, `lighting/preview/stop` and `lighting/hand-back`, `settings/set`, `input/state` for where sliders stand, and the text display's `displays/show`, `displays/light`, `displays/apps`, `displays/time` and `displays/time/clear`. Actions gain `none`, `own` (a device's own action, listed with its label), `host` (one of a device's host slots, a job the frontend does) and `raw` (the device's own code, kept so nothing a device stores is lost); a key in a macro's step is named as a key is in an action, and each step's wait is `after`. The names of keys, modifiers and media keys every core shares are in the crate `ectomatter-protocol`. Draft 10, 2026-10-20. Draft 10 adds what the first mouse needed (SteelSeries' core, the Sensei 310): a device's separate lights, each with its own effect (section 5, lighting); a button's jobs of switching sensitivity and scrolling (section 5, keys); `readable`, for a device that keeps settings it cannot tell back, so the frontend knows which values came from the device and which only from the core's record; and what each light `plays`, for the frontend to show an effect as the device runs it. It also adds what Home's desk needs of every core: a drawing's `scale`, for each device at its real size (section 4, the device's art), and the manifest's `devices`, for the frontend to name them before any is plugged in (section 3). Draft 9, 2026-10-19. Draft 9 runs each core inside the frontend as a WebAssembly component: one module for every system, reaching only the devices its manifest names, through connections the frontend lends it (section 3, and the interface in `wit/core.wit`). The messages are the same; they pass as calls into the module instead of lines over its input and output. Draft 8, 2026-10-07, lets the device's print name one of the frontend's own typefaces, with the typeface its drawing is in and signs to set as others (section 4, the device's print), and lets the light give a key's legend words of its own, move a legend anywhere on its cap, give the caps colors of their own, and put homing bumps on any key or take them off (section 4, the device's light). Draft 7, 2026-10-06, added what is printed on each control as words (section 4, the device's print), so the frontend can set a device's legends anew in another typeface, size or place. Draft 6, 2026-10-04, writes a device's description to be read by people as well as programs (section 4): each control is tied to the drawing by one number, its other pieces following by rule; the light's values are named as the lab names them, every length in millimeters; and the lock lights are described. Draft 5, 2026-10-03, added the device's light (section 4), the description Ectomatter's lab tunes and draws the device's light from. Draft 4, 2026-09-29, was for review before anything was built on it. Draft 3 took in the review of the M3 plugin's author. Draft 4 replaces the device picture with one drawing in named, numbered pieces (section 4, the device's art), so a core brings its own art and the frontend draws it with nothing added by hand. It adds a layout to the shared settings language (section 6, Layout), so a core arranges its settings without drawing them, and lets a core's files carry comments.

Ectomatter is one app for every device on a desk. It follows the model RetroArch made work for emulators: one frontend, and the engines that know each device brought in as **cores**. The frontend owns the windows, the tray, profiles, and every feature that spans devices. A core owns the truth about the devices it handles: how to find them, what they can do, and how to talk to them.

This document is the contract between the two.

## 1. Words

- **Frontend.** The Ectomatter app itself: the interface, and the features that work across every device.
- **Core.** A package that handles a device the frontend talks to itself: a WebAssembly module, with its art and its device files, such as the EVGA Z10, the MagTran M3 or the SteelSeries Sensei 310.
- **Plugin.** A package that adds to the frontend's pages, such as a section on each game's page in the Library, or brings in what another program drives, such as OpenRGB's lighting (section 11). A plugin is packaged and run as a core is, but the two are kept apart: in the frontend's source, in its data folder, and in Settings, which shows each core with its device, under Devices, and the plugins under Plugins.
- **Device.** One physical thing a core has found and connected to.
- **Control.** One part of a device that you press, turn, slide or look at: a key, a dial, a slider, a screen.
- **Capability.** One kind of thing a device can do, such as reassigning keys, lighting or showing images. The frontend has one screen per capability, so every device that offers a capability is edited the same way.

## 2. Principles

1. **Cores are sandboxed modules.** Each core runs inside the frontend as a WebAssembly component, on a thread of its own. A core that fails takes nothing else down: its failure stops at the module. A core reaches nothing but what the frontend lends it, and the frontend lends it only the devices its manifest names. One module runs on every system, so a core's author writes nothing for Windows, macOS or Linux; the frontend's connections do that once. A core can be written in any language that builds a WebAssembly component, and keeps its own license. (Open question: whether a GPL core, such as one built from OpenDeck's deck code, run this way leaves the frontend's license alone, as a separate program did.)
2. **The device is the truth.** When a device stores settings in its own memory, as the Z10 does, the core reads them from the device every time and the frontend never keeps a second copy that can drift. When a device keeps nothing, as the M3 does, the frontend stores its settings and hands them to the core.
3. **The frontend draws everything.** A core never draws interface. It describes what a device can do in data, and the frontend presents it with its own controls. This is what keeps the app coherent as cores are added, and it is the deliberate difference from RetroArch, where every core's options pile into one long menu.
4. **Nothing is written to a device without a reason the user can see.** A preview is shown without storing anything. A save writes, then reads back to confirm. Before a core first writes to a device's memory in a session, it hands the frontend a backup.
5. **Firmware updates are not part of this protocol.** Writing firmware can leave a device unusable, so it stays with dedicated tools.

## 3. Transport and life cycle

### A core on disk

Each core, and each plugin (section 11), is a folder with a manifest, `core.json`, and the module it names. Every file of data in a core's folder is JSON, with comments allowed (`//` to the end of a line, and `/* */`), so a core's files can explain themselves; the messages between a core and the frontend are the same JSON without comments (Messages, below).

The manifest:

```json
{
  "id": "evga-z10",
  "kind": "core",
  "name": "EVGA Z10 RGB",
  "version": "1.0.0",
  "protocol": 2,
  "ectomatter": "0.1.0",
  "module": "core.wasm",
  "license": "MIT",
  "devices": ["EVGA Z10 RGB"],
  "matches": [{ "usb": { "vendor": "3842", "product": "2611", "usagePage": "0008", "usage": "004B" } }],
  "uses": []
}
```

`kind` says which the package is: `"core"`, or `"plugin"` (section 11). A manifest without one is read as a core if it names the devices it reaches (`matches`), and as a plugin otherwise. `module` is the core's code, built for `wasm32-wasip2`; `ectomatter` is the oldest frontend it runs on. `devices` names the devices the core works with, so the frontend can say which before any is plugged in. `matches` is also the whole of what the core may open: the frontend lists and opens for it the devices these name, and no others. `matches` lets the frontend start a core only when one of its devices is plugged in, the way RetroArch picks a core for a file. A device often shows several interfaces and only one of them is the one the core talks to (the Z10's is the LED-indicator collection, usage page 0x08; the M3's is usage page 0xFFA0, usage 1), so a match may name an `interface`, a `usagePage` and a `usage` as well as the vendor and product.

`uses` lists what a package does for itself beyond talking to its devices, so the frontend can tell you before it first runs. A core may use `"sound-capture"` (reading what your computer is playing, which the M3's music backgrounds do) and `"network"` (the M3's weather backgrounds fetch forecasts and alerts). A plugin (section 11) may use `"monitors"` (reading the monitors, and having Ectomatter change them while a game runs); an address it may connect to, and no other, `{ "network": "127.0.0.1:6742", "to": "OpenRGB" }`; a web host it may ask, and no other, `{ "web": "api.open-meteo.com", "to": "Open-Meteo" }`; a file or folder it may read, and nothing in it changed, `{ "read": "/sys" }` (whole, from `/`, with no way up out of it); and `"nvidia"`, NVIDIA's driver, which Ectomatter loads itself and asks for it. `systems`, where a package gives it, names the systems it works on (`["linux"]`, as `std::env::consts::OS` names them); one that doesn't work on this one isn't listed or started here, so no page says it works only elsewhere, and it can't be added here. Settings tags each plugin with the systems it works on, every one where it names none. A plugin that brings in what another program drives, as OpenRGB's does through OpenRGB's server, names that address in `uses`, has no `matches`, and gives `devices` as an empty list. Settings says what each package can do in the frontend's words, from `matches` and `uses` alone ("Talks to OpenRGB on this computer."), never in the package's.

A package also gives `description`, a sentence of what it's for, in its own words ("Brightness, resolution, HDR and more, set for each game."); `glyph`, an SVG file in its folder for its tile in Settings and its devices' rows (drawn in one color, its `currentColor`); `repository`, where its releases are on GitHub (`"owner/name"`), for its updates; and `adds`, what it adds to the frontend's pages: `["game"]`, a section on each game's and app's page (section 11).

### Messages

The frontend loads the core's module and they talk through the interface in `wit/core.wit`. A request is a call to the module's `call` export, with the method and its params as JSON, and the module gives back the result as JSON or a message for the person; these are JSON-RPC 2.0's requests and responses, without the envelope. A notification from the core is a call to the frontend's `notify`. The module asks for its own timers (`after`, answered by its `timer` export), since it has no threads, and reaches devices through `usb`: the HID collections its manifest matches, each opened through the frontend. It also has the WebAssembly system interface's clocks, and its own folder of files, seen as `/data`. A module answers one call at a time, so a long one (a whole read of a device, a save) would hold up everything behind it; between its exchanges with the device, the module can take the calls waiting that it can answer at once without leaving what it is doing (`take-waiting`, naming those methods, such as a preview) and answer each (`answer`), so what the person is looking at keeps up with them. The frontend keeps every other call until the long one returns. The exchanges below are written as the JSON-RPC messages the calls carry.

The first exchange is `initialize`:

```json
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocol":1,"frontend":"ectomatter 1.0.0","platform":"linux-x86_64","dataDir":"/home/someone/.local/share/ectomatter/cores/evga-z10"}}
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
- **kind** says what sort of device it is (`keyboard`, `mouse`, `deck`), for the frontend's words about it: "The mouse can't tell Ectomatter its polling rate."
- **sub**, where a device has one, is a line about it shown under its name, for a device that stands for several: OpenRGB's says what OpenRGB lights ("Motherboard, RAM, graphics card, LED strips and mouse").
- **firmware** says whether the device runs its maker's firmware (`"kind": "retail"`), Ectomatter's custom firmware (`"custom"`, with its build), or neither (`"unknown"`). **features** lists what the custom build adds, such as `sliders.read`; a core offers a custom feature, and accepts its methods, only when the device's build has it, so a device on retail firmware is never sent a command it does not know. **later** names features of newer builds, for the frontend to mention. The frontend keeps custom features apart from everything else, labeled as needing Ectomatter's firmware.
- **controls** lists every part of the device a capability can refer to, and which pieces of the drawing belong to it. Controls have stable ids, so settings stay attached to the right key across versions of a core; an id is a word a person can read, such as `esc`, `f1` or `keypad-plus`, and names the control everywhere else in the description. `label` is the control's name in the interface; `short`, where a control has one, is a shorter name for where room is tight, as on the lab's map of the keys or the cap beside a button in the list of them. `group`, where a control has one, is the heading the frontend lists it under among the device's controls, such as `Side buttons` (a mouse's buttons by where they sit, a keyboard's keys by their rows); controls with none are listed together. What is printed on a control is in the drawing, and as words in its `print` (the device's print, below). A core may give a control fields of its own, as the Z10 core gives each key its `pos`, its place in the keyboard's own reports.
- **screens** lists the device's displays and the pieces of the drawing that show them. The displays capability refers to a screen by its id, as it does to a control. A key with its own small screen, as on a Stream Deck, is a control that also names its `screen`.
- **layout** arranges the device's settings on the frontend's pages (section 6, Layout).
- **capabilities** says which capabilities the device offers, each with its own description (section 5). A device offers only what it really has.

### The device's art

A core brings its own art, and the frontend draws it exactly as it is. Everything that makes up the look, shading, chrome, print and glass included, is drawn in the SVG, so what a core's artist draws in Inkscape or any other editor is what the app shows. The frontend adds only what changes while the app runs: each light's color, a pressed control, the live picture on a screen, and its own interface over the drawing, such as a selection outline or the name of a key's new assignment.

**Size.** `scale` is the drawing's units to the millimeter, so the frontend can draw devices at their real sizes beside each other, as Home's desk does. A device with a light description gives it there instead (`light.scale`, below), and the frontend reads either.

**Pieces.** The drawing is broken into pieces. Each piece is a group or shape whose `id` is its kind and a number, such as `control_12`. The number only tells pieces of the same kind apart: each kind counts from 1 on its own, so a keyboard with 104 controls still has `body_1`. How finely to break the drawing up is the artist's choice, with one rule: anything that changes on its own needs a piece of its own. A key that takes its own color needs its own illumination, a key that presses needs its own control, and a legend that lights with its key needs its own legend.

The kinds, in the order they usually sit from the bottom of the drawing to the top. The drawing itself decides: pieces are drawn in the order they come in the file, as SVG always is, and a kind can sit at more than one depth. The Z10 has light under its caps (thrown on the deck) and over them (on each cap's top, and round each legend).

| Kind | What it is | What the frontend does with it |
|---|---|---|
| `body` | The shell of the device, with its shading and chrome | Draws it as it is |
| `illumination` | The light a control throws, drawn in white at the strength it has at full light, with any blur in the drawing | Sets its color (`fill` and `color`) to the control's, and fades it by how strongly the control is lit |
| `mask` | Where an illumination may show, such as the wells a row of keys sits in: a `<clipPath>`, which the illuminations use in the drawing (`clip-path="url(#mask_1)"`), so the drawing looks right in any editor | Nothing more; it is part of the drawing |
| `control` | A key, button, dial or slider, with its shading | Draws it as it is; nudges it down and darkens it while it is pressed |
| `legend` | What is printed, on the body or on a control, in its color when unlit | Draws it as it is; one tied to a control takes on the control's color as strongly as the control is lit, and moves with it when pressed |
| `screen` | Where a display sits | Shows the display's live picture inside it |
| `overlay` | Anything in front of everything else, such as the glass over a screen and its glare | Draws it as it is |

**Tying pieces together.** Each control names its piece of the drawing by number, `"art": 12`, and its other pieces follow from that number by rule: its control is `control_12`, its legend `legend_112` (100 more) and its light `illumination_12`, `illumination_212` and `illumination_412` (200 and 400 more), where the drawing has them. A key with no LED of its own says so, `"lit": false`. A screen's `"art": 1` is its `screen_1` and the `overlay_1` over it. A drawing numbered otherwise (a device with 200 controls or more, say) names a control's pieces itself: `"legend": 312`, `"illumination": [12, 212, 412]`. Masks need no tie, because the drawing applies them itself. A piece that nothing ties to, such as the body, the print on the body or an overlay, is drawn as it is and never changes.

**The file.**

- Pieces are found by their `id`. Inkscape's labels are not read, because saving as plain SVG removes them; an id is set in Inkscape's Object Properties.
- Each id is used once. A number that is no longer used is not given to a different piece, so settings saved for other pieces do not move.
- Plain SVG only: no scripts, no animation, and text converted to outlines, so the drawing looks the same wherever it is drawn. Filters such as a blur are fine.
- Positions come from the drawing, in its own units: where a control sits is where its pieces are, so the core lists no positions.
- On a screen with room above its white, the frontend draws the illuminations, and the legends they light, brighter than white. The rest of the drawing stays at ordinary white.

### The device's print

What is printed on each control, as words, for the frontend to set a device's legends anew: in another typeface, larger or smaller, or elsewhere on the cap (the lab's Legends section). The drawing keeps the legends as outlines, which draw alike everywhere; the print says what they say and how the drawing sets them.

- Each control's **print** lists its legend's parts in order, each its words (`text`) or one of the device's icons (`icon`): its own legend, the default, its shifted sign beside it (`"role": "shift"`) and its second function at its foot (`"role": "second"`). A sign the cap prints larger than the rest of its kind says how much (`"scale": 1.22`). A control whose cap prints its legend in the middle rather than at the top says `"centered": true`.
- The description's **print** says how the device's legends are printed: the `typeface` and `weight` they are printed in, shipped in the core's folder (`fonts`) or one of the frontend's own (Ectomatter's are listed in `crates/ectomatter-draw/typefaces/README.md`), and a typeface for the signs it lacks (`symbols`, in `fonts`); where the drawing has its legends in another typeface (a device's own may be one no frontend may carry), that one as `art`, `typeface` and `weight`, in `fonts`, with the sizes the drawing has them at (`letters`, `words`, `inset`) where those are not the device's own, so the drawing's legends are kept only where they are asked for as drawn; signs the typeface draws otherwise than the device prints them, each to set as another of its signs (`swap`: Orbitron's zero is slashed, its O is a plain zero); how tall a legend of one letter or sign is (`letters`) and a legend of words (`words`), a capital's height in millimeters as the device itself prints them (the Z10's are measured from photographs of it; a key whose words are printed smaller, such as the Z10's navigation keys, says so in the light's `keys`); how far down from the top edge of a cap's top a one-letter legend starts (`inset`, in mm); and the `icons`, each drawn on a 24-unit square (`fill` and `stroke` path data, the stroke's `width`), `size` times as tall as its part's capitals would be and `drop` times them below its line.
- With the drawing's own typeface (its `art`, or the print's where it has none), sizes and place, the frontend draws the drawing's legends as they are. Otherwise it sets each control's print anew: its own legend at the top of the cap with its shifted sign beside it, words of more than one line one under the other, its second function at the foot, a centered control's legend in the middle; a word too long for its cap set smaller, the control's other words with it.

```jsonc
"print": {
  "typeface": "Orbitron", "weight": 600, "symbols": "Z10 Symbols",
  "art": { "typeface": "Barlow Semi Condensed", "weight": 600, "letters": 3.786, "words": 2.568, "inset": 2.022 },
  "swap": { "0": "O" },
  "fonts": ["fonts/BarlowSemiCondensed-SemiBold.ttf", "fonts/Z10Symbols-Bold.ttf"],
  "letters": 2.8, "words": 1.9, "inset": 2.022,
  "icons": { "mute": { "fill": "M3 9h4l5-4v14l-5-4H3z", "stroke": "M16 9l6 6M22 9l-6 6", "width": 2, "size": 1.92, "drop": 0.15 } }
},
"controls": [
  { "id": "2", "label": "2", "pos": 24, "art": 25, "print": [{ "text": "2" }, { "text": "@", "role": "shift" }, { "text": "PF2", "role": "second" }] },
  { "id": "backspace", "label": "Backspace", "pos": 35, "art": 36, "centered": true, "print": [{ "text": "BACKSPACE" }] }
]
```

### The device's light

How the device's light gets out, for the frontend to draw it as light: the `light` section of the device description. The frontend simulates it, following each LED through the device's shape as a lamp's light is followed, so the section describes the device physically rather than how it should look. Ectomatter's lab tunes it on any device and saves it back here, each value it changes in its place, so the file's comments and lines stay as the core's author wrote them. The lab is the frontend's: it knows no device, and a core carries none of it. A person may also keep light setups of their own in the frontend, each holding only what it changes from this section (the device with other caps on it, say); what they leave alone follows the core, so a change here reaches them too.

```jsonc
"light": {
  "scale": 1.38,       // the drawing's units to the millimeter
  "typical": "2",      // the key the lab draws for all keys
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

- **scale** is how many of the drawing's units make a millimeter. Positions come from the drawing in its own units; every length in the description is in millimeters, every brightness in nits and the LEDs' light in lumens. Every surface is as bright as the light that reaches it makes it, from the LEDs' lumens: nothing is scaled to fit.
- **typical** is the control the lab draws in section when no key is chosen, by its id: a key as most of the device's are.
- **switches** is the switch under each key: its housing's `kind` (`led-slot`, black with its LED through a slot in its top; `clear-top`; or `clear`), its `top` above the deck and its `width`, where its LED sits (`led`, north of the switch's middle, south below 0), the light it gives off at full white (`lumens`), how tight the LED's `beam` is, and how a clear housing glows (`scattered`, `sides`, `glow`).
- **caps** is the cap over it: its `kind` (`shine-through`, `pudding` or `translucent`), its skirt's `foot` and its `top` above the deck, how opaque its walls' plastic is (`opacity`, 0 letting the light inside through, 1 opaque; a translucent cap's top too), how far down a pudding cap's sides its top's color comes (`black`, named for the black tops most pudding caps have), its homing bump (`bump`: `none`, `bar` or `dot`; where none is given, the bump the drawing gives the key), the plastic's colors as sRGB (`color`, its top's, black where none is given; `sides`, its sides', the top's on a shine-through cap and clear on a pudding cap's sides or a translucent cap where none is given), how `milky` the plastic is (its haze: the share of the light it takes in that it scatters and spreads a little way, the less far the milkier it is, 0 clear to 1 milky white; the rest goes straight through), and the shares of the light its `inside` and `outside` reflect.
- **legends**, **wells** and **display** are the legends' `opacity` (how much of the light behind them they hold back, 0 letting it all through, 1 as dark as the cap's black), how a legend looks with its LED off (`unlit`, a share of the drawing's gray) and how they are set where it differs from the device's print (`typeface` and `weight`; `place`, `printed` (where the device prints each, its centered keys in the middle; the default), `top` (at the top on every key), `middle` or `topleft`; `letters`, `words` and `inset`, as the print's; and how far a legend is moved from where its place sets it, `across` to the right and `down`, in millimeters, to the left and up below 0; and each part's own, its own legend's, its shifted sign's and its second function's, `main_`, `shift_` and `second_` before `size` (how tall its capitals are, in millimeters, in place of `letters` or `words`, which it follows where none is given), `across` and `down` (moved on from where the rest puts it): a keypad's arrows smaller than its numbers, say); the wells (the mask piece of the drawing that holds them, their `depth`, and the shares of the light the `deck` and the `chassis` reflect); and the display's backlit dots, in nits, with how much of its cell a dot fills and how strongly the glass shows its sheen.
- Each value is one of the lab's sliders or choices, named for it: `inside` is the lab's Inside reflection, and `milky` its Haze. The Z10's file says what each one is in a comment beside it.
- **keys** holds a key's own values, where they differ from all keys', by the control's id. An entry takes any of the sections above that belong to a key, `switches`, `caps` and `legends`, and only what differs: a stock cap among pudding caps, say. It can also say where the key's switch sits under its cap: **place**, how far east and north of the cap's middle, as on a long key whose switch is off its middle. The LED goes with its switch. A key's own `legends` can also give its legend words of its own, each part in place of the print's: `main` (its own legend, a line break between its lines), `shift` and `second`, an empty one taking that part away; a cap that prints ⌘ where the device prints Win, say. A line that is one of the print's `icons` by its name in brackets (`[mute]`) is that icon. A part can instead be a picture of the person's own, `main_image`, `shift_image` or `second_image`, by the name of an SVG file the frontend keeps beside the person's setups (an artisan cap's logo); a key whose own legend is a picture has it in the middle of its cap. A key with nothing of its own has no entry.
- A button with no light of its own stands in the keys' light too: the drawing shows it as a block (its control's first piece, with no shadow drawn under it as a cap has; its face the next piece where that lies inside it). Its entry in **keys** says where it stands and how high, its caps' `foot` and `top`, as the Z10's G/PC and media keys stand on its chassis.

The lock lights, small windows with an LED behind each, are the description's `lock_lights`: the piece of the drawing for each of `num`, `caps` and `scroll`, their `color` and `brightness`, how far from the LED a window stays bright (`spread`), how lit its corners stay (`floor`), and the little light out of its edges (`spill`, its brightness and how far it reaches).

```jsonc
"lock_lights": {
  "num": "num-lock-light", "caps": "caps-lock-light", "scroll": "scroll-lock-light",
  "color": "#ff6000",
  "brightness": 300,
  "spread": 1.45,
  "floor": 0.5,
  "spill": { "brightness": 6, "reach": 0.72 }
}
```

A device whose lights are windows of their own, as a mouse's logo and wheel are, each an LED under a piece of diffusing plastic, describes them in **window**: the light each LED gives off at full white (`lumens`), the share of it that comes out through the window's face (`through`), how far under the face the LED sits (`depth`, in millimeters: close under it, the window is brightest over the LED; deep under it, as at the end of a light pipe, the light spreads evenly), the share that gets out round the window's edge onto the shell beside it (`halo`) and how far that reaches (`reach`), and where the LED sits under the window (`east` and `north` of its middle). The light falls across the window by the same law the keys' light falls by, and all of it that comes out leaves through the window's shape, so a window is as bright as its LED's lumens make it. Each light's own values are its `window` under **keys**, by the light's id (the lighting capability's `lights`), as a key's are. A core whose description its module makes, as the Sensei 310's does, names a file in its folder for its light (`"light": "light.json"`), the whole file the light, which the lab saves to.

```jsonc
// light.json, in the core's folder
{
  "window": { "lumens": 0.8, "through": 0.5, "depth": 8, "halo": 0.05, "reach": 2 },
  "keys": { "wheel": { "window": { "lumens": 0.25, "depth": 3 } } }
}
```

What a device has decides what the lab tunes: keys with LEDs under caps, wells round them, a display, small lights behind windows, lights that are windows of their own. Anything the description leaves out is the frontend's ordinary default.

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

- `keys/get {device, profile}` returns every control's assignment in every layer, by the controls' ids, and the same as the device was made: `{"layers": {"base": {"esc": {"key": "Escape"}, ...}, "fn": {...}}, "factory": {"base": {...}, "fn": {...}}}`.
- `keys/set {device, profile, layer, control, action}` changes one; `keys/reset` returns one to its factory assignment where `factory` is true. A device that writes a whole profile at a time takes every change to it at once with `profiles/set` (below).

A device that stores what it is set to but cannot tell it back says `"readable": false` (or names what it can tell, as `"readable": "colors"`), and its core keeps its own record of what it has set; the core's `get` answers then say which values are `known`, from the device or because Ectomatter set them. `keys/get` gives one `known` for all the controls of a device that takes them in one report (`false` until Ectomatter has set them); `lighting/get` one for each light (`{"logo": true, "wheel": "colors"}`: `true` once Ectomatter has set it, `"colors"` when the device tells only its color, `"none"` when it tells nothing); `settings/get` one for each setting. The frontend writes no value that isn't known on its own: a profile it makes from what a device is set to leaves those values out, so putting the profile on never writes a guess. A change the person makes goes through, and when it would write values that aren't known along with it (a mouse taking all its buttons in one report), the frontend says so before it is made. A mouse's buttons add two actions: switching between its sensitivity presets (`{"sensitivity": "switch"}`) and scrolling (`{"scroll": "up"}`, `{"scroll": "down"}`).

An **action** is one of a fixed set the frontend knows how to present: a key (`{"key": "F13"}`, a modifier alone included), a shortcut (`{"shortcut": ["LeftCtrl", "C"]}`, the modifiers first), a media key (`{"media": "Mute"}`), a mouse button (`{"mouse": 1}`) or scrolling (`{"scroll": "up"}`), a stored macro (`{"macro": 3, "plays": "times", "times": 2}`, or `once`, `while-held`, `until-pressed-again`), a profile switch (`{"profile": 2}`, `{"profile": "next"}`, `{"profile": "previous"}`), the Fn layer (`{"fn": true}`), a mouse's next sensitivity (`{"sensitivity": "switch"}`), nothing at all (`{"none": true}`), off (`{"off": true}`), one of the device's own actions (`{"own": "game-mode"}`, which the keys description lists in `own`, each with its `label`), or `{"host": 0}`, one of the device's host slots (the keys description's `host.slots`), which tells the frontend to run one of its own actions when the control is pressed (section 7). A code no action can hold travels as `{"raw": [3, 1, 0, 0]}`, the device's own bytes, so a core never loses what a device stores; the frontend shows it as a code and writes it back unchanged. Key, modifier and media names are the ones in the crate `ectomatter-protocol` (the USB HID usages: `F13`, `ArrowUp`, `LeftMeta`, `Mute`), so every core means the same thing by each.

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
- Steps are one shared shape across cores: `{"key": "LeftCtrl", "down": true, "after": 120}` (`after` is the wait before the next step, in milliseconds), `{"media": "Mute", "down": true, "after": 0}`, and a wait before the first press, `{"wait": 40, "after": 0}`. A key a device's macros can't name travels as its usage number.
- `macros/list` gives each stored macro by its slot: its `name`, `steps`, how the device keeps it playing (the Z10 keeps `run`, `repeat` and `unit`), its `length`, and whether the core can rewrite its steps exactly (`editable`). `macros/set {device, macro, name, steps, ...}` and `macros/delete {device, macro}` answer with every macro.
- `budget` lets the frontend show how much room a macro has left before the device refuses it.
- `record` means the core can record from the device itself. `macros/record/start` takes the device's input for the recording alone (so its keys reach nothing else), `macros/record/status` reports progress, and `macros/record/stop` returns the steps. A device without `record` can still have macros; the frontend records them from the system's keyboard.
- A device with `"stored": "host"` gets its macros played by the frontend when its controls send input.

### lighting: effects, previews and live color

```json
"lighting": {
  "stored": "device",
  "lights": [{ "id": "keys", "label": "Keys", "zones": [{ "bit": 128, "label": "Whole keyboard", "whole": true }, { "bit": 2, "label": "F-keys" }] }],
  "effects": [
    { "id": "wave", "label": "Wave", "note": "Colors roll across the keyboard.", "params": {
        "colors": { "type": "colors", "max": 7 },
        "speed": { "type": "duration", "min": 1000, "max": 5000, "label": "Speed" },
        "direction": { "type": "choice", "options": [{ "value": 0, "label": "Up", "icon": "up" }, { "value": 1, "label": "Down", "icon": "down" }] },
        "zone": { "type": "zones", "label": "Zones" } } }
  ],
  "preview": true,
  "direct": { "controls": 116, "rate": 25 }
}
```

- Effects are the device's own built-in animations, each with parameters in the shared settings language (section 6), so the frontend builds every effect's editor from data. An effect may carry a `note`, a sentence the frontend shows over its settings ("One color, steady."), and `start`, the settings it begins with, as its maker set it up, when a light that doesn't have its colors yet is set to it (the Sensei 310's Color shift starts as SteelSeries GG's does: red, green and blue, a second around). Without a layout, an effect's settings are drawn colors first, then choices, then amounts and speeds, then switches.
- A device whose lights each play an effect of their own (a mouse's logo and wheel) lists them as `lights`, each with an `id`, a `label` and its pieces in the art; `lighting/set {device, light, effect, params}` then sets one. It lists them in the order they sit on the device, front to back (a mouse's wheel, then its logo), and the frontend lists them so, matching its picture of the device.
- `lighting/get` and `lighting/set` read and write the stored lighting of a profile. A device that keeps each effect's settings whatever effect is in use, as the Z10 does, gives them all: its keys' light is `{"effect": "wave", "effects": {"static": {...}, "wave": {...}}}`, its display's backlight `screen`, and what game mode leaves lit `game_keys`.
- A device that writes a whole profile at a time (section 5, profiles) says where a profile keeps each light's effect and each effect's own settings, as JSON pointers into the profile: `"kept": {"keys": {"effect": "/lighting/keys/effect", "settings": "/lighting/keys/effects/{effect}"}}`. Each of an effect's settings says where it is within the effect's own settings (`at`, section 6), so the frontend reads and changes it there and sends the profile whole, as the device keeps it.
- A light lit in zones lists them, each by its bit in an effect's `zones` setting (section 6), in the order the frontend shows them; the one that is `whole` lights every part of the light: `"zones": [{"bit": 128, "label": "Whole keyboard", "whole": true}, {"bit": 2, "label": "F-keys"}, ...]`.
- An effect may say `reset`: the frontend offers a Reset that puts its settings back as the device's factory profile has them. `reactive`: it plays as keys are pressed, and the frontend offers pretend typing on its drawing of the device. `paint`: the light is painted control by control, the frontend keeping the colors and streaming them as `direct` sends them, with a brush of its own. An effect's `note` may name the profile being edited, `{profile}`, and the device's kind, `{kind}`.
- Each light `lighting/get` gives says what it `plays`: `{ms, held, stops}`, the cycle the device runs, `ms` long, through `stops` (`{at, color}`, `at` from 0 to 255 through the cycle, blended straight from each to the next), or one color `held`. `lighting/set` answers with the same for the light it set, once the device has taken it, which is when the device starts the cycle. The frontend plays it on the device's drawing, so the drawing and the device run the same effect at the same moment.
- `preview` means the device can show an effect without storing it. `lighting/preview {device, effect, params}` starts or updates a preview (the Z10's takes a whole `lighting`, as `lighting/get` gives it, and `t`, where the frontend's own drawing of the effect is), and `lighting/preview/stop` hands the lighting back to the stored profile. `lighting/hand-back` does the same for a preview a frontend left running when it stopped short. The core runs whatever loop the device needs (the Z10 needs a frame every 40 ms) and stops by itself if the frontend stops refreshing.
- `direct` means the frontend can set every lit control's color itself, up to `rate` times a second, with `lighting/direct {device, colors}` (one `[r, g, b]` for each control, in the order of the controls' places). This is what makes lighting that spans the whole desk possible: the frontend draws one effect across every device and streams each its part. A device whose lights each play an effect of their own names them in `direct`'s `lights`, and takes `colors` as one `"#rrggbb"` for each light by its id. Streamed colors are never kept in the device's memory, nor is anything else until `lighting/hand-back` puts the lights back as the device keeps them; then what changed meanwhile is kept.

### displays: screens and image keys

```json
"displays": [
  { "control": "lcd", "kind": "text", "cols": 20, "rows": 4, "charset": "cp1252" }
]
```

A deck whose keys each have their own small screen offers one display per key: `{"control": "k0", "kind": "image", "width": 72, "height": 72}`. A deck like the M3, whose keys are windows onto one screen behind them, offers that screen: `{"control": "panel", "kind": "image", "width": 854, "height": 480, "windows": [...]}`, where `windows` is the geometry Ectomatter already uses to place each key's image and to line a background up behind the keys.

A core draws its own displays. The M3 is a whole screen with magnetic keys over it, and its core composes that screen (key images, labels, animated backgrounds) the way the M3's OpenDeck plugin does today. The frontend asks for what it needs to show you:

- **Live feed.** `display/feed/start {device, control, rate}` starts a feed of what the display is showing right now, at no more than `rate` frames a second (the frontend asks for about 30; the app needs no more), until `display/feed/stop`. The frontend shows it in the device's picture, so the screen on your desk and the one in the app match.
- **Preview.** Nearly everything on a screen like the M3's moves, so one still frame would misrepresent it. A preview is a short feed of its own: `display/preview/start {device, control, content, rate}` draws how some content would look, such as a new background, without sending it to the device, until `display/preview/stop`. A preview running beside the live screen roughly doubles the core's drawing work, so the frontend runs one only while it is on screen.
- **Content.** `display/show {device, control, content}` applies it. Content is what to show, such as text, an image, a key's label and icon, or a background by name.

**A text display** takes its rows with `displays/show {device, control, rows}`, as many rows as it has, each cut to its columns, and `displays/hand-back {device, control}` leaves it to the device's own screens, as the device's own software does on closing. A core sends either between the exchanges of a long call (a whole read, a save), as it does a preview, so a screen that changes every tenth of a second keeps its time while the frontend reads the device. The Z10's core spells each row in the characters its screen draws (printable ASCII, the degree sign and the full block) before sending it.

**Frames travel on their own channel.** Text displays send their rows in the feed's notifications. Image frames are too large for the message line (60 frames a second of JPEG is several megabytes a second), so `display/feed/start` and `display/preview/start` answer with a frame channel: a local socket (a named pipe on Windows) on which each frame arrives as a small header (`type`, `width`, `height`, `length`) and then its bytes. A frame is whatever the core already makes, JPEG or PNG, as its header says; the M3 core already compresses every frame it sends the deck as a JPEG, so the feed costs it nothing extra.

**What a display can show is a catalog.** An image display lists the content it accepts in its description:

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

Each background carries its own settings in the shared settings language, and the catalog can change: when you add a shader file of your own, the core sends `display/catalog/changed` and the frontend reads the list again.

### input: presses, turns and slides

Notifications from the core whenever a control is used: `input/down`, `input/up`, `input/turn {ticks}`, `input/slide {value}`, `input/touch {x, y}`, each with the `device` and the `control`'s id, and for a control on one of its host slots, the slot's number as `host`. Devices run by the host, such as the M3, depend on these entirely. Devices with their own memory, such as the Z10, send them only for controls assigned to `{"host": ...}`, so a keyboard key can trigger a frontend action without the keyboard losing its own assignments.

A device can't always say which control was pressed, only what the press sent: a keyboard's report says a key's code went down, not which of its keys sent it. A core tells what it can. Where it knows the control, it gives `control`; with each notification it may give `sends`, what the press sent as an action is written (section 5, keys): `{"key": "A"}`, `{"media": "VolumeUp"}`, or for a host slot `{"host": 2}`. The frontend finds the control whose job sends that, as it does for a key the person presses to pick it.

Devices with their own memory send presses for host slots alone, as above, unless the frontend asks for more: `input/watch {device, all: true}` asks for every press, for a picture of the device that shows each key as it is used; `{"all": false}` goes back to the host slots alone. The answer says whether the core is listening now (`{"listening": true}`). A core that offers it says so in its input capability: `"input": { "watch": true }`.

How a core hears a host slot is its own business. The Sensei 310's core gives each of the mouse's buttons one slot, a keyboard usage no key has (`0xA5` and on, reserved by the USB tables; Linux reports it as `KEY_UNKNOWN`, which the desktop's keymaps give no symbol, so no app types anything for it): the mouse sends it from its keyboard interface when the button is pressed, and the core, reading that interface while any button has a slot, tells the frontend which button it was. The mouse itself keeps working without the frontend; a button on a slot then does nothing. While the frontend watches every press, the core also reads the pointer's own reports on the mouse's second interface (its buttons as bits, unnumbered), beside the computer, which still gets every one, and the keys and media keys its keyboard interface sends; it tells each press by the button whose job sends what came (the first, where two buttons send the same), with `sends`. The sensitivity button and a button set to scroll send nothing it can tell apart, so they aren't heard. It opens the pointer's reports only while watched.

The Z10's core hears its keys in the keyboard's own reports, read beside the computer, which still gets every key: the boot keyboard's report on its first interface (the modifiers, then six keys), the ten-key report (number 0x10) on its third, and the media keys (report 2) on its second. It listens every 8 ms while keys are in use and every 32 ms once nothing has come for two seconds; the keyboard keeps each report until it is read, so a tap shorter than that is still told, down and then up. It tells `device/added` and `device/removed` as the keyboard comes and goes, looking for it every two seconds while it is gone; a key held when it goes is let go. On Windows, where the system keeps every keyboard's reports to itself, a core can't hear them this way, and the frontend will need its own route there.

A core acts on its own input first and tells the frontend second. A press has to show on the device within a frame (the M3's backgrounds answer every press with a ripple, a splash or a fish feeding), and a round trip through the frontend would be too slow for that.

`input/simulate {device, control, kind}` goes the other way: the frontend presses a control as if it had been pressed on the device, which is what happens when you click a key in the app's picture of the device.

### profiles

```json
"profiles": { "stored": "device", "count": 5, "factory": true }
```

- `profiles/list` and `profiles/active` read them; `profiles/activate {device, profile}` switches.
- `profiles/get {device, profile}` gives one profile whole: its `name`, its `keys` (as `keys/get` gives its layers) and its `lighting`. `profiles/set {device, profile, name, keys, lighting}` writes any of them at once, as a device that stores a whole profile at a time needs (the Z10 does), and answers with the profile as the device holds it after.
- `device/read {device}` gives everything a device's page shows at once, in these shapes: the firmware, the active profile, each profile, the keys and lighting as the device was made (`factory`), and every macro. A core may serve it from one pass over the device, as the Z10's does.
- For devices with `"stored": "host"`, the frontend keeps profiles itself and hands the core what to apply.
- Either way, the frontend can switch profiles when an app or game comes to the front, for every device at once.
- A device that can't tell which profile it's on says so, `"active": false`, and lists its profiles in its description (`list`, each with its `id` and `name`). Where it can put back what it was on before the frontend first switched it, it says `"restore": true`, and `profiles/restore {device}` does that, as the app that switched it leaves the front; it answers with the profile it went back to. OpenRGB's lighting is such a device: a profile there sets every device OpenRGB lights at once, and OpenRGB says which profile is in use only at protocol 6, which the OpenRGB plugin asks over a connection of its own.

### settings: everything else a device has

Device settings that belong to no capability, such as the Z10's game-mode keys or its display's backlight color, described in the shared settings language (section 6): `settings/get` and `settings/set`.

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
| `color` | a color swatch | Static color |
| `colors` | a row of swatches, up to `max` | Wave colors |
| `zones` | the whole light or some of its zones, a chip for each (the light's `zones`), kept as the bits of those lit | Where an effect plays |
| `text` | a text field, with `max` length | Profile name |
| `place` | a search for a place by name, stored as its name with its latitude and longitude | The city a weather background shows |
| `image` | a picture you choose | A background photo |

Every setting may also carry a `label`, the words for it; `default`, what the device does while it has no value for it, so the frontend shows what the device really plays; and `short`, a shorter name for where its section already says what it is about (a mouse's "First" and "Second" under Sensitivity), as `short` does for a control. A `duration` that says `speed`, or whose id is `speed`, is how fast something goes: its slider runs from slow to fast, and its value is the time a cycle takes.

An `int` with `percent` is shown as a percent of its span (a light's level, 0 to 255), and with `tint` its slider is in the effect's first color. A `choice`'s options may be objects with words and an icon of their own, `{"value": 3, "label": "Left", "icon": "left"}` (the icons are the frontend's, by name: `up`, `down`, `left`, `right`, `clockwise`, `counterclockwise`); one with `rule` has a rule before it in a menu. A `choice` with `menu` is drawn as a dropdown, and its `other` gives the words for a value no option has, `{0}` and `{1}` that value's parts, so the value is shown, never hidden.

A setting a device keeps in a profile it writes whole says where, within the effect's own settings: `at`, a JSON pointer (`/brightness`; `/colors/*/brightness` every color's, written to each and read from the first), and `also`, more places the same value is written where they're kept (`/background/brightness`). A list of colors kept as objects names where each color is in its object (`item`, `/color`) and how many are shown (`count`). A value made of several kept fields lists them (`fields`, `["/start", "/stop"]`), its value then a list of theirs. `form` says how a value is kept: `"rgb"`, a color as its three numbers; `"number"`, a checkbox as 1 or 0.

A core cannot send its own interface. If a device needs something this language cannot describe, the language grows in a new protocol version, so every core gains the new control at once.

### Layout

The types above say what a setting is. A core's layout says where each one goes, so a device's settings read in a sensible order without the core drawing anything. The pages stay the frontend's: Keys, Lighting, Macros, Screen and Profiles look the same for every device, and a layout only arranges what goes inside them, plus the device's own settings on its Device page.

The device description carries the layout, written out or as the name of a file in the core's folder (`"layout": "layouts/z10.json"`):

```jsonc
"layout": {
  "lighting": {
    "wave": [
      { "section": "Colors", "show": ["colors"] },
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
- A page is a list of **sections**, drawn in order, each with a heading, the settings it `show`s, by their ids, and, if it needs one, a `note` under them.
- An item in `show` is a setting's id; `{"row": [...]}` for settings drawn side by side; or `{"id": ..., "when": ...}` for a setting shown only sometimes.
- **when** is a condition, on a setting or a whole section: every setting it names must have the value given, or one of the values in a list (`{"effect": ["wave", "ripple"]}`). `{"feature": ...}` holds when the device's firmware has that feature (section 4). A setting hidden by its condition keeps its value.
- A setting the layout does not place is drawn after the placed ones on its page, in the order the core listed it, so a missing or out-of-date layout never hides anything. A layout is optional.
- In an effect's layout, a section with `"box": "place"` goes in a box of its own beside the effect's settings, titled by its `section`: where and when the effect plays. A `zones` setting there comes first, as the whole light or some zones; the rest each as a menu, under a hairline.
- A plugin's section on a game's page (section 11) is a list of parts, drawn in order in one box: `{"show": [...], "note": ...}` rows, each a setting as a dropdown with its label (an item may be `{"id": ..., "label": ...}`), the note under the last; `{"grid": {"columns": [...], "rows": [...]}}`, a column for each of several things alike (each `id`, `label` and a `note` under it: a monitor, its mode now) and a row for each setting they have, each cell a setting's id, or `{"none": "No HDR"}`, the words for one that can't; and `{"note": ...}` alone.

The layout is the whole of it: headings, order, rows and conditions. Sizes, colors, spacing and styles stay the frontend's, so every core looks like the rest of the app.

## 7. What the frontend adds across every device

These are why the platform is more than a set of separate apps:

- **Profiles that follow the app in front,** switching every device together.
- **One Macro Editor,** for devices that store macros and for devices whose macros the frontend plays.
- **Lighting across the whole desk,** drawn once by the frontend and streamed to every device that offers `direct`.
- **Live pictures of every display,** from each core's feed, with previews before anything is applied.
- **Content for any display:** system stats, clocks, images, key labels.
- **Actions.** A control assigned to `{"host": ...}` runs an action: open an app, send a shortcut, control media, or run a Stream Deck plugin's action. The Stream Deck plugin host is OpenDeck's code, which is GPL, so it runs as its own service beside the frontend rather than inside it.
- **Backups** before any write, for every device with its own memory.
- **Light setups of the person's own** over each core's light, made in the lab, for the device as they have it.
- **Custom Scripts:** the person's own commands on each game's and app's page, run as the game starts and undone as it quits (below).

### Custom Scripts

Custom Scripts is Ectomatter's own section on each game's and app's page, after the plugins' (section 11): the person's own commands, each a command to run as a game starts and another to undo it as the game quits, the way Sunshine runs an app's "do" and "undo". Ectomatter keeps them, in the Library, and runs them itself; no core or plugin is told them, and none can run one. As a game starts, Ectomatter makes the plugins' changes first, then runs the commands, Every game's before the game's own; as it quits, each is undone in the opposite order, and Ectomatter keeps its record of them as it does of a plugin's changes (section 11, Putting things back). Ectomatter never runs a command as an administrator: one that would ask to be one (`sudo`, `pkexec`, `doas`, `su`, `run0` and the like) is refused, and its form says so before it's added. A command runs in the system's own shell, in a process group of its own, so stopping it stops what it started too, with the game's name and id in `ECTOMATTER_GAME` and `ECTOMATTER_GAME_ID`; it's waited for up to its time limit (and stopped there), or kept running until the game quits, or neither. While games start through Steam, Ectomatter can hold up only its own next change, not the game's start. Each command's last run is kept with it, for its card to say how it went.

## 8. How the first cores fit

| | EVGA Z10 | MagTran M3 | SteelSeries Sensei 310 |
|---|---|---|---|
| Stored where | on the device | on the host | on the device, with a record on the host of what it can't tell back |
| keys | 116 keys, two layers | 15 keys and 3 dials, host actions | 8 buttons, one layer |
| macros | 100 slots, recorded on the device | played by the frontend | played by the frontend, from a button on a host slot |
| lighting | 8 effects, preview, direct at 25 per second | none | 2 lights (the wheel and the logo), 4 effects, direct at 8 per second |
| displays | 20 by 4 text | one 854 by 480 screen behind the keys, seen through each key | none |
| input | host-assigned keys, the volume slider | every key and dial | host-assigned buttons |
| profiles | 5 on the device | the frontend's | the frontend's, one put on the mouse at a time |
| Built from | the Rust core already written | the M3's OpenDeck plugin | written anew, with libratbag's SteelSeries driver as reference |

The Z10 and the M3 sit at opposite ends: one keeps everything on the device, the other keeps nothing. If both fit this protocol without special cases, it will hold for most of what is in between.

The M3 as its core will describe it: 15 keys in three rows of five; three dials down the right edge that turn and press, with no screens of their own; no touch; one 854 by 480 screen seen through a 110-pixel window over each key; nothing stored on the device, so the core sends everything again on reconnect, with a watchdog that reconnects within 15 seconds.

**Never offered.** Some commands a device accepts do harm, and a core must not offer them as settings, however tempting:

- **MagTran M3: key sensitivity and actuation.** Its vendor commands for these (QUCMD and SENS) make keys press on their own and miss presses until the deck is unplugged. The deck core offers no key sensitivity at all.
- **EVGA Z10: the firmware updater.** Entering update mode cannot be undone without a complete write, so the Z10 core has no path to it (section 2, principle 5).

## 9. Versions

`protocol` is one whole number. A frontend supports a range of versions and says which it speaks in `initialize`; a core that cannot speak it declines. Within a version, new optional fields may appear and must be ignored by anyone who does not know them.

## 10. Open questions

Settled in review: recording on Windows uses the system's keyboard hook, as Unleash does, and holds keys back from other apps while recording (only Ctrl+Alt+Del and Windows+L cannot be held); cores draw their own displays and the frontend gets live feeds and previews (section 5, displays); cores will be offered from a catalog later, signed so a core's source is known before it runs.

1. **System permissions.** Each system asks once before an app may do certain things: on macOS, Input Monitoring (to read a keyboard's presses, which recording needs) and Accessibility (to send keystrokes, which shortcut actions need); on Linux, a rule installed once with admin rights so the app can reach the devices without running as administrator; on Windows, usually nothing. A core reports that it needs one (`device/status` with `"needs": "input-monitoring"`), and the frontend explains the step in plain words at the moment it is needed. What a core does for itself, such as capturing sound or using the network, is declared in its manifest's `uses` instead (section 3). The exact list grows as cores find what they need.

## 11. Plugins

A plugin is a package that adds to the frontend's pages, a section on each game's and app's page in the Library (`"adds": ["game"]`), or brings in what another program drives, or both. It's packaged and run as a core is (section 3): a folder with its manifest (`"kind": "plugin"`), its module and its glyph, run inside the frontend as a WebAssembly component, reaching only what its manifest names. Plugins are kept apart from cores: in Ectomatter's source (`plugins/`, beside `cores/`), in its data folder, and in Settings, which shows each core with its device under Devices (the device's switch is its core's) and each plugin under Plugins, each with its glyph, its name, what it's for, what it can do, its version, and On and Off. Some come with Ectomatter; the rest are added, from a GitHub address or from a file (an `.ectocore`, a zip of its folder), after Settings has said what the package can do, into the section its `kind` names, whichever section it was added from. Once a day, for each package with a `repository`, core or plugin, Ectomatter reads the manifest attached to the repository's latest release (`core.json`) and, where its version is newer and it runs on this Ectomatter (its `ectomatter` and `protocol`), downloads the package attached beside it (`<id>.ectocore`) into a folder for that version, and uses it from then on. GitHub's own address for a latest release's file is used, which needs no account. A package added from a GitHub address keeps that address beside it for its updates.

A game's page in the Library holds Ectomatter's own sections, Devices first and Custom Scripts last (section 7), and each plugin's between them, in Settings' order. Before them, on the left, On this page lists each section and what it changes for that game, as the page's way around. Every game's page holds the defaults a game follows until it chooses for itself; an app's page has the same sections without them.

A plugin that brings in what another program drives, as OpenRGB's brings in the lighting OpenRGB controls, tells `device/added` for each device as it finds them, as a core does, and they join Devices on each game's page, switching while the game is in front as the keyboard and the mouse do. OpenRGB's is one device, its profiles OpenRGB's own, since a profile there sets everything OpenRGB lights at once.

### The calls

- `game/describe {for, name}` asks for its section on a page: `for` is `"game"`, `"app"` or `"every"` (Every game's page), `name` the game's or app's. It answers `{"section": {"label", "settings", "layout"}}`, its settings in the shared settings language (section 6), each a `choice` with `menu`, its `options` and its `default`, laid out in the parts section 6 describes. The frontend keeps each page's choices (a game's own, and Every game's), shows a choice the page follows in gray, and marks Every game's in a menu.
- `game/changes {game, values}` asks what a game's choices change as it starts: `game` is its `id`, `name` and `kind`, and `values` its choices (its own over Every game's). It answers `{"changes": [...]}`, in the order to make them, each `{"change", "label", "words"}`: the change, a few words in capitals for the keyboard's screen while it's made (`"BRIGHTNESS"`, at most 20 letters), and words for saying it was put back ("the Side 13 2K’s brightness").
- A plugin tells `plugin/status {warning}` when something stands in its way that the person can fix (OpenRGB's server off), shown under it in Settings, and an empty warning once it's gone; and `game/changed` when its sections would describe themselves otherwise (a monitor plugged in).

So far a change is of one kind, and Ectomatter makes none a plugin's `uses` doesn't name:

- `{"monitor": id, "set": what, "to": value}`, for a plugin that uses `"monitors"`: `brightness` (0 to 10000), `mode` (`{width, height, refresh}`, refresh in millihertz), `hdr` (`true` or `false`, wide color with it, as the system's own settings set them), `vrr` (`"never"`, `"always"` or `"automatic"`), `enabled` (`false` turns the monitor off; its windows go to the others) and `main` (`true` makes it the main monitor).

### A plugin's own Settings section

A plugin that adds to Settings (`"adds": ["settings"]`) describes a section of its own there, drawn after the plugins' list, in Settings' order. It tells it as it changes, `settings/section {section}` (a notification, so the frontend never waits on a plugin busy on the network), as `{"label", "settings", "layout"}` in the shared settings language (section 6). Ectomatter keeps the section's values, in its own settings, and hands all of them over as one changes and as the plugin starts, `settings/set {values}`, without waiting for the answer. The layout's parts, in order:

- `{"show": [ids], "note"}`: a row for each setting, its note under the last. A `choice` is its options side by side; a `search` is a box to type in, `text` the words it holds, whose matches the plugin finds once typing pauses, `settings/search {id, text}`, answering `{"found": [{"value", "label", "sub"}], "none"}` (`none` the words for finding nothing). The value chosen from them is the setting's value.
- `{"card": {"label", "big", "beside", "facts"}}`: a box with a reading in large, words beside it, and facts two to a line (`[["Cloud", "0%"], ...]`); or `{"card": {"label", "waiting"}}`, the words while it waits for them.
- `{"status": {"label", "tone", "words", "note"}}`: a dot (`tone` is `"quiet"`, `"good"`, `"bad"` or `"warn"`) and its words, the note under them.
- `{"credit": words}`: whose the data is, in small words.

### Readings

A plugin with readings (`"adds": ["readings"]`) tells what it can read, `readings/list {readings}`, and each reading as it comes, `readings/changed {values}`: a value by its id, a number (in the reading's own unit: °C, %, MHz, GiB, W, RPM), a share of a whole `[used, all]`, words, or `null` for one it can't have now. Ectomatter keeps the latest of each, for every page and screen that shows them, none of which waits on the plugin. Where reading costs something, the plugin reads only what's wanted: Ectomatter tells it, `readings/watch {ids}`, all that every screen and page showing its readings wants, as that changes.

Two plugins come with Ectomatter this way. **Weather** reads the weather at the place chosen in its section, from Open-Meteo every quarter of an hour, and the warnings in force from the service that issues them there every three minutes, through `net.fetch`: its readings are `temperature` (°C) and `sky` (words). **Sensors** lists every sensor on a Linux computer, each named as people say it and in the part of the computer it reads (its readings are the list: devices, each with its part, its name and its sensors), and reads those watched once a second, through `system.read` and the rest; Ectomatter draws the Sensors box in Settings from its list, and keeps which sensors the screens may show and their names.

### Putting things back

Putting things back is the frontend's job, never a plugin's. Before it makes each change, Ectomatter writes it down with what it was (a monitor's brightness, say, as it reads it then; for one of the person's own commands, section 7, its undo), in a record in its data folder; then it makes it. As the game quits, it puts each one back, the last first. Where a game started later has changed the same thing, the first to quit hands the later one what it was, rather than putting it back under it. If Ectomatter stops while a game runs, the record stays: at its next start, a game still running keeps its changes until it quits; one that isn't running has its changes put back at once, and Ectomatter says what it put back, in the Library and as a notice on the desktop. A plugin turned off has what it changed for a game running put back first. A check run offscreen (a snapshot) makes no change and runs no command.

A game is seen starting and quitting by the Library's watch: a Steam game or app by Steam's own process for it, a game added by hand by its program. An app that doesn't run through Steam starts as its first window opens and quits as its last one closes.

### What a plugin may reach

- **The monitors** (`system.monitors`, for a plugin that uses `"monitors"`): a JSON list, each with its `id`, made from its EDID (its maker's code, its product, its serial and the week it was made), so the same monitor has the same id whatever port it's on; its `name` (the EDID's) and `maker`; the `port` it's on now, only to show; whether it's on (`enabled`) and the `main` one; its `modes` and the `mode` in use; and what it can do, each absent where it can't: `hdr` and `vrr` as they are now, `brightness` (0 to 10000), and `ddc` (whether DDC/CI may be used). On KDE, Ectomatter reads them from KDE's own output protocols and the brightness from PowerDevil, and changes them the same way, so the system's slider and keys agree.
- **An address** (`net.connect`, for one its `uses` names, as a core's may too, section 3): a connection, written to and read from with a time limit, as a USB device is.
- **The web** (`net.fetch`, for a host its `uses` names): what a web address gives, asked with GET over HTTPS or HTTP, as text, with Ectomatter's name on the request; an answer other than success is an error with its status.
- **Files, read only** (`system.read`, `system.entries` and `system.resolve`, for paths its `uses` names): a file's text, the names in a folder, and where a path leads once its links are followed. Ectomatter follows every path's links before reading it, and refuses one that leads outside what the plugin may read.
- **NVIDIA's driver** (`system.nvidia`, for a plugin that uses `"nvidia"`): what the driver says of the card at a PCI address, `"probe"` (what it reports, and its video memory in all) or one reading.
