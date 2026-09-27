# Ectodeck core protocol

Draft 1, 2026-09-26. For review before anything is built on it.

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

Each core is a folder with a manifest, `core.json`:

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
  "matches": [{ "usb": { "vendor": "3842", "product": "2611" } }]
}
```

`matches` lets the frontend start a core only when one of its devices is plugged in, the way RetroArch picks a core for a file. A core that finds its devices another way, such as the OpenRGB core, which asks the OpenRGB server, declares `{ "always": true }` instead.

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
  "firmware": { "version": "1.171", "official": false, "note": "Unofficial build 1 of EVGA's 1.17" },
  "picture": { "art": "art/board.svg", "top": "art/board-top.svg", "width": 661, "height": 262 },
  "controls": [
    { "id": "esc", "kind": "key", "label": "Esc", "legend": { "main": "ESC" }, "rect": [54, 60, 27, 27], "light": true },
    { "id": "volume", "kind": "slider", "label": "Volume", "rect": [582, 36, 50, 6] },
    { "id": "lcd", "kind": "display", "label": "Display", "rect": [300, 20, 84, 22] }
  ],
  "capabilities": { "keys": { }, "macros": { }, "lighting": { }, "displays": { }, "profiles": { }, "settings": { } }
}
```

- **picture** is the device's drawing, shipped in the core's folder, with every control's `rect` in the drawing's own units. This is how the frontend draws the Z10 today, with its keys laid over its artwork, and it is how it will draw a mouse or a deck.
- **controls** lists every part of the device a capability can refer to. Controls have stable ids, so settings stay attached to the right key across versions of a core.
- **capabilities** says which capabilities the device offers, each with its own description (section 5). A device offers only what it really has.

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

A deck whose keys each have their own small screen offers one display per key: `{"control": "k0", "kind": "image", "width": 72, "height": 72}`. A deck like the M3, whose keys are windows onto one screen behind them, offers that screen: `{"control": "panel", "kind": "image", "width": 854, "height": 480, "windows": [...]}`, where `windows` is the geometry Ectodeck already uses to place each key's image and to line a background up behind the keys. `display/show {device, control, text | image}` puts something on it, where an image is a PNG. The frontend decides what to show, such as system stats, key images or a background, so every display on the desk can show every kind of content.

### input: presses, turns and slides

Notifications from the core whenever a control is used: `input/down`, `input/up`, `input/turn {ticks}`, `input/slide {value}`, `input/touch {x, y}`. Devices run by the host, such as the M3, depend on these entirely. Devices with their own memory, such as the Z10, send them only for controls assigned to `{"host": ...}`, so a keyboard key can trigger a frontend action without the keyboard losing its own assignments.

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
| `duration` | a slider in seconds, stored in milliseconds | Wave speed |
| `choice` | a segmented control or menu, with `options` | Wave direction |
| `color` | a colour swatch | Static colour |
| `colors` | a row of swatches, up to `max` | Wave colours |
| `text` | a text field, with `max` length | Profile name |

A core cannot send its own interface. If a device needs something this language cannot describe, the language grows in a new protocol version, so every core gains the new control at once.

## 7. What the frontend adds across every device

These are why the platform is more than a set of separate apps:

- **Profiles that follow the app in front,** switching every device together.
- **One Macro Editor,** for devices that store macros and for devices whose macros the frontend plays.
- **Lighting across the whole desk,** drawn once by the frontend and streamed to every device that offers `direct`.
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

## 9. Versions

`protocol` is one whole number. A frontend supports a range of versions and says which it speaks in `initialize`; a core that cannot speak it declines. Within a version, new optional fields may appear and must be ignored by anyone who does not know them.

## 10. Open questions

1. **Who draws a deck's screen.** Today Ectodeck's M3 plugin composes the whole 854 by 480 screen itself (key images, labels and animated backgrounds) and sends it. Should the frontend compose it and send finished images (so every deck and every display draws the same way, and backgrounds become a frontend feature for any screen), or should the deck core keep composing? The draft says the frontend composes, which moves the background renderers into the frontend.
2. **Discovery on Windows and macOS.** `matches` by USB id works everywhere, but a core may still need the system's permission first (Input Monitoring on macOS). The frontend should explain that step itself rather than leave it to each core.
3. **A catalogue of cores.** Bundled cores are enough to start. Downloading cores later means signing them, so a core's source is known before it runs.
4. **Recording on Windows.** Taking one keyboard's input for a recording has no clean route on Windows; the Z10 core may have to record without keeping keys from other apps there.
