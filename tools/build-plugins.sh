#!/bin/bash
# Build each plugin's WebAssembly module in plugins/*/module and put it beside the
# plugin's manifest as the file core.json names (core.wasm). Ectomatter runs the
# module from there when it runs from its source.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
for m in plugins/*/module/Cargo.toml; do
  plugin=$(dirname "$(dirname "$m")")
  tools/build-module.sh "$m" "$plugin/core.wasm"
done
