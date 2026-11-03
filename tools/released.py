"""Each of Ectonomic's packages linked here (cores/<name>, plugins/<name>:
each its own repository), as it stands here, held to its release, the
version the list of packages names (registry/packages.json):

- the files it would ship now (tools/release-package.py --list), each byte
  for byte as the release's package on GitHub carries it (its built module,
  license and notices aside);
- its module's source and its own libraries, each file as its release's tag
  has it; and, where the tag holds its own copy of what Ectomatter's tree
  gives every package (the protocol's library, the sensors' library, the
  interface), that too. A tag made before the package moved to the top of
  its repository has it under cores/<name>/ or plugins/<name>/, and a
  Cargo.toml is compared without its dependencies' folders (`path = ...`):
  where a library sits isn't what it is, which its own files say.

A package changed since its release, without a new version, shows here. The
built module isn't compared, only its source: built in another folder, the
same source can give a few bytes of difference (docs/deferred.md).

  released.py   prints what differs, a line each, and exits 1 when anything does

Needs the network (github.com) and each package's tags (git fetch --tags).
"""
import importlib.util
import io
import json
import os
import re
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# (git's own variables left out: a commit hook sets them for this repository,
# and they'd point a package's git there too)
GIT_ENV = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
spec = importlib.util.spec_from_file_location('release_package', ROOT / 'tools/release-package.py')
rp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rp)
# (what Ectomatter's tree gives every package, by the module dependency that names it)
SHARED = {'ectomatter-protocol': 'crates/ectomatter-protocol', 'ectomatter-sensors': 'crates/ectomatter-sensors'}


def git(at, *args):
    return subprocess.run(['git', '-C', str(at), *args], check=True, capture_output=True, env=GIT_ENV).stdout


def same(path, a, b):
    if path.endswith('Cargo.toml'):
        a, b = (re.sub(rb'path = "[^"]*"', b'path = ""', x) for x in (a, b))
    return a == b


def fetched(url):
    with urllib.request.urlopen(urllib.request.Request(url, headers={'User-Agent': 'ectomatter-released'}), timeout=60) as r:
        return r.read()


def main():
    listed = {e['versions'][-1]['manifest']['id']: e for e in json.loads((ROOT / 'registry/packages.json').read_text())['packages']}
    said = []
    for d in sorted(p.parent for kind in ('cores', 'plugins') for p in (ROOT / kind).glob('*/core.json')):
        m = rp.jsonc((d / 'core.json').read_text())
        pid, version, at = m['id'], m['version'], f'{d.parent.name}/{d.name}'
        e = listed.get(pid)
        if not e:
            said.append(f'{at}: {pid} is not on the list')
            continue
        tag = e['versions'][-1]['manifest']['version']
        if tag != version:
            said.append(f'{at}: is {version} here, the list names {tag}')
            continue
        # (the files it ships, against its release's package)
        try:
            z = zipfile.ZipFile(io.BytesIO(fetched(f'https://github.com/{e["repository"]}/releases/download/{tag}/{pid}.ectocore')))
        except Exception as err:
            said.append(f'{at}: its release {tag} can\'t be read from {e["repository"]} ({err})')
            continue
        there = {n.split('/', 1)[1]: z.read(n) for n in z.namelist() if not n.endswith('/')}
        here = {str(f.relative_to(d)): f.read_bytes() for f in rp.package(d)}
        for f in sorted((set(here) | set(there)) - {m['module'], 'LICENSE', 'NOTICES'}):
            if f not in there:
                said.append(f'{at}: {f} is here, not in its release {tag}')
            elif f not in here:
                said.append(f'{at}: {f} is in its release {tag}, not here')
            elif here[f] != there[f]:
                said.append(f'{at}: {f} has changed since its release {tag}')
        # (its source, against its release's tag)
        try:
            names = git(d, 'ls-tree', '-r', '--name-only', tag).decode().split('\n')
        except subprocess.CalledProcessError:
            said.append(f'{at}: has no tag {tag} (git -C {at} fetch --tags)')
            continue
        old = next((p for p in (f'cores/{d.name}/', f'plugins/{d.name}/') if any(n.startswith(p) for n in names)), '')
        own = [f for f in git(d, 'ls-files').decode().split('\n') if f.startswith(('module/', 'crates/'))]
        pairs = [(d / f, (old + f) if f.startswith('module/') else f) for f in own]
        needs = (d / 'module/Cargo.toml').read_text()
        for name, path in SHARED.items():
            if f'{name} = ' in needs and any(n.startswith(path + '/') for n in names):
                pairs += [(ROOT / f, f) for f in git(ROOT, 'ls-files', path).decode().split('\n') if f]
        if 'wit/core.wit' in names:
            pairs.append((ROOT / 'wit/core.wit', 'wit/core.wit'))
        tagged = set(names)
        for local, path in pairs:
            if path not in tagged:
                said.append(f'{at}: {local.relative_to(ROOT)} is here, not in its release {tag}')
            elif not same(path, git(d, 'show', f'{tag}:{path}'), local.read_bytes()):
                said.append(f'{at}: {local.relative_to(ROOT)} has changed since its release {tag}')
    print('\n'.join(said))
    return 1 if said else 0


if __name__ == '__main__':
    sys.exit(main())
