#!/bin/bash
# Build the packages only the tests use (apps/ectomatter/tests/packages/*), each
# module beside its manifest as core.wasm, as tools/build-plugins.sh does for
# the plugins that come with Ectomatter.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
for m in apps/ectomatter/tests/packages/*/module/Cargo.toml; do
  package=$(dirname "$(dirname "$m")")
  cargo build --quiet --release --target wasm32-wasip2 --manifest-path "$m" --target-dir target/cores
  name=$(sed -n 's/^name = "\(.*\)"/\1/p' "$m" | head -1 | tr - _)
  cp "target/cores/wasm32-wasip2/release/$name.wasm" "$package/core.wasm"
  echo "$package/core.wasm $(stat -c %s "$package/core.wasm") bytes"
done
