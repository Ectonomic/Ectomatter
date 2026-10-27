#!/bin/bash
# Holds the repository to a list of words that must never be in it (a
# person's town, computer name, home folder, serial numbers): the list lives
# outside the repository, so the check names nothing it guards. One word or
# phrase a line, matched without regard to case; lines starting with # are
# passed over. The list is $ECTOMATTER_PRIVATE_WORDS, else
# ~/.config/ectomatter-dev/private-words; with no list there's nothing to
# hold to, and it says so.
#   tools/private-words.sh            the tracked files
#   tools/private-words.sh --staged   what a commit would add (the commit hook)
#   tools/private-words.sh --history  every commit's files and messages
set -uo pipefail
cd "$(dirname "$0")/.."
list=${ECTOMATTER_PRIVATE_WORDS:-$HOME/.config/ectomatter-dev/private-words}
if [ ! -s "$list" ]; then echo "private words: no list at $list, nothing checked"; exit 0; fi
words=$(mktemp); trap 'rm -f "$words"' EXIT
grep -v -e '^#' -e '^[[:space:]]*$' "$list" > "$words"
case "${1:-}" in
  --staged) hits=$(git diff --cached -U0 --no-color | grep '^+' | grep -v '^+++' | grep -n -i -F -f "$words") ;;
  --history)
    hits=$(git log --all --format=%B | grep -i -F -f "$words" | sed 's/^/message: /')
    for c in $(git rev-list --all); do
      hits+=$(git grep -I -l -i -F -f "$words" "$c" 2>/dev/null)
    done ;;
  *) hits=$(git grep -I -n -i -F -f "$words") ;;
esac
if [ -n "$hits" ]; then
  echo "private words found (the list is $list):"
  printf '%s\n' "$hits" | cut -c1-200
  exit 1
fi
echo "private words: none"
