#!/bin/bash
# Holds Ectomatter and every package that comes with it (each folder under
# cores/ and plugins/ with a manifest and a module, as tools/install.sh
# installs them) to the licenses deny.toml approves (cargo-deny finds it
# above each project): every dependency that
# ships, on every system, checked by cargo-deny. Prints each project's
# result. A crate docs/deferred.md names as a known failure (its line
# "`tools/licenses.sh` known failures:", each crate in backticks, `name-*` for
# a family) is one whose decision waits: it's reported, and the check exits
# 3; any other crate not approved exits 1, as does a license deny.toml
# approves for some crates alone (its exceptions) whose text isn't in
# LICENSES/<its id>.txt, which the install ships with the app, and a part
# of Ectomatter (a crate of the workspace, a package's module or manifest)
# under another license than Ectomatter's own (Cargo.toml's, LICENSE).
# The files carried from others are held too (tools/notices.py --check):
# each file that shows another's work (a typeface, a notice or license in it
# or beside it) has a table of its own in REUSE.toml, whose licenses have
# their text in LICENSES/ and are ones deny.toml approves; a license
# docs/deferred.md names in the same list waits, as a crate does.
#   tools/licenses.sh          check
#   tools/licenses.sh --list   each license and the crates under it
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v cargo-deny > /dev/null; then
  echo "licenses: cargo-deny isn't installed (cargo install --locked cargo-deny)"
  exit 2
fi
projects=(Cargo.toml)
for kind in cores plugins; do
  for d in "$kind"/*/; do
    [ -f "$d/core.json" ] && [ -f "$d/module/Cargo.toml" ] && projects+=("$d"module/Cargo.toml)
  done
done
# (the list's lines joined, as the entry may wrap)
known=$(tr '\n' ' ' < docs/deferred.md | sed 's/  */ /g' | grep -oE 'tools/licenses.sh` known failures: [^)]*' | sed 's/^[^:]*: //' | grep -oE '`[^`]+`' | tr -d '`')
is_known() { local k; for k in $known; do [[ $1 == $k ]] && return 0; done; return 1; }
tmp=$(mktemp); trap 'rm -f "$tmp"' EXIT
bad=0
for id in $(awk '/^\[\[licenses\.exceptions\]\]/{x=1; next} /^\[/{x=0} x && /^allow *=/' deny.toml | grep -oE '"[^"]+"' | tr -d '"' | sort -u); do
  if [ ! -f "LICENSES/$id.txt" ]; then
    echo "licenses: deny.toml approves $id for some crates, and its text isn't in LICENSES/$id.txt"
    bad=1
  fi
done
own=$(sed -n 's/^license = "\(.*\)"$/\1/p' Cargo.toml | head -1)
for m in "${projects[@]}"; do
  cargo metadata --manifest-path "$m" --locked --no-deps --format-version 1 2>/dev/null \
    | python3 -I -c 'import json, sys; [print(p["name"], p["license"]) for p in json.load(sys.stdin)["packages"]]'
done > "$tmp"
for d in cores/*/ plugins/*/; do
  [ -f "$d/core.json" ] && [ -f "$d/module/Cargo.toml" ] && echo "${d%/}/core.json $(sed -n 's/^ *"license": *"\([^"]*\)".*/\1/p' "$d/core.json")" >> "$tmp"
done
while read -r part lic; do
  if [ "$lic" != "$own" ]; then
    echo "licenses: $part is under ${lic:-no license}, not Ectomatter's $own"
    bad=1
  fi
done < "$tmp"
# (the files carried from others)
python3 -I tools/notices.py --check > "$tmp"
while read -r what id rest; do
  [ -n "$what" ] || continue
  if [ "$what" = unapproved ] && is_known "$id"; then
    echo "licenses: carried under $id, not approved yet (known: docs/deferred.md): $rest"; [ $bad -eq 1 ] || bad=3
  elif [ "$what" = unapproved ]; then
    echo "licenses: carried under $id, which deny.toml doesn't approve: $rest"; bad=1
  else
    echo "licenses: $what $id${rest:+ $rest} (REUSE.toml)"; bad=1
  fi
done < "$tmp"
for m in "${projects[@]}"; do
  name=$(dirname "$m"); [ "$name" = . ] && name=ectomatter || name=${name%/module}
  if [ "${1:-}" = --list ]; then
    echo "== $name"
    cargo deny --manifest-path "$m" --locked -L error list 2>&1
    continue
  fi
  said=$(cargo deny --manifest-path "$m" --locked -L error -c never check licenses 2>&1)
  if [ $? -eq 0 ]; then
    echo "licenses: $name holds"
  else
    echo "licenses: $name has dependencies whose license isn't approved:"
    # (each rejected crate, and the licenses it offers: its line in the
    # report under the license it gives)
    printf '%s\n' "$said" | awk '/^[0-9 ]+│ license = /{sub(/^[^"]*"/,""); sub(/".*/,""); lic=$0} /^ +├ [A-Za-z0-9_-]+ v[0-9][^ ]*$/{print $2 " " $3 ": " lic}' | sort -u > "$tmp"
    while read -r crate rest; do
      if is_known "$crate"; then
        echo "  $crate $rest (known: docs/deferred.md)"; [ $bad -eq 1 ] || bad=3
      else
        echo "  $crate $rest"; bad=1
      fi
    done < "$tmp"
    [ -s "$tmp" ] || bad=1
  fi
done
exit $bad
