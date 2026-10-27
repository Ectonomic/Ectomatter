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
  draws them (`lcd_page.rs`, `status_rows`), and the Z10's own methods for
  its screen's light and apps (`device.rs`: `displays/light`,
  `displays/apps`, which the protocol doc doesn't define). (Its sliders and
  backlight, its onboard layout's pieces and its status readings come from
  its description now: `display.rs`, `Slider`; `lcd_page.rs`, `onboard_can`.)
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
- The boundary check `every_core_starts_the_same_way` looks only for a
  second `Hidapi::new()` outside `core_host.rs`, not every way a device
  could be opened again.
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
- The mouse lab's offscreen snapshot needs the keyboard's core: its readout
  is the keyboard's board's (`main.rs`, `snap_lab`).
- The Z10's core sends only its name in `device/added`; the app reads the
  description its manifest names (`deviceFile`) from the core's folder
  (`cores.rs`, `Core::load`), not from what the core tells.
- A core that's off is started alone for a moment as Ectomatter starts, with
  no device, to say its device's id (`main.rs`, `devices_described`), up to
  ten seconds each; its device's settings, jobs and Library choices are kept
  by that id. With no keyboard core at all, the keyboard's settings path has
  no id in it (`cores::keyboard_setting`, "devices..screen.pages"); nothing
  writes it, there being no screen page.
- Cores can't leave this repository: they depend on it by path
  (`cores/evga-z10/module/Cargo.toml:16,18`), and their built modules exist
  only after `tools/build-cores.sh` runs here.

## Stage 3: plugins on equal footing

- The window's own thread still calls plugins for a page's sections, a
  game's changes and its start (`hosted.rs` `sections`, `changes`,
  `launch`), each waiting up to three seconds (`WINDOW_WAITS`), and starts a
  package turned on or added, waiting up to ten for it to be made ready and
  met (`START_WAITS`) and three for a core to describe its devices; none of
  it is off the thread.
- A package stuck inside one of Ectomatter's own calls (a USB read, a web
  fetch) isn't stopped by its time until it's back in its own code; its
  caller stops waiting after a minute (`Limits::wait`).
- Review looks up the latest release again: what it shows (and what Add
  then puts in place, held to the manifest shown) can be a newer version
  than the one the row said was waiting (`front.rs`, `Ctl::PackageReview`).
- A game's start holds what the plugin's manifest names (`launch.wrap`,
  `launch.env`); the floor under it is two lists Ectomatter keeps, of
  programs that run what they're given (`launcher.rs`, `runs_code`) and of
  variables that load code (`loads_code`), and one rule for list values (no
  setting naming a command, a file, a folder or a log: `plain_as`). Neither
  list can name every such program or variable; the person sees the
  programs by name before adding the plugin.
