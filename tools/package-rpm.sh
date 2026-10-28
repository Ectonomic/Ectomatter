#!/bin/bash
# Ectomatter as an RPM (packaging/ectomatter.spec): tools/install.sh lays it
# out under /usr in a folder of its own, and rpmbuild makes the package of
# that tree, at the app's own version (apps/ectomatter/Cargo.toml).
#
#   tools/package-rpm.sh [--no-build] [--bin DIR] [--out DIR]
# --no-build and --bin go to tools/install.sh; the package is written to
# --out (target/rpm). Needs rpmbuild (Fedora: rpm-build). Writes nothing
# outside --out and target/rpm-work.
set -euo pipefail
cd "$(dirname "$0")/.."
out=target/rpm pass=()
while [ $# -gt 0 ]; do
  case "$1" in
    --no-build) pass+=(--no-build) ;;
    --bin) pass+=(--bin "$2"); shift ;;
    --out) out="$2"; shift ;;
    *) echo "package-rpm.sh: what's $1?" >&2; exit 2 ;;
  esac
  shift
done
command -v rpmbuild >/dev/null || { echo "package-rpm.sh: rpmbuild isn't installed (Fedora: dnf install rpm-build)" >&2; exit 1; }
version=$(sed -n 's/^version = "\(.*\)"/\1/p' apps/ectomatter/Cargo.toml | head -1)
work="$PWD/target/rpm-work"
rm -rf "$work"
mkdir -p "$work/staged" "$out"
tools/install.sh "${pass[@]}" --prefix /usr --destdir "$work/staged" >&2
# (its files dated as the last commit, so the same commit makes the same package)
export SOURCE_DATE_EPOCH=$(git log -1 --format=%ct)
rpmbuild -bb packaging/ectomatter.spec \
  --define "_topdir $work/rpmbuild" \
  --define "source_date_epoch_from_changelog 0" \
  --define "staged $work/staged" \
  --define "version $version" >&2
find "$work/rpmbuild/RPMS" -name '*.rpm' -exec cp {} "$out/" \; -print | sed "s|.*/|$out/|"
