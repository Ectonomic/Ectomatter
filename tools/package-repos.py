"""Each of Ectomatter's own packages laid out as its own repository, the one
its GitHub releases come from (Ectonomic/ectomatter-<folder>): the package's
folder as its release carries it (tools/release-package.py) with its module's
source, the libraries the module is built from (crates/), the interface
(wit/core.wit), the build tool (tools/build-module.sh), a root Cargo.toml so
the libraries find their workspace, the license, the REUSE tables of the
files it carries from others with their texts, and a README. Laid out as in
this repository, so the same command builds the module in either.

  package-repos.py OUT   each repository into OUT/ectomatter-<folder>, and
      each release's notes beside them (OUT/notes-<folder>.md)

The README and the notes are made from the package's manifest (its name,
description and kind) and its libraries' descriptions, so they say what the
package says of itself.
"""
import fnmatch
import importlib.util
import json
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# REUSE.toml's keys (split, so this file doesn't show them: tools/notices.py, SHOWS)
WHO, LICENSE = 'SPDX-File' + 'CopyrightText', 'SPDX-License' + '-Identifier'
spec = importlib.util.spec_from_file_location('release_package', ROOT / 'tools/release-package.py')
rp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rp)

# (what a library's tests read from outside its folder)
TESTS_READ = {'crates/evga-z10': ['parity/gen.py', 'parity/vectors.json.gz']}
# (the makers whose names a package carries, which didn't make it)
NOT_BY = {'evga-z10': 'EVGA', 'steelseries-sensei-310': 'SteelSeries', 'openrgb-link': 'the OpenRGB project'}
# (where a core's knowledge of its device came from)
FROM = {
    'evga-z10': "The keyboard's protocol was worked out from EVGA's Unleash RGB 1.0.11.0 and, for the display, "
                "from AIDA64. The firmware updater's commands are deliberately not part of it.",
    'steelseries-sensei-310': "Its commands were written from rivalcfg's profile of the mouse and libratbag's "
                              "SteelSeries driver, used as references; neither is copied.",
}


def tracked(*paths):
    out = subprocess.run(['git', '-C', str(ROOT), 'ls-files', '-z', '--', *paths], check=True, capture_output=True).stdout
    return [f for f in out.decode().split('\0') if f]


def libraries(cargo_toml, seen):
    """The crates a Cargo.toml depends on by path, and theirs, as folders of this repository."""
    for d in tomllib.loads(cargo_toml.read_text()).get('dependencies', {}).values():
        if isinstance(d, dict) and 'path' in d:
            p = (cargo_toml.parent / d['path']).resolve().relative_to(ROOT)
            if p not in seen:
                seen.add(p)
                libraries(ROOT / p / 'Cargo.toml', seen)
    return seen


def lower(d):
    d = d.rstrip('.')
    return d[:1].lower() + d[1:]


def q(s):
    return json.dumps(s, ensure_ascii=False)


def workspace(crates):
    """The root Cargo.toml: this repository's shared settings, for the crates taken."""
    text = (ROOT / 'Cargo.toml').read_text()
    shared = re.search(r'^\[workspace\.package\]\n.*?(?=^\[workspace\.dependencies\])', text, re.S | re.M).group(0)
    deps = re.search(r'^\[workspace\.dependencies\]\n(?:(?!^\[|^#).*\n)*', text, re.M).group(0)
    members = ', '.join(q(str(c)) for c in crates)
    return ("# The libraries this package's module is built from, as one workspace, so\n"
            "# each finds the settings it shares with the rest of Ectomatter. The module,\n"
            "# in its folder's module/, is a workspace of its own: it builds only for\n"
            "# WebAssembly (tools/build-module.sh).\n"
            f'[workspace]\nresolver = "3"\nmembers = [{members}]\n\n{shared}{deps}')


def reuse(files):
    """REUSE.toml's first table, and each later one that names a file here (its
    paths cut to those), with the licenses they name."""
    tables = tomllib.loads((ROOT / 'REUSE.toml').read_text())['annotations']
    out = ['# Who made each file here, and the license it comes under, in the REUSE',
           "# format (https://reuse.software/spec-3.3/). Every file is Ectonomic's own",
           '# unless a later table names it.', 'version = 1', '']
    ids = set()
    for n, a in enumerate(tables):
        ps = a['path'] if isinstance(a['path'], list) else [a['path']]
        ps = ps if n == 0 else [p for p in ps if any(fnmatch.fnmatch(f, p) for f in files)]
        if not ps:
            continue
        ids.update(t for t in re.findall(r'[A-Za-z0-9.+-]+', a[LICENSE]) if t not in ('AND', 'OR', 'WITH'))
        who = a[WHO]
        who = '[' + ', '.join(q(c) for c in who) + ']' if isinstance(who, list) else q(who)
        out += ['[[annotations]]', 'path = ' + (q(ps[0]) if n == 0 else '[' + ', '.join(q(p) for p in ps) + ']'),
                f'{WHO} = {who}', f'{LICENSE} = {q(a[LICENSE])}', '']
    own = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['license']
    return '\n'.join(out), sorted(ids - {own})


