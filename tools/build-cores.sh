#!/bin/bash
# Build each core's WebAssembly module in cores/*/module and put it beside the
# core's manifest as the file core.json names (core.wasm). Ectomatter runs the
# module from there when it runs from its source.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
for m in cores/*/module/Cargo.toml; do
  core=$(dirname "$(dirname "$m")")
  tools/build-module.sh "$m" "$core/core.wasm"
done
