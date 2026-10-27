#!/bin/bash
# Ectomatter installed as a Linux system lays a program out, under PREFIX
# (default /usr/local), staged under DESTDIR for a package to be made of it:
#   PREFIX/bin/ectomatter, and ectomatter-launch beside it (the program Steam
#     starts a game through when it starts it through Ectomatter)
#   PREFIX/share/ectomatter/cores/<id>, PREFIX/share/ectomatter/plugins/<id>:
#     each package that comes with Ectomatter, as installed (its manifest,
#     module and files; not its source)
#   PREFIX/share/applications/com.ectonomic.ectomatter.desktop, and its icon
#     in PREFIX/share/icons/hicolor/scalable/apps
#   PREFIX/lib/udev/rules.d/70-ectomatter.rules: the person at the computer
#     may open each device a core reaches (made by `ectomatter --udev-rules`
#     from the cores' manifests)
#   PREFIX/share/licenses/ectomatter: Ectomatter's license (the GPLv3), the
#     license texts in LICENSES/, and NOTICES: each library the app is built
#     from and each file it carries from others, with its license text
#     (tools/notices.py); each package's own NOTICES beside its manifest
#
#   tools/install.sh [--prefix DIR] [--destdir DIR] [--no-build] [--bin DIR]
# Builds the release program, the launcher, the cores and the plugins first,
# unless --no-build; --bin names the folder the built programs are in
# (target/release). Writes nothing outside DESTDIR/PREFIX.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
prefix=/usr/local destdir="" build=1 bin=target/release
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) prefix="$2"; shift ;;
    --destdir) destdir="$2"; shift ;;
    --no-build) build=0 ;;
    --bin) bin="$2"; shift ;;
    *) echo "install.sh: what's $1?" >&2; exit 2 ;;
  esac
  shift
done
root="$destdir$prefix"
if [ $build = 1 ]; then
  cargo build --release -p ectomatter -p ectomatter-launch
  tools/build-cores.sh
  tools/build-plugins.sh
fi
install -Dm755 "$bin/ectomatter" "$root/bin/ectomatter"
if [ -x "$bin/ectomatter-launch" ]; then
  install -Dm755 "$bin/ectomatter-launch" "$root/bin/ectomatter-launch"
else
  echo "install.sh: no $bin/ectomatter-launch: games can't be started through Ectomatter from Steam" >&2
fi
# each package as installed: its files, not its module's source or what building it left
for kind in cores plugins; do
  for dir in "$kind"/*/; do
    id=$(basename "$dir")
    [ -f "$dir/core.json" ] || continue
    [ -f "$dir/core.wasm" ] || { echo "install.sh: $kind/$id isn't built: run tools/build-$kind.sh" >&2; exit 1; }
    out="$root/share/ectomatter/$kind/$id"
    mkdir -p "$out"
    (cd "$dir" && find . -path ./module -prune -o -type f ! -name '*.before-*' -print) | while read -r f; do
      install -Dm644 "$dir/$f" "$out/$f"
    done
    [ ! -f "$dir/module/Cargo.toml" ] || python3 -I tools/notices.py "$dir/module/Cargo.toml" --files "$dir" > "$out/NOTICES"
  done
done
install -Dm644 packaging/com.ectonomic.ectomatter.desktop "$root/share/applications/com.ectonomic.ectomatter.desktop"
install -Dm644 packaging/com.ectonomic.ectomatter.svg "$root/share/icons/hicolor/scalable/apps/com.ectonomic.ectomatter.svg"
mkdir -p "$root/lib/udev/rules.d"
ECTOMATTER_PACKAGES="$root/share/ectomatter" "$root/bin/ectomatter" --udev-rules 2>/dev/null > "$root/lib/udev/rules.d/70-ectomatter.rules"
licenses="$root/share/licenses/ectomatter"
install -Dm644 LICENSE "$licenses/LICENSE"
for f in LICENSES/*.txt; do
  [ -f "$f" ] || continue
  install -Dm644 "$f" "$licenses/$(basename "$f")"
done
python3 -I tools/notices.py Cargo.toml > "$licenses/NOTICES"
echo "installed under $root"
