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
    game-helper is still installed where it was used).
  - The new sliders code (2026-10-26) was tested offscreen and by unit
    tests, not by moving the Z10's sliders: freeing a slider is now one
    call each (`input/free`) where it was one call for both, and a slider's
    job, its hold on the backlight and its notes come from the core.

## Stage 4: maintainable

- Quitting can wait longer than its four seconds: the loop's wait doesn't
  count the time left, nor the sliders' 300 ms release
  (`shell/session.rs`, `step`; `shell/wayland.rs`, `run`).

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
  `launch`, inside it). A device's description is typed at its top, its
  firmware, its controls and its screens (`ectomatter_protocol::device`,
  every core's held to it by `every_devices_description_is_the_protocols`),
  but the app still reads it as JSON everywhere, and its print, its light
  and each capability's own description are untyped. Untyped too: every
  call's params and answer, a plugin's sections and cells, readings, and
  Ectomatter's own saved files.
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
- A keyboard read once and then unplugged says so when its core tells
  `device/removed` (`front/mod.rs`, `retry`, the keyboard's thread's
  `there`), and is read again when it tells `device/added`; no test unplugs
  one (the renders draw a read that failed, `kb-unplugged` and `kb-stopped`,
  not the core's telling).

## Before any repository goes public

- Grant's own details (his town, his computer's name, his home folder's
  path, his monitors' names, his drives' serial numbers, his OpenRGB
  profiles' names) were replaced with made-up ones on 2026-10-27, in the
  files and through the whole history. `tools/private-words.sh` holds every
  file (and, with `--history`, every commit) to a list kept outside the
  repository (`~/.config/ectomatter-dev/private-words`); the commit hook and
  `tools/boundaries.sh` run it. Still his own: the made-up computer in
  `crates/ectomatter-sensors/src/fake.rs` copies his parts' models (they
  test real drivers' names, and a part's model names no one), and the
  copyright and the keyboard's drawing carry his name, as an author's do.

## Stage 5: Windows and macOS

- The winit shell (`src/shell/winit.rs`, `ECTOMATTER_SHELL=winit` on Linux,
  the shell on every other system) has the main window and the windows of
  their own (drawn in SDR through softbuffer, as the Wayland shell's are in
  SDR), and the system's own clipboard (arboard, `clipboard.rs`,
  `use_system`: built, not yet copied or pasted with, as that would take
  the clipboard of the computer it was tried on). Not in it yet: drags,
  closing into the tray (closing quits), the windows' rounded corners and
  shadow, and a resize from a window of its own's edges.
- The winit shell takes the screen's white and peak from wgpu's
  `display_hdr_info` where the system says (Windows, macOS), once, as the
  window opens (`winit.rs`, `show`): not again when the window moves to
  another screen or the screen's HDR is switched. On Linux it keeps the
  levels read from Wayland's screens (`main.rs`, `levels`), the peak at
  1000 nits. It gives no HDR10 metadata.
- The winit shell's keys: dead keys and an input method's text, Insert and
  the F-keys' own text don't reach the page (`winit.rs`, `key_text`); the
  window's focus and its being minimized aren't told to Slint. On macOS,
  where winit can't resize from an edge, a press within 6 points of one is
  taken for a resize that doesn't happen (`winit.rs`, `edge`).
- No test runs a session with a shell of its own: `Session::start`, `take`
  and `step` are tried only through the two real shells (`shell/session.rs`).
  A sender's wake is tested (`shell/mod.rs`).
