# Ectomatter

One app for the devices on your desk. It sets up each one's keys, lighting,
screen and settings, switches every device's profile with the game or app in
front, and changes the computer around a game while it runs, putting it all
back when the game quits.

Ectomatter is an independent project. It is not made, endorsed or supported by
EVGA, SteelSeries or any other maker whose devices it drives.

## How it's put together

- **The app** (`apps/ectomatter`), in Rust and Slint. It draws every page and
  keeps the profiles, the macros, the Library of games and apps, and the record
  of everything it changes while a game runs.
- **Cores** (`cores/`), one for each device Ectomatter talks to itself: the
  EVGA Z10 RGB keyboard and the SteelSeries Sensei 310 mouse. A core is a
  WebAssembly module that reaches only the USB devices its manifest names, and
  it describes its device in data. The app does the drawing.
- **Plugins** (`plugins/`), which add to the app's pages or bring in another
  program: the Game Configurator sets how each game starts; the Monitor
  Switcher sets each monitor while a game runs; OpenRGB Link switches OpenRGB's
  lighting profiles with the game in front; the Weather Feed brings the weather
  and its warnings to the screens that show them; and the Sensor Feed reads the
  computer's temperatures, loads and fans for them, on Linux.
- **The core protocol** (`docs/core-protocol.md`, and the WebAssembly interface
  in `wit/core.wit`): how the app talks to its cores and plugins.
- **Libraries** (`crates/`): `evga-z10`, the keyboard's own code;
  `ectomatter-draw`, which draws the devices and their light;
  `ectomatter-protocol`, the names every core shares; and the weather and the
  sensors code the Weather Feed and the Sensor Feed run on, `ectomatter-weather`
  and `ectomatter-sensors`.
- **Design** (`design/`): the styles and data the design mocks are drawn with.

## Status

Ectomatter runs on Linux (KDE Plasma on Wayland). Windows and macOS are planned,
and so is a core for the MagTran M3.

## Building

The cores and plugins build for WebAssembly
(`rustup target add wasm32-wasip2`), the app for the computer it runs on.

```sh
tools/build-cores.sh                  # each core's module, beside its manifest
tools/build-plugins.sh                # each plugin's module
cargo build --release -p ectomatter
cargo test --release -p ectomatter
./target/release/ectomatter
```

## The keyboard's library

`evga-z10` finds the Z10 through hidapi by its LED-indicator collection
(usage page 0x08, usage 0x4B), which carries the keyboard's reports 3 to 10.
The protocol was worked out from EVGA's Unleash RGB 1.0.11.0 and, for the
display, from AIDA64.

The firmware updater's commands are deliberately not part of this crate.

```sh
cargo test -p evga-z10                     # the parity tests
cargo run -p evga-z10 --example state      # print what a connected Z10 holds (reads only)
```

### Parity

The library replaces an earlier Python editor. `parity/gen.py` runs that
editor's code on thousands of cases and records what it produced: decoded
profiles and every kind of settings edit, preview frames, macros both ways,
display frames, recordings, and every exchange with the keyboard for each
command and editor action. `cargo test -p evga-z10` replays them all, and every
byte has to match.

The committed vectors are synthetic. Vectors made from a real keyboard's
backups go in `parity/private/`, which is not committed, and are checked too
when present.

## License

Copyright (C) 2026 Ectonomic LLC. Made by Grant Garrison and Claude.

Ectomatter, its cores and its plugins are free software under the GNU
General Public License, version 3 (`LICENSE`): you may share and change
them under its terms. They come with no warranty. The app draws its windows
with [Slint](https://slint.dev), which it takes under the same license.
The libraries it's built from and the typefaces and files it carries from
others are under their own licenses, named in `REUSE.toml` and listed with
their texts in the `NOTICES` the install writes.
