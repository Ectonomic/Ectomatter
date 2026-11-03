#!/bin/bash
# Each core and plugin is a repository of its own (Ectonomic's are
# Ectonomic/ectomatter-<name>), linked here as a Git submodule at
# cores/<name> or plugins/<name>: none of a package's files is in
# Ectomatter's repository, and every package folder here is one of those
# links. Prints what isn't, a line each, and exits 1 when anything is.
set -uo pipefail
cd "$(dirname "$0")/.."
# (a file of the repository's own, not a link to another repository)
said=$(git ls-files -s -- cores plugins | awk '$1 != "160000" {print "in Ectomatter'"'"'s repository: " $4}')
linked=$(git config -f .gitmodules --get-regexp '^submodule\..*\.path$' 2>/dev/null | awk '{print $2}')
for m in cores/*/core.json plugins/*/core.json; do
  [ -f "$m" ] || continue
  d=$(dirname "$m")
  printf '%s\n' "$linked" | grep -qx "$d" || said+=$'\n'"not linked as a repository of its own: $d"
done
said=$(printf '%s\n' "$said" | sed '/^$/d')
[ -z "$said" ] && exit 0
printf '%s\n' "$said"
exit 1
