# Ectomatter: rules for working here

Grant's global rules (~/.claude/CLAUDE.md, "Build like an architect") apply.
This is where they live in this repository.

- **Boundary tests.** `tools/boundaries.sh` runs every boundary the app claims
  and says which hold. A boundary that doesn't hold yet is a test marked
  `#[ignore = "known failure: ..."]`; never delete or weaken one to pass, and
  turn it into an ordinary test only when the code holds. The checks that read
  the source are in `apps/ectomatter/src/boundaries.rs`; the ones that run a
  core or plugin sit beside the code they try (`core_host.rs`, `hosted.rs`),
  named for the boundary. The checks for package knowledge in the app take
  their words from the packages themselves: their manifests' ids, names,
  programs and variables (`boundaries.rs`), and the settings, buttons and
  folds the plugins describe when run (`hosted.rs`,
  `no_plugins_setting_or_button_is_written_into_the_app`). A new check
  counts only after it has failed on a planted violation.
- **Deferred list.** `docs/deferred.md`. Anything put off, stubbed, faked or
  hardcoded goes there with its file and line the moment it's put off. Every
  report starts from it.
- **Docs are claims.** `docs/core-protocol.md` and `docs/design-system.md`
  describe what's built. Check the code before repeating them; where a claim
  can be checked, a test in `boundaries.rs` or `tokens.rs` checks it.
- **Stranger check.** `apps/ectomatter/tests/stranger.rs` (run by
  `tools/boundaries.sh`): the release build copied away from the repo with
  fresh `XDG_*` folders and no devices, drawn offscreen. Packages are found as
  the app runs (`cores::home()`): `ECTOMATTER_PACKAGES`, else the source
  folder above the program, else `share/ectomatter` beside its folder, else
  `XDG_DATA_DIRS`.
- **Render tests.** `cargo test --release -p ectomatter --test renders`
  (run by `tools/boundaries.sh` too; needs `bwrap`): every page drawn
  offscreen in a sandbox that hides this computer, on a made-up desk
  (`apps/ectomatter/tests/renders/desk/`, a computer described in a folder, a
  made-up Steam, the time fixed), held pixel for pixel to
  `apps/ectomatter/tests/renders/approved/`; both desks' pages drawn twice the
  same; every place of the window drawn. A page that changes writes its
  picture, where it differs and its log to `target/renders/`; when the change
  is meant, look at them, then `ECTOMATTER_APPROVE=1` the same command.
  Nothing of Grant's goes into the desk or the pictures.
  `ECTOMATTER_RENDERS=name,name` runs only those; `ECTOMATTER_RENDERS_LOG=1`
  keeps every page's log.
- **Presenting through wgpu.** `crates/ectomatter-present` turns a composed
  frame into HDR10, scRGB or SDR for a wgpu surface; its tests draw offscreen
  on Mesa's lavapipe (Vulkan on the CPU, `VK_ICD_FILENAMES`), so they touch
  no GPU: `cargo test --release -p ectomatter-present`.
- **Other systems.** `tools/check-systems.sh` type-checks the app for
  Windows and macOS from Linux (the C code some crates build goes to a
  stand-in compiler, `tools/cross/`); it failed on a planted Linux-only use.
  Linux-only code is marked `#[cfg(target_os = "linux")]` with a stand-in
  beside it elsewhere. Nothing is linked or run there.
- **Install.** `tools/install.sh [--prefix DIR] [--destdir DIR]` lays
  Ectomatter out as a Linux system has a program; `tests/install.rs` stages
  one and starts it from there (run by `tools/boundaries.sh`).
- **Log.** `~/.local/state/com.ectonomic.ectomatter/ectomatter.log`
  (`$XDG_STATE_HOME`), the run before's as `ectomatter.log.1`: the app's
  stderr is the file itself, so the terminal shows only where the log is.
  Read the file to check a relaunch. Snapshots and `--read-state` don't
  write it.
- **Private words.** `tools/private-words.sh` (run by `tools/boundaries.sh`
  and by the commit hook, `tools/pre-commit`, which a fresh clone links
  with `ln -s ../../tools/pre-commit .git/hooks/pre-commit`) holds the files
  to a list kept outside the repository,
  `~/.config/ectomatter-dev/private-words`: Grant's own details, which never
  go in. `--history` checks every commit and message.
- **Licenses.** `tools/licenses.sh` (run by `tools/boundaries.sh` and by
  the commit hook when a dependency changes or a file is added; needs
  `cargo install --locked cargo-deny`) holds the app and every package that ships with it to the
  licenses `deny.toml` approves. Adding a dependency, or a license to that
  list, is Grant's call: ask him first, with the dependency's license. A
  crate whose decision waits is named in `docs/deferred.md` as a known
  failure. Ectomatter and every part of it are GPL-3.0-only (`LICENSE`;
  Grant, 2026-10-27), which `tools/licenses.sh` checks in each crate's
  and package's own license; Slint comes under the same license. A license
  approved for some crates alone is a `deny.toml` exception with its text
  in `LICENSES/`, which `tools/licenses.sh` checks and the install ships
  (`tests/install.rs`). A file carried from others (a typeface, a protocol,
  icons, data) gets its own table in `REUSE.toml`, its makers and its
  license, and the license's text in `LICENSES/`: `tools/licenses.sh` finds
  each file that shows another's work (`tools/notices.py --check`) and stops
  on one without a table. The install writes `NOTICES` (the app's, and each
  package's beside its manifest) with `tools/notices.py`: every library and
  carried file, and its license text; `tests/install.rs` holds it to Cargo's
  own record of what ships. Settings' About (`src/about.rs`) shows the
  GPLv3's legal notices and opens the license and the notices (run from
  the source folder, it writes them with `tools/notices.py` first);
  `tests/install.rs` holds an installed copy to read them where the
  install puts them.
- **Design system.** `cargo test -p ectomatter tokens::system`; see
  `docs/design-system.md`.

## Never

- Push this repository.
- Commit Grant's tuning from the lab (`cores/evga-z10/device.json`,
  `cores/steelseries-sensei-310/light.json`) without his word. He had his
  tuning as of 2026-10-26 shipped as the cores' defaults; later
  tuning waits for him again. Stage explicit paths only; never `git stash`,
  `git checkout -- file` or `git reset --hard`.
- Commit untracked art, `before-*.svg`, `tools/sensei-310-art/`, untracked
  mocks, `cores/*/module/target` or `docs/drafts`.
- Write to a device from a test or a scratch tool without Grant's go. Set
  `MOUSE_LIVE` (it opens the real mouse). Send firmware commands.
- Signal a process without checking its exe, cwd and command line.
