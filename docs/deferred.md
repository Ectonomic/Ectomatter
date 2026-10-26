# What's put off

Everything Ectomatter claims but doesn't do yet, and everything stubbed, faked
or hardcoded "for now", with where it is. Every report on Ectomatter starts
from this list. An item leaves the list only when the code holds and a test
proves it (`tools/boundaries.sh` shows which boundary tests pass).

Found by the architecture audit of 2026-10-26 (at 814f96c). The stages run in
the order Grant set: a device is only a core, plugins on equal footing,
maintainable, other systems, and installable on Linux last. Two items of the
installable stage came first because a device can't be only a core without
them: finding packages at run time, and starting without the Z10's core
(both done: `tests/stranger.rs`).

Paths are under `apps/ectomatter/src/` unless they say otherwise.

## Stage 2: a device is only a core

- A core started like any other gets no USB: `hosted.rs:199` (`NoDevices`).
  The Z10's and the mouse's cores are started by name instead
  (`hosted.rs:141-143` `started_elsewhere`, `main.rs:1937-1938`,
  `main.rs:1969-1972`, `main.rs:583`), with switch keys of their own
  (`packages.rs:147-153`).
- The app links the Z10's driver (`Cargo.toml:13`) and uses it outside tests
  in 13 places: `display.rs:96`, `lcd_player.rs:49`, `lcd_page.rs` (8 uses of
  `lcd::lcd_text`), `layout_page.rs:905`, `macros.rs:345`,
  `macro_page.rs:126,239,1012,1028`, `z10.rs:636,1162`, `front.rs:5265,7867,9350`,
  `main.rs:671`, `overlays.rs:97`.
- Macro recording reads the Z10 through Linux's input devices, by the Z10's
  ids, outside the core: `crates/evga-z10/src/recorder.rs:333-368`, driven
  from `macro_page.rs:994-1027`.
- The keyboard page is the Z10's: 5 profiles (`z10.rs:18`, `z10.rs:570`,
  `lcd_page.rs:946`, `layout_page.rs:96`, `front.rs:1631,3498,3555`), 116 keys
  (`z10.rs:540,1218,1230`, `lighting.rs:297`), the Z10's geometry built in
  (`z10.rs:459-461`, `overlays.rs:92-97`, `lcd_draw.rs:18`), its firmware table
  (`front.rs:7867`), its screen's built-in apps (`display.rs:13`).
- The screen is fixed at 20 by 4 (`lcd_page.rs:27-28`, `lcd_anim.rs:13-14`,
  `layout_page.rs:555,906`, `screen_edit.rs`, `lcd_player.rs:94`) and its
  character set at the Z10's (`lcd_player.rs:49`), though the core describes
  its displays.
- The mouse page is the Sensei 310's: its device id (`mouse.rs:22`), buttons
  (`mouse.rs:178-187`), lights (`mouse.rs:470`), callout positions
  (`mouse.rs:192-206`), and its picture read by Inkscape's shape ids
  (`mouse_art.rs:19-42`), not the pieces the description names. Its light and
  art are read from `cores::dir()`, not its package (`mouse.rs:1227`,
  `front.rs:5582`, `main.rs:3885-3890`). Fallback names and sizes typed in:
  `front.rs:5923-5936,6733-6735`, `main.rs:3886`.
- Pages are found by device name: `place == "z10"` / `"mouse"` in
  `ui/app.slint:1568,1576,2447,3118,4478`, 58 literals in `front.rs`,
  `main.rs:1395,2451`. Saved jobs, scenes and Library choices key devices as
  `"z10"` and `"sensei310"` (`jobs.rs:23-24`, `front.rs:6569,6573,8811`,
  `launcher.rs:916`).
- The mouse lab's offscreen snapshot needs the keyboard's core: its readout
  is the keyboard's board's (`main.rs`, `snap_lab`).
- The Z10's core sends only its name in `device/added`; the app reads
  `device.json` from disk (`cores.rs:25`) though the core's describe names it
  (`cores/evga-z10/module/src/lib.rs:379`).
- Cores can't leave this repository: they depend on it by path
  (`cores/evga-z10/module/Cargo.toml:16,18`), and their built modules exist
  only after `tools/build-cores.sh` runs here.

## Stage 3: plugins on equal footing

- Readings come from two plugins by id: `lcd_stats.rs:24` (`sensor-feed`),
  `front.rs:10496` (`weather-feed`), `front.rs:6791-6794`. The two send
  `readings/list` in different shapes (`plugins/weather-feed/module/src/lib.rs:204`,
  `plugins/sensor-feed/module/src/lib.rs:90`).
- No time limit on any call (`core_host.rs:631-640`), no CPU or memory limit
  (`core_host.rs:667`, `Engine::default()`). Calls that block the window:
  `hosted.rs:227,312,343,377`, from `front.rs:6630,7078,7137`.
- Nothing restarts a core or plugin that crashed (`core_host.rs:709`), though
  `docs/core-protocol.md` section 3 says the app does.
- The launch permission runs what the plugin hands it: `apps/ectomatter-launch`
  accepts any program for `wrap` (`:122-127`, `:384-389`), any folder holding
  a `proton` file (`:266-270`), and any environment (`:136-156`).
- Updates don't ask again when a plugin wants more, and aren't checked against
  a checksum (`updates.rs`).

## Stage 4: maintainable

- One object runs every page: `front.rs`, 10,506 lines, about 200 fields.
- One window component holds every page: `ui/app.slint`, 7,236 lines.
- Untyped JSON at every file and protocol boundary.
- No test runs the whole app and compares its renders with approved pictures.
- The design checks skip lines marked as art, with no cap on how many.
- No log file.

## Stage 5: Windows and macOS

- The window, tray, file picker, macro typing, the app in front and sound are
  Linux only and always built (`main.rs`, `tray.rs`, `picker.rs`,
  `typist.rs`, `kde_output.rs`, `sound.rs`); the window refuses to open
  without KWin's color management (`main.rs:1876,1885`).

## Stage 1: installable on Linux

- No install layout, release build of the cores, udev rule or desktop entry.
- The lab saves into the source folder (`lab.rs:2181`); Grant's uncommitted
  tuning (`cores/evga-z10/device.json`, `cores/steelseries-sensei-310/light.json`)
  belongs in his data folder over what the core ships.
- A damaged settings file is read as empty and replaced (`settings.rs:22`).
  Safe writes are copied in nine places; no saved file has a version.
- License gaps: Slint's attribution, the fonts' licenses, the Z10's screen
  font read from EVGA's firmware.
- The GitHub repository the app checks for updates doesn't exist yet.
