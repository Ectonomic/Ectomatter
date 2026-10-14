# Ectotype

The software the EVGA Z10 RGB keyboard should have shipped with: every key's
assignment, the lighting and its live preview, macros you record on the
keyboard itself, and the display, on Linux, Windows and macOS.

Ectotype is an independent project. It is not made, endorsed or supported by
EVGA.

## Status

Work in progress. The keyboard core is written in Rust and matches the
original editor it replaces byte for byte. The desktop app comes next.

| Part | State |
|---|---|
| Keyboard core (`crates/ectotype-core`) | Profiles, key assignments, lighting, live preview, macros, display. Recording on Linux. |
| Desktop app | Not started |
| Recording on Windows and macOS | Not started |

## The keyboard core

`ectotype-core` finds the Z10 through hidapi by its LED-indicator collection
(usage page 0x08, usage 0x4B), which carries the keyboard's reports 3 to 10.
The protocol was worked out from EVGA's Unleash RGB 1.0.11.0 and, for the
display, from AIDA64.

The firmware updater's commands are deliberately not part of this crate.

```sh
cargo test                          # the parity tests
cargo run --example state           # print what a connected Z10 holds (reads only)
```

## Parity

The core replaces an earlier Python editor. `parity/gen.py` runs that
editor's code on thousands of cases and records what it produced: decoded
profiles and every kind of settings edit, preview frames, macros both ways,
display frames, recordings, and every exchange with the keyboard for each
command and editor action. `cargo test` replays them all, and every byte has
to match.

The committed vectors are synthetic. Vectors made from a real keyboard's
backups go in `parity/private/`, which is not committed, and are checked too
when present.

## License

MIT.
