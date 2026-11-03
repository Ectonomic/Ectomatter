<p align="center">
  <img src="packaging/com.ectonomic.ectomatter.svg" width="88" alt="">
</p>

<h1 align="center">Ectomatter</h1>

<p align="center">
  <strong>One app for the devices on your desk.</strong><br>
  The keys, lights and screens of your keyboard and mouse in one place,<br>
  and a desk that changes for each game you play.
</p>

<p align="center">
  <a href="#whats-in-it">What's in it</a> &nbsp;·&nbsp;
  <a href="#cores-and-plugins">Cores and plugins</a> &nbsp;·&nbsp;
  <a href="#getting-it">Getting it</a> &nbsp;·&nbsp;
  <a href="#how-its-built">How it's built</a>
</p>

<br>

<p align="center">
  <img src="docs/pictures/readme-hero.webp" width="100%" alt="Ectomatter's Home: an EVGA Z10 RGB keyboard with a wave of orange and teal rolling across its keys, beside a SteelSeries Sensei 310 mouse with its logo lit teal">
</p>

Ectomatter draws each device on your desk as it really looks, lit as it's lit
right now, and gives everything it can do a place to be set: every key, every
light, the screen on your keyboard. Pick a game and choose what changes while
you play it. When you quit, Ectomatter puts it all back.

Every device and plugin is a package of its own, a WebAssembly module that
reaches only what its manifest names. Today Ectomatter drives the EVGA Z10 RGB
keyboard and the SteelSeries Sensei 310 mouse, and a new core is how another
device joins them.

## What's in it

### Your desk, as it is

Home shows your devices to scale. **Arrange** puts each one where it sits on
your real desk, so a **scene** can play one lighting effect across all of them:
a wave crosses the keyboard, then the mouse, the way it would cross the desk
itself.

<img src="docs/pictures/readme-scene.png" width="100%" alt="A new scene over Home: one wave of orange and blue crossing the keyboard and then the mouse, with its colors, direction and speed">

### Every light on every key

Static, breathing, pulse, spiral, wave, stars, reactive and per-key lighting
for the Z10, each with its own colors, brightness and speed, across the whole
keyboard or only the zones you pick: the F-keys, the number row, WASD, the
arrows, the numpad. The keyboard keeps five profiles, and its picture on screen
plays each effect as the keys play it.

<img src="docs/pictures/readme-lighting.png" width="100%" alt="The keyboard's Lighting page: the Z10 lit by a wave of orange and teal, its five profiles, the effects, the wave's seven colors, and its zones">

### Keys and macros

Any key can type another, play media, click like a mouse button, run a macro,
switch the profile or what the screen shows, or do nothing at all, with a
second layer of its own under Fn. The Macro Editor records what you type on
the Z10 and keeps the macro on the keyboard, so it plays even with Ectomatter
closed.

<img src="docs/pictures/readme-keys.png" width="100%" alt="The keyboard's Keys page with the E key chosen, its jobs (key, media, mouse, macro, profile, screen, keyboard, nothing) and the list of keys to give it">

<img src="docs/pictures/readme-macros.png" width="100%" alt="The Macro Editor: the keyboard's macros listed by name, one open with its six steps">

### The screen on your keyboard

The Z10's screen plays a playlist of layouts: the computer's temperatures and
loads, the time, the weather where you are, a heartbeat that races as the CPU
works harder, rain, a starfield, a campfire, an aquarium, bars that rise and
fall with the computer's sound. Make your own in the Screen Editor from text,
readings, bars and effects, and choose what the screen shows while a game
starts and as it quits.

<p>
  <img src="docs/pictures/readme-screen.png" width="49%" alt="The keyboard's Screen page: the screen showing the computer's temperatures, loads and memory, a playlist of three layouts, and the layouts to choose from">
  <img src="docs/pictures/readme-screen-editor.png" width="49%" alt="The Screen Editor: every layout listed, the Stats layout open in blocks of text, readings and bars">
</p>

### The mouse, too

The Sensei 310's eight buttons, its logo and wheel lights, its two sensitivity
settings and its polling rate, on pages like the keyboard's, with as many
profiles as you like, which Ectomatter keeps for it.

<img src="docs/pictures/readme-mouse.png" width="100%" alt="The mouse's Lighting page: the Sensei 310 with its logo lit teal and its wheel orange, and the effects to choose from">

### A desk for every game

The Library finds your Steam games and takes any you add by hand. Choose what
changes while you play one: the scene, each device's profile, the Proton build
and how it runs, gamescope, HDR, GameMode and MangoHud, and how your monitors
are set. Quit, and Ectomatter puts everything back.

<p>
  <img src="docs/pictures/readme-library.png" width="49%" alt="The Library: eight games, each with its cover">
  <img src="docs/pictures/readme-game.png" width="49%" alt="A game's page: its banner, the hours played, the scene and keyboard profile it switches to, and its Proton build">
