#!/bin/bash
# Every boundary Ectomatter claims, tried, the known failures too: which hold
# and which don't yet. A boundary that doesn't hold is marked in its test as a
# known failure and listed in docs/deferred.md. Builds and runs the tests in
# release, as the app's other tests run.
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
out=$(cargo test --release -p ectomatter -- --include-ignored boundar 2>&1)
lines=$(printf '%s\n' "$out" | grep -E '^test .*boundar.* \.\.\. ' | sort)
if [ -z "$lines" ]; then printf '%s\n' "$out" | tail -30; exit 2; fi
held=$(printf '%s\n' "$lines" | grep -c ' ok$')
not=$(printf '%s\n' "$lines" | grep -c ' FAILED$')
printf '%s\n' "$lines" | sed -E 's/^test (.*) \.\.\. ok$/  holds      \1/; s/^test (.*) \.\.\. FAILED$/  not yet    \1/'
echo "$held hold, $not don't yet"
