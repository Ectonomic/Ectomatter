# Making a core or a plugin

A **core** makes one device work in Ectomatter: a keyboard, a mouse, anything
on USB. A **plugin** brings in something that isn't a device, such as another
program (OpenRGB), the games on the computer, or the weather. Both are
packages, built and run the same way: a WebAssembly module that runs inside
Ectomatter and reaches only what its manifest names. The two in this
repository's `cores/` and the five in `plugins/` are complete examples; start
from the one closest to yours.

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
cargo build --release --target wasm32-wasip2 --manifest-path my-mouse/module/Cargo.toml
cp my-mouse/module/target/wasm32-wasip2/release/my_mouse_core.wasm my-mouse/core.wasm
```

`tools/build-cores.sh` and `tools/build-plugins.sh` do this for every package
in this repository. Put the folder in Ectomatter's own (`cores/` or
`plugins/`) and run Ectomatter from the source folder to try it, or add its
file in Settings (Add a core, then From an address or a file).

For now a package builds only inside this repository: its module depends on
the crate `ectomatter-protocol` by path (`docs/deferred.md`).

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
