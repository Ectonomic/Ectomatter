#!/bin/bash
# Build each plugin's WebAssembly module in plugins/*/module and put it beside the
# plugin's manifest as the file core.json names (core.wasm). Ectodeck runs the
# module from there when it runs from its source.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
for m in plugins/*/module/Cargo.toml; do
  plugin=$(dirname "$(dirname "$m")")
  cargo build --quiet --release --target wasm32-wasip2 --manifest-path "$m" --target-dir target/cores
  name=$(sed -n 's/^name = "\(.*\)"/\1/p' "$m" | head -1 | tr - _)
  cp "target/cores/wasm32-wasip2/release/$name.wasm" "$plugin/core.wasm"
  echo "$plugin/core.wasm $(stat -c %s "$plugin/core.wasm") bytes"
done
