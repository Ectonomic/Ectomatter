#!/bin/bash
# Build the packages only the tests use (apps/ectomatter/tests/packages/*), each
# module beside its manifest as core.wasm, as tools/build-plugins.sh does for
# the plugins that come with Ectomatter.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
for m in apps/ectomatter/tests/packages/*/module/Cargo.toml; do
  package=$(dirname "$(dirname "$m")")
  tools/build-module.sh "$m" "$package/core.wasm"
done
