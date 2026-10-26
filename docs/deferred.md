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

- A core's update runs from Ectomatter's next start, with nothing said: its
  device's page holds the core it started with (`front.rs`, the updates
  loop).
- The app's tests of the Z10's core check it against the original editor
  with the Z10's driver crate (a dev-dependency only: `apps/ectomatter/Cargo.toml`,
  `core_host.rs` tests, `lcd_parity.rs`); they belong with the core once it
  has a repository of its own.
- The keyboard page still holds the Z10's: its board's regions and the
  screen's fonts built in (`overlays.rs`, `lcd_draw.rs`, the knobs
  `control_117`/`control_118` in `overlays.rs`), its screen's built-in apps
  (`display.rs:13`), its screen's own status screen, four rows as its firmware
  draws them (`lcd_page.rs`, `status_rows`), and the Z10's own setting ids
  (`device.rs`: `dimmer`, `modes`, `displays/light`, `displays/apps`). The
  page's module is still named for it (`z10.rs`, `z10::Page`).
- A key's box on the page is given in the description (`box`, EVGA's own
  editor's), not found from its piece in the art.
- The layouts Ectomatter ships are laid out on the Z10's 20 by 4
  (`lcd_page.rs`, `LAID_OUT_ON`, kept as data, as a saved layout's places
  are): a screen of another size shows them cut off at its edges, or with room
  to spare, and nothing lays them out anew for it. (The screen's size itself
  is the one its core describes, `lcd_page::size`: boundary
  `the_screen_is_the_size_its_core_describes`.)
- The keyboard is one: `cores::keyboard()` is the first core describing a
  keyboard, and its page, screen and counts are that one's. A second
  keyboard's core starts but has no page.
- Devices are kept by their cores' device ids with no instance in them
  (`z10`, `sensei310`): two of the same model would share their jobs, scenes,
  Library choices and place on the desk.
- No test runs a keyboard core describing another screen size end to end:
  the keyboard is one (`cores::keyboard()`, the Z10's in tests), so the
  player's frames, the keyboard picture's panel, the Screen tab and the
  window's grid are only run at 20 by 4. The other sizes are tested through
  the screen engine's functions (`lcd_fx`, `screen_edit`, `lcd_page`).
- The boundary check `the_screen_is_the_size_its_core_describes` looks for
  the old spellings of 20 by 4 (`const COLS`, `; 20]`, `take(20)`), not every
  way a size could be typed again.
- A saved layout with blocks beyond the screen (hand-edited or imported) now
  keeps them where they were saved and draws them cut at the screen's edges;
  before, it moved them onto the screen. The editor can't make one.
- The Library's cards draw a layout's picture at the Z10 screen's shape
  (260 by 52, `front.rs`, `app.slint`); another shape is letterboxed.
- The heartbeat's trace takes its width from the first time it's drawn
  (`lcd_fx::pulse`); drawn first at 0 by 0, it starts flat.
- A block past the screen's edge gets an outline clamped to the last row or
  column in the editor, over whatever block is there.
- The mouse is one, with two lights: `mouse::shape()` is the first core
  describing a mouse, and the picture and the scenes keep two lights' colors
  (`mouse_art.rs` `[Lit; 2]`, `front.rs` `scene_colors`, `scene_mouse`,
  `mouse.rs` `plays`). A mouse with another number of lights shows only two.
- A mouse's callouts are in its mock's canvas (`callout`), not placed from
  its buttons' pieces.
- The mouse fixture the offscreen renders read was recorded from an older
  core; it's kept current by hand (the scratchpad's his-mouse-state3.json).
- One run of `tests/stranger.rs`, beside another full build, saw the app stop
  silently after its first lines; twelve runs since didn't. The test prints
  how the app ended now.
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
- The window's own thread still calls plugins for a page's sections, a
  game's changes and its start (`hosted.rs` `sections`, `changes`,
  `launch`), each waiting up to three seconds (`WINDOW_WAITS`), not off the
  thread.
- A package stuck inside one of Ectomatter's own calls (a USB read, a web
  fetch) isn't stopped by its time until it's back in its own code; its
  caller stops waiting after a minute (`Limits::wait`).
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

- The window, tray, file picker, macro typing and recording, the app in front
  and sound are Linux only and always built (`main.rs`, `tray.rs`,
  `picker.rs`, `typist.rs`, `recording.rs`, `kde_output.rs`, `sound.rs`); the window refuses to open
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
