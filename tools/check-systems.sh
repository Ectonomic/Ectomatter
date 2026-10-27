#!/bin/bash
# Ectomatter checked for every system it's to run on, from this one: the app
# and its crates type-checked for Windows and macOS (cargo check, nothing
# linked). The C code a few crates build (hidapi, ring) is handed to a stand-in
# compiler that makes empty objects (tools/cross/), so this checks Ectomatter's
# own Rust, not theirs. Needs the targets once:
#   rustup target add x86_64-pc-windows-gnu aarch64-apple-darwin
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
here="$PWD/tools/cross"
status=0
for target in x86_64-pc-windows-gnu aarch64-apple-darwin; do
  var=$(echo "$target" | tr '-' '_')
  out=$(env "CC_$var=$here/cc" "AR_$var=$here/ar" cargo check -q --target "$target" -p ectomatter -p ectomatter-present 2>&1)
  if printf '%s\n' "$out" | grep -q '^error'; then
    printf '%s\n' "$out" | grep -E '^error|-->' | head -40
    echo "  doesn't build  $target"
    status=1
  else
    echo "  builds         $target"
  fi
done
exit $status
