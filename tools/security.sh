#!/bin/bash
# Holds Ectomatter and every package that comes with it (each folder under
# cores/ and plugins/ with a manifest and a module, as tools/install.sh
# installs them) to RustSec's advisory database
# (https://github.com/RustSec/advisory-db), with cargo-deny and deny.toml's
# [advisories] and [sources]: no library that ships with a known security
# hole, one that's unsound or no longer looked after, or a version taken back
# from crates.io; and every library from crates.io alone. Fetches the
# database first, so it needs the network. Prints each project's result.
# An advisory docs/deferred.md names as a known failure (its line
# "`tools/security.sh` known failures:", each advisory's id in backticks) is
# one whose fix waits for work written down there: it's reported, and the
# check exits 3; any other exits 1, as does a library from anywhere else.
#   tools/security.sh
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v cargo-deny > /dev/null; then
  echo "security: cargo-deny isn't installed (cargo install --locked cargo-deny)"
  exit 2
fi
projects=(Cargo.toml)
for kind in cores plugins; do
  for d in "$kind"/*/; do
    [ -f "$d/core.json" ] && [ -f "$d/module/Cargo.toml" ] && projects+=("$d"module/Cargo.toml)
  done
done
# (the list's lines joined, as the entry may wrap)
known=$(tr '\n' ' ' < docs/deferred.md | sed 's/  */ /g' | grep -oE 'tools/security.sh` known failures: [^)]*' | sed 's/^[^:]*: //' | grep -oE '`[^`]+`' | tr -d '`')
is_known() { local k; for k in $known; do [[ $1 == $k ]] && return 0; done; return 1; }
bad=0 late=0
for m in "${projects[@]}"; do
  name=$(dirname "$m"); [ "$name" = . ] && name=ectomatter
  out=$(cargo deny --format json --manifest-path "$m" check advisories sources 2>&1)
  # (each problem cargo-deny found: an advisory's id and its library, or a source's)
  found=$(printf '%s\n' "$out" | python3 -I -c '
import json, sys
for line in sys.stdin:
    try:
        d = json.loads(line)
    except ValueError:
        continue
    f = d.get("fields", {})
    if f.get("severity") != "error":
        continue
    a = f.get("advisory")
    print((a["id"] + " " + a["package"] + " " + a["title"]) if a else ("- - " + f.get("message", "")))
')
  if [ -z "$found" ] && ! printf '%s\n' "$out" | grep -q '"type":"summary"'; then
    printf '%s\n' "$out" | tail -5
    echo "security: $name couldn't be checked"
    exit 2
  fi
  if [ -z "$found" ]; then
    echo "security: $name holds"
    continue
  fi
  while read -r id crate title; do
    if [ "$id" != - ] && is_known "$id"; then
      echo "security: $name: $crate, $id: $title (known failure, docs/deferred.md)"
      late=$((late + 1))
    else
      echo "security: $name: $crate, $id: $title"
      bad=$((bad + 1))
    fi
  done <<< "$found"
done
[ "$bad" -gt 0 ] && exit 1
[ "$late" -gt 0 ] && exit 3
exit 0