- Found by the audit of 2026-10-26 (evening), the same kind as the launch
  lists, and not fixed yet (the rest of what it found is fixed: a plugin's
  buttons say what they do, game-helper's import is gone, the Proton builds
  aren't sorted for the plugin, the screen finds its readings by part and
  kind, the variable form's words are the plugin's, the plugins' old ids
  are gone with their migrations, the held key isn't named for the Z10, and
  the name checks read every package's manifest):
  - game-helper's session note is read, for its launch steps on the screen
    (`lcd_player.rs`, `Core::new` with `z10-lcd/session`; game-helper's
    `lcd-announce.sh` still writes it when game-helper runs a game, and
    game-helper is still installed on Grant's computer).
  - The new sliders code (2026-10-26) was tested offscreen and by unit
    tests, not by moving the Z10's sliders: freeing a slider is now one
    call each (`input/free`) where it was one call for both, and a slider's
    job, its hold on the backlight and its notes come from the core.

## Stage 4: maintainable

- One object runs every page: `Front`, about 200 fields. Its code is split by
  page and part into `src/front/` (eleven files, 2026-10-26), but its state
  isn't: every part reaches every field, and no page's state is a type of its
  own. Its largest file is `front/mod.rs` (3,278 lines: the types, `new`, and
  what every page shares, such as acting on the window's controls).
- The main window's pages are components of their own (`ui/pages/`: Home,
  the keyboard's, the mouse's, the Library, Settings; 2026-10-26), but each is
  bound to the window property by property (the keyboard's page takes 190 of
  them): every page's state is still the window's, as `Front`'s is one
  object, and Rust sets it on the window. A page's own state as a type (a
  Slint global per page, a Rust type per page) waits for both.
- Untyped JSON at most file and protocol boundaries. A package's manifest is
  typed (`ectomatter_protocol::manifest`, every package's held to it by
  `every_packages_manifest_is_the_protocols`), though what reads it here
  still takes it as JSON in places (`packages.rs` `works_here`,
  `systems_of`, `can_words`; `updates.rs` `more_than`; a game's start,
  `launch`, inside it). Untyped yet: a device's description and its
  capabilities (section 4 and 5), every call's params and answer, a
  plugin's sections and cells, readings, and Ectomatter's own saved files.
- The render tests (`apps/ectomatter/tests/renders.rs`) draw in a sandbox
  (bubblewrap) on a made-up desk, but the sandbox lends them this system's
  programs, libraries and typefaces (/usr, /etc/fonts), so their pictures
  hold for a Linux like the one that approved them; another system draws no
  picture for them yet. They need `bwrap`, and a kernel that lets a user make
  namespaces.
- The render tests' pages are a list typed in `renders.rs`, with the clicks
  that reach a tab or a box by where it is. A check holds every place of the
  window to a page drawn (`every_place_of_the_window_is_drawn`), but a new tab,
  box or dialog isn't drawn until someone adds it. A click that misses draws
  the page as it was (the lab's slider case did, and was taken out).
- A snapshot settles (`main.rs`, `settle`) by waiting, in real time, until no
  package has told anything new for a second and a half, up to twenty
  seconds: a plugin whose readings keep changing (on a computer that isn't
  made up) never settles and is drawn at twenty seconds.
- The design checks skip 773 lines as art, capped at that (`tokens.rs`,
  `ART_LINES_AT_MOST`): 501 marked `// art:` (and its regions) and the rest the
  parts named in `ART`, whole. Many aren't a device's own drawing but a page's
  pieces drawn by hand: the playlist's strip (73), the selected block's
  corner (30), the edit mode's grips (26), a macro step (25), the layer tabs
  (20), a lab slider's row (20), a layout's card (17); and of `ART`, the
  glyphs (`WayGlyph`, `BlockGlyph`, `MacroIcon`). Each is a part to make, and
  the cap comes down as they go.
- The log is stderr itself only on Linux and macOS (`log.rs`,
  `become_stderr`); on Windows nothing is written to the file yet. The lines
  aren't stamped with their time.
- Every package's compiled module is kept until Ectomatter quits
  (`core_host.rs`, `compiled`), so a session that takes in updates keeps one
  more module for each. It's there because package threads still run while
  the process exits (nothing stops and waits for them first); a module let go
  by the last of them then aborted the process. Both core dumps of
  2026-10-26 were this, in snapshots (one the stranger test's run that
  stopped silently); no test makes it happen on purpose.
- A device's page opened while its device isn't plugged in says it's reading
  the device ("Reading your keyboard"), and its button stays on the rail:
  the first-run render shows it (`first-keyboard`).

## Before any repository goes public

- The tracked files hold Grant's own details, in tests and comments: his
  town and its coordinates (`hosted.rs`, the weather tests), his monitors'
  names and EDIDs (`hosted.rs` `Desk`, `monitors.rs`), his drives' serial
  numbers (`crates/ectomatter-sensors/src/fake.rs`, `list.rs`), his
  computer's name (`macro_page.rs`, `jobs.rs`, `setups.rs`, `described.rs`,
  `picker.rs`, `crates/ectomatter-draw/src/hdr.rs`; "studio" six times in
  `hosted.rs`), his home folder's path (`plugins/game-configurator/module/src/lib.rs`,
  `hosted.rs` near line 1043, `picker.rs` near line 85), his OpenRGB
  profiles and games ("ARC Raiders", "Deepdriver II" in `hosted.rs`). The
  sensors crate's made-up computer is his (`fake.rs`: his processor, board,
  cards and two drives' serial numbers); the render tests rename all of it.
  Found with `git grep` and a review on 2026-10-26. The history holds them
  too.

## Stage 5: Windows and macOS

- The winit shell (`src/shell/winit.rs`, `ECTOMATTER_SHELL=winit` on Linux,
  the shell on every other system) shows the main window only. Not in it
  yet: the windows of their own (the Macro Editor, the lab, the screen
  layout editor: it says so in the log), the clipboard, drags, closing into
  the tray (closing quits), the window's rounded corners and shadow.
- The winit shell takes the desktop's white from the Wayland screens
  (`main.rs`, `levels`) and gives the screen's peak and the brightest
  screen's as 1000 nits (`winit.rs`, `draw`); on Windows and macOS they're
  to come from wgpu's `Surface::display_hdr_info`.
- On Linux the winit shell's HDR goes through the Vulkan driver's own
  description of the surface (Mesa's WSI attaches color management itself),
  not Ectomatter's: the desktop's white and the peak aren't told to the
  compositor as the Wayland shell tells them (wgpu issue #10545).
- The winit shell was run on Linux under lavapipe (Vulkan on the CPU, no
  GPU), with no cores, and drew Home; nobody has pressed or typed in it, and
  it hasn't run on Windows or macOS. The app doesn't build there yet: its
  Linux-only libraries (Wayland's, D-Bus's, the tray's) aren't kept to
  Linux in `apps/ectomatter/Cargo.toml`, and no cross-check runs here (no
  cross C compiler for hidapi and ring).

- The tray, file picker, macro typing and recording, the app in front and
  sound are Linux only and always built (`tray.rs`, `picker.rs`,
  `typist.rs`, `recording.rs`, `front_app.rs`, `kde_output.rs`, `sound.rs`).
  The Wayland shell refuses to open without the compositor's color
  management (`shell/wayland.rs`, `run`); the winit shell doesn't need it.

## Stage 1: installable on Linux

- No install layout, release build of the cores, udev rule or desktop entry.
- The lab saves into the source folder (`lab.rs:2181`), over what the core
  ships.
- A damaged settings file is read as empty and replaced (`settings.rs:22`).
  Safe writes are copied in nine places; no saved file has a version.
- License gaps: Slint's attribution, the fonts' licenses.
- The GitHub repositories exist in the Ectonomic organization
  (Ectonomic/Ectomatter and one per package, `ectomatter-<package id>`,
  public and empty since 2026-10-26); nothing is in them, and no package's
  manifest names its repository yet.