- On Linux the winit shell's HDR goes through the Vulkan driver's own
  description of the surface (Mesa's WSI attaches color management itself),
  not Ectomatter's: the desktop's white and the peak aren't told to the
  compositor as the Wayland shell tells them (wgpu issue #10545).
- The winit shell was run on Linux under lavapipe (Vulkan on the CPU, no
  GPU), with no cores, and drew Home; nobody has pressed or typed in it, and
  it hasn't run on Windows or macOS. It type-checks for both
  (`tools/check-systems.sh`), but has never been linked for either: the C
  code of hidapi and ring goes to a stand-in compiler there.

- On Windows and macOS these do nothing yet, a stand-in in each file's place
  (`#[cfg(not(target_os = "linux"))]`): the tray (closing quits) and the
  desktop's notices (`tray.rs`), the file picker (none chosen, `picker.rs`),
  typing a macro's keys (`typist.rs`), the app in front (`front_app.rs`),
  NVIDIA's readings (`nvml.rs`), the launcher's wait for a game's changes
  (`launcher.rs`, `listen_at`), a stop asked for by the system (`main.rs`,
  `on_term`, Unix's signals), and the time of day as local (`lcd_page.rs`,
  Windows' `localtime_s`, untried). A folder is opened with `explorer` or
  `open` (`picker.rs`, `show_folder`, untried); the runtime folder is the
  system's temporary one off Unix, and a note's inode is 0 there
  (`lcd_player.rs`, `runtime_dir`, `read_event`). Recording keys and capturing sound read
  Linux's devices and programs (`recording.rs`, `sound.rs`) and have no
  stand-in yet; they build because they find nothing.
  The Wayland shell refuses to open without the compositor's color
  management (`shell/wayland.rs`, `run`); the winit shell doesn't need it.
- Start at login (Settings, Startup; `autostart.rs`) writes a value under
  the Run key on Windows (with `reg`, and clears Task Manager's turning it
  off) and a launch agent in `~/Library/LaunchAgents` on macOS. Neither has
  run on its system: the text each writes is tested on Linux
  (`each_systems_entry_starts_the_program_it_names`), the code type-checks
  (`tools/check-systems.sh`). macOS's Login Items list shows a launch agent
  under Allow in the Background, not as a login item; a login item proper
  is `SMAppService` (macOS 13), which needs Ectomatter as an app bundle. With
  no tray in the winit shell, Ectomatter started at login there shows its
  window (`--hide` needs the tray).
- The winit shell still needs its display at its start (`winit.rs`, `run`):
  Windows and macOS have one before any program starts at login, but
  `ECTOMATTER_SHELL=winit` on Linux before the compositor is ready ends.

## Stage 1: installable on Linux

- Ectomatter started at login before KWin had told systemd where its display
  is (Fedora KDE 44, 2026-10-28: Plasma's login starts programs then; KDE's
  own video bridge failed the same way, and Fedora's forum has Thunderbird and
  Yakuake doing it) is met by ordering the login entry's unit after KWin's
  (`autostart.rs`, `ORDER`), Fedora's forum's fix for the video bridge. On a
  desktop other than Plasma whose login does the same, Ectomatter starts with
  no `WAYLAND_DISPLAY` and waits without its window for good, saying so in
  its log (`shell/wayland.rs`, `wait`). The bug isn't reported to Fedora or
  KDE: that waits for Grant (his account). The order's fix is tested in its
  writing (`starting_at_login_is_written_where_the_desktop_reads_it`), not
  by logging in.
- The tray's icon waits for a desktop's tray that comes after Ectomatter, as
  at login (`tray.rs`, `assume_sni_available`); no test has a desktop's tray
  come late. A window of its own (the Macro Editor's, the lab's, the screen
  layout editor's) is made again after the compositor goes and comes back
  (`shell/wayland.rs`, `open`); `tests/compositor.rs` tries the main window
  only.

- The list of packages Ectonomic reviewed (`registry/`, Grant's go
  2026-10-28) isn't published: its repository, Ectonomic/ectomatter-packages,
  doesn't exist, so Add a core and Add a plugin say Ectomatter can't read the
  list (`front/package_browser.rs`) until it does, and the daily check finds
  no listed versions (`updates.rs`, `check_all`). The list pins packages
  built on this computer (`tools/list-packages.py`); none of Ectonomic's
  packages has a GitHub release yet, so `registry/check.py` without
  `--releases` fails on each of them.
- `registry/check.py` takes a release's files as given: nothing rebuilds a
  package from its repository's source to see the release is what that
  source makes. Nothing is signed either: the checksums pinned in the list
  catch a release changed after review, as long as the list's own
  repository isn't changed.
- Add a core and Add a plugin show every listed package of the kind in one
  well, with no search and no scrolling; a long list would run off the
  dialog (`ui/app.slint`, `gl-modal == 6`).
- Adding a package from the list runs untested past choosing it: the renders
  draw the list and a package chosen (`add-core`), and the parts are tested
  on their own (`registry.rs`, `permission.rs`), but nothing presses Add and
  follows the package in and the password prompt.
- No template repository for someone making a package: `docs/making-a-package.md`
  points at the packages in this repository.
- `tools/install.sh` lays Ectomatter out under a prefix (its programs, its
  packages as installed, a desktop entry and its icon, udev's rule made from
  the cores' manifests, the licenses), tried by `tests/install.rs`, and
  `tools/package-rpm.sh` makes an RPM of that (`packaging/ectomatter.spec`,
  Grant's pick, 2026-10-27), tried by `tests/rpm.rs`. The RPM is made from
  the programs built on this computer, for its system (Fedora 44, x86_64):
  there is no source RPM (Cargo's libraries aren't vendored for a build
  without the network), it isn't signed, and no other system's package
  (deb) is made. udev reads the rules again where the system does that for
  every package (Fedora's does); `%udev_rules_update` asks it to elsewhere.
  An install by `tools/install.sh` alone has nothing that removes it, and
  the rule for a core added later isn't written (`main.rs`, `udev_rules`,
  run once, at install).
- Installed (not run from the source folder), the lab saves a core's tuning
  as a whole copy of the core's file in the person's data folder
  (`cores.rs`, `tuning_written`), and the core is read from that copy from
  then on: an update to the core's own file isn't seen while a copy is
  there, and nothing says the copy shadows it.
- No file Ectomatter saves has a version of its own (`files.rs`): a change to
  what one holds is read by its keys alone.
- The check finds a file carried from others by what shows it (a typeface,
  a notice or license in it or in its folder: `tools/notices.py`, `SHOWS`,
  `FONT`, `NOTICE`); a file copied in with none of that isn't found.
  `lcdfont.json` (Adafruit's, BSD-2-Clause) was found by hand, from where
  it was made.
- A library's notice is the license files its crate ships, or the license's
  own text with its authors from its manifest when it ships none
  (`tools/notices.py`, `library`); a license file whose name doesn't say
  its license is taken whole. Nothing reads a crate's copyright from its
  source files.
- The source beside each download: Ectomatter is under the GPLv3 (Grant,
  2026-10-27), which asks that
  directions to the source sit beside the program wherever it's offered
  (GPLv3 section 6). Nothing is offered yet; a release needs them. Settings'
  About (`src/ecto_settings.rs`, `about_section`) has no row for the source
  either; it gets one when the repository has it.
- About's Read opens `LICENSE` and `NOTICES` as the desktop opens a file
  (`src/about.rs`, `crate::games::open`); neither name has an extension,
  which Linux's `xdg-open` reads past but Windows' `start` asks about. No
  install lays Ectomatter out on Windows or macOS yet.
- The GitHub repositories exist in the Ectonomic organization
  (Ectonomic/Ectomatter and one per package, `ectomatter-<package id>`,
  public and empty since 2026-10-26); nothing is in them, and no package's
  manifest names its repository yet.
