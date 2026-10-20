#!/bin/bash
# Build each core's WebAssembly module in cores/*/module and put it beside the
# core's manifest as the file core.json names (core.wasm). Ectodeck runs the
# module from there when it runs from its source.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
for m in cores/*/module/Cargo.toml; do
  core=$(dirname "$(dirname "$m")")
  cargo build --quiet --release --target wasm32-wasip2 --manifest-path "$m" --target-dir target/cores
  name=$(sed -n 's/^name = "\(.*\)"/\1/p' "$m" | head -1 | tr - _)
  cp "target/cores/wasm32-wasip2/release/$name.wasm" "$core/core.wasm"
  echo "$core/core.wasm $(stat -c %s "$core/core.wasm") bytes"
done