</p>

## Cores and plugins

Ectomatter does the drawing and the keeping. What it can reach comes from
packages: a **core** for each device, a **plugin** for everything else. Each
runs inside Ectomatter as a WebAssembly module and reaches only what its
manifest names: a core, the USB devices it lists; a plugin, the folders,
programs and addresses it lists.

| | Package | What it does |
|---|---|---|
| Core | [EVGA Z10 RGB](https://github.com/Ectonomic/ectomatter-evga-z10) | Keys, lighting and the screen of the EVGA Z10 RGB |
| Core | [SteelSeries Sensei 310](https://github.com/Ectonomic/ectomatter-steelseries-sensei-310) | Buttons, lighting and sensitivity of the SteelSeries Sensei 310 |
| Plugin | [Game Configurator](https://github.com/Ectonomic/ectomatter-game-configurator) | Proton, prefixes, gamescope, GameMode and MangoHud for each game |
| Plugin | [Monitor Switcher](https://github.com/Ectonomic/ectomatter-monitor-switcher) | Brightness, resolution, HDR and more, set for each game |
| Plugin | [OpenRGB Link](https://github.com/Ectonomic/ectomatter-openrgb-link) | Lighting that OpenRGB controls, such as your RAM |
| Plugin | [Sensor Feed](https://github.com/Ectonomic/ectomatter-sensor-feed) | Your computer's temperatures, loads and fans, for screens that show them |
| Plugin | [Weather Feed](https://github.com/Ectonomic/ectomatter-weather-feed) | The weather and warnings where you are, for screens that show them |

Each lives in a repository of its own, linked here under `cores/` and
`plugins/`. In Settings, **Add a device core** and **Add a plugin** show what a
package can reach before you add it, and take it only if it matches the
checksum pinned in [Ectonomic's list of reviewed packages](https://github.com/Ectonomic/ectomatter-packages).
Ectomatter checks each package's releases once a day, and a version that can
reach more than the one you have waits for you to review it.

<img src="docs/pictures/readme-plugins.png" width="100%" alt="Settings, Plugins: the five plugins, what each reaches, and On and Off for each">

Want your device in Ectomatter? A core is how it gets there:
[Making a core or a plugin](docs/making-a-package.md).

## Getting it

Ectomatter runs on Linux, and is made and tested on KDE Plasma with Wayland.
Windows and macOS are planned. There's no release to download yet, so it's
built from source, with [Rust](https://rustup.rs):

```sh
git clone --recurse-submodules https://github.com/Ectonomic/Ectomatter
cd Ectomatter
rustup target add wasm32-wasip2
tools/build-cores.sh && tools/build-plugins.sh
cargo build --release -p ectomatter
```

On Linux, a program can open a USB device only where a udev rule lets it.
Ectomatter writes the rule for the devices its cores reach:

```sh
./target/release/ectomatter --udev-rules | sudo tee /etc/udev/rules.d/70-ectomatter.rules
sudo udevadm control --reload && sudo udevadm trigger
./target/release/ectomatter
```

Or make an RPM, which installs Ectomatter, its packages and the rule together:
`tools/package-rpm.sh`, then `sudo dnf install target/rpm/ectomatter-*.rpm`.

## How it's built

Ectomatter is written in Rust and draws its windows with
[Slint](https://slint.dev). Its packages run as WebAssembly components in
[Wasmtime](https://wasmtime.dev). How the app and a package talk is in
[docs/core-protocol.md](docs/core-protocol.md) and [wit/core.wit](wit/core.wit).

What it promises is tested from the outside:

- **Boundaries.** Each boundary the app claims (what a package can reach, what
  it can't say about itself, what the app mustn't know about any one package)
  has a test that tries to cross it with a made-up package.
  `tools/boundaries.sh` runs them all.
- **Pictures.** Every page is drawn offscreen in a sandbox, on a made-up desk,
  and held pixel for pixel to its approved picture. The pictures in this README
  are drawn the same way, and a change to a page they show fails until they're
  drawn again.
- **A stranger's computer.** The release build runs away from its source, with
  fresh settings and no devices, and installs as a Linux system lays a program
  out.

## License

Copyright (C) 2026 Ectonomic LLC. Made by Grant Garrison and Claude.

Ectomatter is free software under the GNU General Public License, version 3
only ([LICENSE](LICENSE)): you can share and change it under its terms. It
comes with no warranty. It draws its windows with Slint, which it takes under
the same license. The libraries it's built from and the files it carries from
others are under their own licenses, named in [REUSE.toml](REUSE.toml) with
their texts in [LICENSES/](LICENSES), and listed in the `NOTICES` the install
writes.

Ectomatter is an independent project, not made, endorsed or supported by EVGA,
SteelSeries or any other maker whose devices it drives.
