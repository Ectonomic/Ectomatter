# Making a core or a plugin

A **core** makes one device work in Ectomatter: a keyboard, a mouse, anything
on USB. A **plugin** brings in something that isn't a device, such as another
program (OpenRGB), the games on the computer, or the weather. Both are
packages, built and run the same way: a WebAssembly module that runs inside
Ectomatter and reaches only what its manifest names. Each is a repository of
its own. Ectonomic's two cores and five plugins are complete examples, linked
into this repository at `cores/` and `plugins/` (clone it with
`git clone --recurse-submodules`); start from the one closest to yours.

## The folder

```
my-mouse/
  core.json      the manifest: what the package is and what it may reach
  core.wasm      the module, built from module/
  glyph.svg      its icon, on a 64-unit square in currentColor, like the others
  device.json    a core's device: its art, controls and lights (deviceFile)
  LICENSE        its license's text
  module/        the module's source (not shipped)
```

The folder is the top of the package's own repository. Beside the package, the
repository may keep what isn't shipped: `crates/` (its own libraries),
`tools/`, `parity/` (test cases), `README.md`, `REUSE.toml` and `LICENSES/`
(who made the files it carries from others, and their licenses' texts),
`Cargo.toml` and `Cargo.lock`. A release leaves those out, and anything the
repository hasn't committed (`tools/release-package.py --list` shows what it
carries).

The manifest's fields are in `docs/core-protocol.md`, section 3, and the
calls a package answers in sections 5 and 6 (11 for a plugin). Ectomatter
reads `id`, `kind`, `name`, `version`, `protocol`, `module`, `license` and
`description` from every package. A core lists every USB device it opens in
`matches`, by vendor and product id; Ectomatter opens those and no others for
it. A plugin lists what else it reaches in `uses`: a website, a folder, a
program it starts.

## Building

The module is a WebAssembly component built for `wasm32-wasip2` against the
interface in `wit/core.wit`:

```sh
rustup target add wasm32-wasip2
tools/build-module.sh my-mouse/module/Cargo.toml my-mouse/core.wasm
```

`tools/build-module.sh` runs Cargo with the paths of the files the module
is built from written short (this repository's folder as `.`, Cargo's as
`cargo`): a module keeps them for its error messages, and built as it comes
they'd be your own folders, your home folder among them, in every copy of
your package. `tools/release-package.py` refuses a module that still holds
them. `tools/build-cores.sh` and `tools/build-plugins.sh` build every package
linked here this way.

A module is built inside this repository: it depends on the crate
`ectomatter-protocol` (`crates/ectomatter-protocol`) and the interface
(`wit/core.wit`) by path, from `cores/<name>/module/` or
`plugins/<name>/module/` (`../../../crates/ectomatter-protocol`,
`../../../wit`). Clone your package's repository there, or link it
(`git submodule add <its address> cores/my-mouse`), build it, and run
Ectomatter from the source folder to try it; or add its file in Settings
(Devices, Add a device core, then From an address or a file).

## License

A package runs inside Ectomatter, which is under the GPLv3, so it has to come
under a license compatible with it to be listed: the GPLv3 itself, or one of
those in `registry/gpl-compatible.json` (MIT, Apache-2.0, BSD and others). The
manifest names it in `license`, as an SPDX id.

## Releasing

A release is a GitHub release of the package's own repository, tagged with
the version in its manifest (`1.2.0`, no `v`), carrying three files:

```sh
python3 tools/release-package.py my-mouse --repository you/my-mouse
```

writes them into `target/releases/my-mouse/1.2.0/`:

- `core.json`, the manifest
- `my-mouse.ectocore`, the package: a zip of the folder without `module/`,
  with the license and the notices of the libraries the module is built from
- `my-mouse.ectocore.sha256`, the package's checksum

The same folder always makes the same file, so the checksum doesn't change
between your release and the list. The command also prints the package's
entry for the list of packages; `registry/README.md` says how to send it in.
Ectomatter checks each package's releases once a day, and a version that can
reach more than the one in use waits for the person to review it.
