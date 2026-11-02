"""Each of Ectomatter's own packages as it stands here held to its release:
its repository as tools/package-repos.py lays it out now, file for file,
against the source at the release's tag on GitHub (the version the list
names). A package changed since its release, without a new version, shows
here. The README is left out: it may change on the repository's main branch
between releases. The built module isn't compared, only its source: built in
another folder, the same source can give a few bytes of difference
(docs/deferred.md).

  released.py   prints what differs, a line each, and exits 1 when anything does

Needs the network (github.com) and Cargo (the libraries' descriptions).
"""
import io
import json
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def tagged(repo, tag):
    """The files of `repo` at `tag`, by path."""
    url = f'https://github.com/{repo}/archive/refs/tags/{tag}.tar.gz'
    with urllib.request.urlopen(urllib.request.Request(url, headers={'User-Agent': 'ectomatter-released'}), timeout=60) as r:
        t = tarfile.open(fileobj=io.BytesIO(r.read()), mode='r:gz')
    out = {}
    for m in t.getmembers():
        if m.isfile():
            out[m.name.split('/', 1)[1]] = t.extractfile(m).read()
    return out


def main():
    listed = {(e['versions'][-1]['manifest']['id']): e for e in json.loads((ROOT / 'registry/packages.json').read_text())['packages']}
    said = []
    with tempfile.TemporaryDirectory() as tmp:
        subprocess.run([sys.executable, '-I', str(ROOT / 'tools/package-repos.py'), tmp], check=True, capture_output=True)
        for at in sorted(Path(tmp).glob('ectomatter-*')):
            here = {str(p.relative_to(at)): p.read_bytes() for p in at.rglob('*') if p.is_file() and str(p.relative_to(at)) != 'README.md'}
            manifest = next(p for p in at.glob('*/*/core.json'))
            pid = json.loads(subprocess.run([sys.executable, '-I', '-c',
                                             'import importlib.util,sys,json;s=importlib.util.spec_from_file_location("r",sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);print(json.dumps(m.jsonc(open(sys.argv[2]).read())))',
                                             str(ROOT / 'tools/release-package.py'), str(manifest)], check=True, capture_output=True, text=True).stdout)['id']
            e = listed.get(pid)
            if not e:
                said.append(f'{pid}: not on the list')
                continue
            tag = e['versions'][-1]['manifest']['version']
            try:
                there = {k: v for k, v in tagged(e['repository'], tag).items() if k != 'README.md'}
            except Exception as err:
                said.append(f'{pid}: its release {tag} can\'t be read from {e["repository"]} ({err})')
                continue
            for f in sorted(set(here) | set(there)):
                if f not in there:
                    said.append(f'{pid}: {f} is here, not in its release {tag}')
                elif f not in here:
                    said.append(f'{pid}: {f} is in its release {tag}, not here')
                elif here[f] != there[f]:
                    said.append(f'{pid}: {f} has changed since its release {tag}')
    print('\n'.join(said))
    return 1 if said else 0


if __name__ == '__main__':
    sys.exit(main())
