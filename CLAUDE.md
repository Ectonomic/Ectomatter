# Ectomatter: rules for working here

Grant's global rules (~/.claude/CLAUDE.md, "Build like an architect") apply.
This is where they live in this repository.

- **Boundary tests.** `tools/boundaries.sh` runs every boundary the app claims
  and says which hold. A boundary that doesn't hold yet is a test marked
  `#[ignore = "known failure: ..."]`; never delete or weaken one to pass, and
  turn it into an ordinary test only when the code holds. The checks that read
  the source are in `apps/ectomatter/src/boundaries.rs`; the ones that run a
  core or plugin sit beside the code they try (`core_host.rs`, `hosted.rs`),
  named for the boundary.
- **Deferred list.** `docs/deferred.md`. Anything put off, stubbed, faked or
  hardcoded goes there with its file and line the moment it's put off. Every
  report starts from it.
- **Docs are claims.** `docs/core-protocol.md` and `docs/design-system.md`
  describe what's built. Check the code before repeating them; where a claim
  can be checked, a test in `boundaries.rs` or `tokens.rs` checks it.
- **Stranger check.** Not available yet: the app still finds its packages
  through this source folder (deferred list, stage 2). Once it doesn't, the
  check is to run the release build from a copy outside the repo with a fresh
  `XDG_CONFIG_HOME` and `XDG_DATA_HOME` and no devices.
- **Design system.** `cargo test -p ectomatter tokens::system`; see
  `docs/design-system.md`.

## Never

- Push this repository.
- Commit Grant's tuning: `cores/evga-z10/device.json` and
  `cores/steelseries-sensei-310/light.json` stay uncommitted. Stage explicit
  paths only; never `git stash`, `git checkout -- file` or `git reset --hard`.
- Commit untracked art, `before-*.svg`, `tools/sensei-310-art/`, untracked
  mocks, `cores/*/module/target` or `docs/drafts`.
- Write to a device from a test or a scratch tool without Grant's go. Set
  `MOUSE_LIVE` (it opens the real mouse). Send firmware commands.
- Signal a process without checking its exe, cwd and command line.