def readme(m, name, pkg, crates, carried, tests):
    core = m['kind'] == 'core'
    word = 'core' if core else 'plugin'
    what = ('A core is the part of Ectomatter that talks to one device. It runs inside Ectomatter as a '
            f'WebAssembly module and reaches only the USB devices its manifest (`{pkg}/core.json`) names.') if core else (
            "A plugin brings something into Ectomatter that isn't a device, such as another program, the games "
            'on the computer or the weather. It runs inside Ectomatter as a WebAssembly module and reaches only '
            f'what its manifest (`{pkg}/core.json`) names.')
    place, button = ('Devices', 'Add a device core') if core else ('Plugins', 'Add a plugin')
    out = [f'# {m["name"]} for Ectomatter', '', m['description'], '',
           f'This is a {word} for Ectomatter, one app for the devices on your desk. {what}', '']
    if name in NOT_BY:
        out += [f'It is independent: not made, endorsed or supported by {NOT_BY[name]}.', '']
    out += ['## Adding it', '',
            f"In Ectomatter's Settings, under {place}, choose **{button}**, then pick {m['name']} and press **Add**. "
            f'Ectomatter shows what the {word} can reach before you add it, and takes the package only if it matches '
            "the checksum in Ectonomic's list of reviewed packages, "
            '[ectomatter-packages](https://github.com/Ectonomic/ectomatter-packages).', '',
            '## Building it', '',
            "The folders are laid out as in Ectomatter's own repository. To build the module:", '',
            '```sh', 'rustup target add wasm32-wasip2', f'tools/build-module.sh {pkg}/module/Cargo.toml {pkg}/core.wasm', '```', '',
            "Each release's module is built from its tag this way, with Rust "
            + subprocess.run(['rustc', '--version'], check=True, capture_output=True, text=True).stdout.split()[1] + '.', '',
            f"- `{pkg}/`: the {word} itself: its manifest, the files it carries, and its module's source (`module/`)",
            '- `crates/`: the libraries the module is built from']
    out += [f'  - `{c.name}`: {lower(tomllib.loads((ROOT / c / "Cargo.toml").read_text())["package"]["description"])}' for c in crates]
    out += ['- `wit/core.wit`: the interface between the module and Ectomatter']
    for crate, paths in tests.items():
        out += [f"- `{Path(paths[0]).parts[0]}/`: the cases the `{Path(crate).name}` library's tests replay (`cargo test -p {Path(crate).name}`)"]
    out += ['']
    if name in FROM:
        out += [FROM[name], '']
    out += ['## License', '', 'Copyright (C) 2026 Ectonomic LLC. Made by Grant Garrison and Claude.', '',
            f'This {word} is free software under the GNU General Public License, version 3 only (`LICENSE`): '
            'you may share and change it under its terms. It comes with no warranty.'
            + (' The files it carries from others are under their own licenses, named in `REUSE.toml`, '
               'with their texts in `LICENSES/`.' if carried else ''), '']
    return '\n'.join(out)


def notes(m):
    core = m['kind'] == 'core'
    word = 'core' if core else 'plugin'
    place, button = ('Devices', 'Add a device core') if core else ('Plugins', 'Add a plugin')
    return '\n'.join([m['description'], '', f"Add it in Ectomatter's Settings, under {place}: **{button}**.", '',
                      f'- `{m["id"]}.ectocore`: the {word}',
                      f"- `{m['id']}.ectocore.sha256`: its SHA-256 checksum, the one Ectonomic's list of packages pins",
                      '- `core.json`: its manifest: what it is and what it can reach', ''])


def main(argv):
    if not argv or argv[0].startswith('-'):
        sys.exit(__doc__)
    out = Path(argv[0]).resolve()
    for manifest in sorted((ROOT / 'cores').glob('*/core.json')) + sorted((ROOT / 'plugins').glob('*/core.json')):
        folder = manifest.parent
        name, pkg = folder.name, str(folder.relative_to(ROOT))
        m = rp.jsonc(manifest.read_text())
        crates = sorted(libraries(folder / 'module/Cargo.toml', set()))
        tests = {str(c): TESTS_READ[str(c)] for c in crates if str(c) in TESTS_READ}
        at = out / f'ectomatter-{name}'
        shutil.rmtree(at, ignore_errors=True)
        at.mkdir(parents=True)
        # (the package as its release carries it, built module aside, which may hold
        # files not tracked here, such as a drawing; its module's source; the rest
        # as committed)
        shipped = [str(p.relative_to(ROOT)) for p in rp.package(folder) if p.name != m['module']]
        held = rp.unshipped({f: (ROOT / f).read_bytes() for f in shipped})
        if held:
            sys.exit(f'package-repos: {", ".join(held)} still holds a picture or a layer marked to delete before shipping')
        files = shipped + tracked(f'{pkg}/module', 'wit', *map(str, crates), *(p for ps in tests.values() for p in ps))
        files += ['tools/build-module.sh', 'LICENSE']
        for f in sorted(set(files)):
            (at / f).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / f, at / f)
        (at / 'Cargo.toml').write_text(workspace(crates))
        (at / '.gitignore').write_text('target/\ncore.wasm\n')
        table, carried = reuse(sorted(set(files)))
        (at / 'REUSE.toml').write_text(table)
        for i in carried:
            (at / 'LICENSES').mkdir(exist_ok=True)
            shutil.copy2(ROOT / 'LICENSES' / f'{i}.txt', at / 'LICENSES' / f'{i}.txt')
        (at / 'README.md').write_text(readme(m, name, pkg, crates, carried, tests))
        (out / f'notes-{name}.md').write_text(notes(m))
        print(f'{at.name}: {len(set(files))} files, built from {", ".join(c.name for c in crates)}'
              + (f'; carries {", ".join(carried)}' if carried else ''))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
