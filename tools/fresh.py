#!/usr/bin/env python3
"""Holds Ectomatter, and every package that comes with it, to the newest
release of each library they name themselves: each dependency in a crate of
the workspace, or in a package's module (cores/*/module, plugins/*/module), is
locked at the newest version crates.io has (not yanked, not a pre-release),
and every library under them at the newest their own needs allow (nothing
`cargo update` would still move). A library held back by another that needs
an older one isn't Ectomatter's to move: it's counted, not failed.

A release under a week old isn't required yet (Grant, 2026-11-02): a week
lets others find what's wrong with it first, as cargo's own
registry.global-min-publish-age will (stable from Rust 1.100; RFC 3923).
Until then the newest counted is the newest at least AGE old, by its
publish time in crates.io's index; one newer is reported, with the day it's
due, and doesn't fail the check.

A dependency docs/deferred.md names as a known failure (its line
"`tools/fresh.py` known failures:", each crate in backticks) is one whose
move waits for work written down there: it's reported, and the check exits 3.
Anything else behind exits 1; crates.io not reached exits 2.

  tools/fresh.py           check
  tools/fresh.py --list    every dependency, its version and the newest
"""

import json
import os
import re
import subprocess
import sys
import urllib.request
from datetime import datetime, timedelta, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# how old a release must be before it's required
AGE = timedelta(days=7)
NOW = datetime.now(timezone.utc)
# (cargo where rustup puts it, as the other tools here find it)
os.environ["PATH"] = str(Path.home() / ".cargo" / "bin") + os.pathsep + os.environ.get("PATH", "")


def projects():
    """The workspace, and each package's module, as tools/install.sh installs them."""
    found = [ROOT / "Cargo.toml"]
    for kind in ("cores", "plugins"):
        for d in sorted((ROOT / kind).iterdir()):
            if (d / "core.json").is_file() and (d / "module" / "Cargo.toml").is_file():
                found.append(d / "module" / "Cargo.toml")
    return found


def version(v):
    """A version as numbers to compare, and whether it's a pre-release."""
    core = v.split("+")[0]
    pre = "-" in core
    nums = [int(n) for n in re.findall(r"\d+", core.split("-")[0])[:3]]
    return tuple(nums + [0] * (3 - len(nums))), pre


def releases(name, cache={}):
    """Each release crates.io has of `name` (not yanked, not a pre-release),
    and when it was published (None where the index doesn't say)."""
    if name in cache:
        return cache[name]
    n = name.lower()
    path = {1: f"1/{n}", 2: f"2/{n}", 3: f"3/{n[0]}/{n}"}.get(len(n), f"{n[:2]}/{n[2:4]}/{n}")
    req = urllib.request.Request(f"https://index.crates.io/{path}", headers={"User-Agent": "ectomatter-fresh-check"})
    with urllib.request.urlopen(req, timeout=30) as r:
        lines = [json.loads(line) for line in r.read().decode().splitlines() if line.strip()]
    when = lambda e: datetime.fromisoformat(e["pubtime"].replace("Z", "+00:00")) if e.get("pubtime") else None
    cache[name] = {e["vers"]: when(e) for e in lines if not e.get("yanked") and not version(e["vers"])[1]}
    return cache[name]


def due(published):
    """When a release published then is required: AGE after it."""
    return published + AGE if published else None


def newest(name, now=None):
    """The newest release of `name` old enough to be required, and the newest
    of all where that's another, too new yet."""
    now = now or NOW
    rel = releases(name)
    if not rel:
        return None, None
    top = max(rel, key=lambda v: version(v)[0])
    ready = [v for v, t in rel.items() if t is None or t + AGE <= now]
    old = max(ready, key=lambda v: version(v)[0]) if ready else None
    return old, (top if top != old else None)


def direct(manifest):
    """Each library the project's own crates name, and the version locked."""
    out = subprocess.run(["cargo", "metadata", "--manifest-path", str(manifest), "--format-version", "1", "--locked"], capture_output=True, text=True, check=True).stdout
    m = json.loads(out)
    own = set(m["workspace_members"])
    by_id = {p["id"]: p for p in m["packages"]}
    named = {}
    for node in m["resolve"]["nodes"]:
        if node["id"] not in own:
            continue
        for d in node["deps"]:
            p = by_id[d["pkg"]]
            if p["source"] and p["source"].startswith("registry+"):
                named.setdefault(p["name"], set()).add(p["version"])
    return named


def pending(manifest):
    """What `cargo update` would still move, and what's held back by another library."""
    out = subprocess.run(["cargo", "update", "--dry-run", "--verbose", "--manifest-path", str(manifest)], capture_output=True, text=True, cwd=ROOT).stderr
    moves = [line.strip() for line in out.splitlines() if line.strip().startswith(("Updating ", "Adding ", "Removing ", "Downgrading ")) and "crates.io index" not in line]
    held = [line.strip() for line in out.splitlines() if "(available:" in line]
    return moves, held


def known():
    text = re.sub(r"\s+", " ", (ROOT / "docs" / "deferred.md").read_text())
    m = re.search(r"`tools/fresh.py` known failures: ([^)]*)", text)
    return set(re.findall(r"`([^`]+)`", m.group(1))) if m else set()


def main():
    listing = "--list" in sys.argv
    waits = known()
    bad, late, held_total = 0, 0, 0
    for manifest in projects():
        name = manifest.parent.relative_to(ROOT).as_posix() or "ectomatter"
        if name == ".":
            name = "ectomatter"
        try:
            named = direct(manifest)
            behind, waiting = [], []
            for dep, locked in sorted(named.items()):
                top, young = newest(dep)
                for v in sorted(locked, key=lambda v: version(v)[0]):
                    if listing:
                        print(f"  {name}: {dep} {v} (newest {top}" + (f"; {young} due {due(releases(dep)[young]):%Y-%m-%d})" if young else ")"))
                    if top and version(v)[0] < version(top)[0]:
                        behind.append((dep, v, top))
                    if young and version(v)[0] < version(young)[0]:
                        waiting.append(f"{dep} {young}, due {due(releases(dep)[young]):%Y-%m-%d}")
        except OSError as e:
            print(f"fresh: crates.io wasn't reached ({e})")
            return 2
        moves, held = pending(manifest)
        held_total += len(held)
        # (a move to a release too new yet waits for its day, as the newest named does)
        try:
            ripe = []
            for line in moves:
                m = re.match(r"(?:Updating|Adding|Downgrading) (\S+) (?:v\S+ -> )?v(\S+)", line)
                t = releases(m.group(1)).get(m.group(2)) if m else None
                if t and t + AGE > NOW:
                    waiting.append(f"{m.group(1)} {m.group(2)}, due {due(t):%Y-%m-%d}")
                else:
                    ripe.append(line)
            moves = ripe
        except OSError as e:
            print(f"fresh: crates.io wasn't reached ({e})")
            return 2
        for w in sorted(set(waiting)):
            print(f"fresh: {name}: under a week old, not required yet: {w}")
        for dep, v, top in behind:
            if dep in waits:
                print(f"fresh: {name} is on {dep} {v}, and {top} is out (known failure, docs/deferred.md)")
                late += 1
            else:
                print(f"fresh: {name} is on {dep} {v}, and {top} is out")
                bad += 1
        for line in moves:
            print(f"fresh: {name}: cargo update would still move: {line}")
            bad += 1
        if not behind and not moves:
            print(f"fresh: {name} holds ({len(named)} libraries named, {len(held)} under them held back by another)")
    return 1 if bad else 3 if late else 0


if __name__ == "__main__":
    sys.exit(main())
