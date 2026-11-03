"""The notices that go with Ectomatter, or with a package of it: each library
it's built from, with the license text the library ships (or the license's
own text, from LICENSES/, when it ships none), and each file it carries from
others (REUSE.toml's tables after the first), with their copyright and
license text. A text many share is written once, under all their names.

  notices.py MANIFEST [--files DIR]...   the notices of the project at
      MANIFEST (a Cargo.toml): the libraries that ship with it (every crate
      its crates reach, on every system and with every feature, not through
      the tests' own dependencies; as tools/licenses.sh holds them), and the
      carried files under DIR (its crates' folders when no --files)
  notices.py --check                     every file that shows another's
      work has a table of its own in REUSE.toml, each license a table names
      has its text, and is one deny.toml approves; prints what doesn't, one
      line each: `unannotated PATH`, `stale PATTERN`, `no-text ID`,
      `unapproved ID PATH...`, and exits 1 when anything is printed

Reads only this repository, the packages linked in it (each core's and
plugin's own repository, at cores/<name> and plugins/<name>: their files and
their own REUSE.toml), Cargo's metadata and the crates' own folders.
Runs the Cargo $CARGO names, else the one on the PATH.
"""
import fnmatch
import importlib.util
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TEXTS = ROOT / 'LICENSES'
# REUSE.toml's keys (split, so this file doesn't show them: SHOWS)
WHO, LICENSE = 'SPDX-File' + 'CopyrightText', 'SPDX-License' + '-Identifier'
# (git's own variables left out: a commit hook sets them for this repository,
# and they'd point a package's git there too)
GIT_ENV = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}


def own_license():
    return tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['license']


def approved():
    deny = tomllib.loads((ROOT / 'deny.toml').read_text())['licenses']
    allow = set(deny.get('allow', [])) | {own_license()}
    per_crate = {}
    for e in deny.get('exceptions', []):
        per_crate.setdefault(e['crate'], set()).update(e['allow'])
    return list(deny.get('allow', [])), allow, per_crate


# ---- licenses as SPDX writes them

def tokens(expr):
    return re.findall(r'\(|\)|[A-Za-z0-9.+:-]+', expr.replace('/', ' OR '))


def alternatives(expr):
    """The expression as its choices: each a list of terms that all apply
    (`MIT`, `Apache-2.0 WITH LLVM-exception`)."""
    t = tokens(expr)
    at = 0

    def either():
        nonlocal at
        out = both()
        while at < len(t) and t[at] == 'OR':
            at += 1
            out = out + both()
        return out

    def both():
        nonlocal at
        out = one()
        while at < len(t) and t[at] == 'AND':
            at += 1
            more = one()
            out = [a + b for a in out for b in more]
        return out

    def one():
        nonlocal at
        if t[at] == '(':
            at += 1
            out = either()
            at += 1
            return out
        term = t[at]
        at += 1
        if at < len(t) and t[at] == 'WITH':
            term += ' WITH ' + t[at + 1]
            at += 2
        return [[term]]

    return either() if t else [[]]


def ids(term):
    return term.split(' WITH ')


def text_of(i):
    if i == own_license():
        return (ROOT / 'LICENSE').read_text()
    f = TEXTS / f'{i}.txt'
    if not f.is_file():
        sys.exit(f'notices: no text for {i}: LICENSES/{i}.txt')
    return f.read_text()


# ---- the libraries

KIND = [  # a license file's name, and the license it holds
    (r'mit', 'MIT'), (r'apache', 'Apache-2.0'), (r'zlib', 'Zlib'), (r'bsl|boost', 'BSL-1.0'),
    (r'unlicense', 'Unlicense'), (r'isc', 'ISC'), (r'bsd', 'BSD-3-Clause'), (r'unicode', 'Unicode-3.0'),
]
LICENSE_FILE = re.compile(r'(?i)^(licen[sc]e|copying|copyright|notice|unlicense|.*-licen[sc]e)')


def shipped(manifest):
    md = json.loads(subprocess.run(
        [os.environ.get('CARGO', 'cargo'), 'metadata', '--manifest-path', str(manifest), '--locked', '--all-features', '--format-version', '1'],
        check=True, capture_output=True, text=True).stdout)
    pk = {p['id']: p for p in md['packages']}
    nodes = {n['id']: n for n in md['resolve']['nodes']}
    seen, todo = set(), list(md['workspace_members'])
    while todo:
        i = todo.pop()
        if i in seen:
            continue
        seen.add(i)
        todo += [d['pkg'] for d in nodes[i]['deps'] if any(k['kind'] != 'dev' for k in d['dep_kinds'])]
    members = [pk[i] for i in md['workspace_members']]
    # (a crate from a folder, not a registry, is one of Ectomatter's own)
    return sorted((pk[i] for i in seen if pk[i]['source']), key=lambda p: (p['name'], p['version'])), members


def library(p, order, allow, per_crate):
    """The crate's heading, and the texts that go with it."""
    here = Path(p['manifest_path']).parent
    files = sorted(f for f in here.iterdir() if f.is_file() and LICENSE_FILE.match(f.name))
    if p.get('license_file'):
        files.append(here / p['license_file'])
    expr = p.get('license') or ''
    ok = allow | per_crate.get(p['name'], set())

    def kind(f):
        n = f.name.lower()
        return next((k for pat, k in KIND if re.search(pat, n.split('.')[0].replace('licence', 'license').replace('license', ''))), None)

    def covered(alt):
        kinds = {kind(f) for f in files}
        return all(ids(t)[0] in kinds for t in alt)

    alts = alternatives(expr) if expr else []
    allowed = [a for a in alts if all(t in ok or ids(t)[0] in ok for t in a)] or alts
    allowed.sort(key=lambda a: min((order.index(ids(t)[0]) for t in a if ids(t)[0] in order), default=99))
    alt = next((a for a in allowed if covered(a)), allowed[0] if allowed else [])
    taken = {ids(t)[0] for t in alt}
    texts = []
    for f in files:
        k = kind(f)
        if k is None or k in taken or not alt:
            texts.append(f.read_text(errors='replace'))
    have = {kind(f) for f in files}
    generic = None in have
    for t in alt:
        for n, i in enumerate(ids(t)):
            if i not in have and not (generic and n == 0):
                texts.append(text_of(i))
    authors = '; '.join(a for a in p.get('authors', []) if a)
    head = f"{p['name']} {p['version']}" + (f" ({' AND '.join(alt)})" if alt else '') + (f', by {authors}' if authors else '')
    return head, texts


# ---- the files carried from others

def packages():
    """The packages linked here: each core's and plugin's own repository."""
    return sorted(d for kind in ('cores', 'plugins') for d in (ROOT / kind).glob('*') if (d / 'core.json').is_file())


def files_of(root):
    out = subprocess.run(['git', 'ls-files', '-z'], cwd=root, check=True, capture_output=True, env=GIT_ENV).stdout
    return [f for f in out.decode().split('\0') if f and (root / f).is_file()]


def tracked():
    """This repository's files and each package's, by their paths here."""
    files = files_of(ROOT)
    for d in packages():
        files += [f'{d.relative_to(ROOT)}/{f}' for f in files_of(d)]
    return files


def glob(pattern):
    """A REUSE path: `**` anything, `*` anything but a slash."""
    rx = ''
    for part in re.split(r'(\*\*/?|\*)', pattern):
        rx += '.*' if part.startswith('**') else '[^/]*' if part == '*' else re.escape(part)
    return re.compile(rx + r'\Z')


def tables():
    """REUSE.toml's tables, then each package's own after its first (which,
    as here, says every file is Ectonomic's), their paths as they are here;
    a package's tables carry its folder (`at`)."""
    def read(path, at=None):
        t = tomllib.loads(path.read_text())['annotations']
        for a in t:
            for k in ('path', WHO):
                if isinstance(a.get(k), str):
                    a[k] = [a[k]]
            if at:
                a['path'] = [f'{at}/{p}' for p in a['path']]
            a['rx'] = [glob(p) for p in a['path']]
            a['at'] = at
        return t
    t = read(ROOT / 'REUSE.toml')
    for d in packages():
        if (d / 'REUSE.toml').is_file():
            t += read(d / 'REUSE.toml', str(d.relative_to(ROOT)))[1:]
    return t


def table_of(path, ts):
    """The last table naming the file (REUSE's rule); None for the first,
    which says every file is Ectomatter's own."""
    hit = None
    for n, a in enumerate(ts):
        if any(r.match(path) for r in a['rx']):
            hit = n
    return hit or None


def left_out():
    """What a package's folder holds that its release leaves out
    (tools/release-package.py's LEFT_OUT)."""
    spec = importlib.util.spec_from_file_location('release_package', ROOT / 'tools/release-package.py')
    rp = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(rp)
    return rp.LEFT_OUT


def carried(dirs, files, ts):
    under = [str(Path(d).resolve().relative_to(ROOT)).rstrip('/') + '/' for d in dirs]
    # (a package's folder: only what its release carries, not what its own
    # repository keeps beside it)
    packages = [u for u in under if (ROOT / u / 'core.json').is_file()]
    left = left_out() if packages else None
    groups = {}
    for f in files:
        if not any(f.startswith(u) or u == './' for u in under):
            continue
        if any(f.startswith(u) and left.search(f[len(u):]) for u in packages):
            continue
        n = table_of(f, ts)
        if n is not None and ts[n][LICENSE] != own_license():
            groups.setdefault(n, []).append(f)
    out = []
    for n, fs in sorted(groups.items()):
        a = ts[n]
        expr = a[LICENSE]
        head = '\n'.join(fs) + f'\n({expr})\n' + '\n'.join(f'Copyright {c}' for c in a[WHO])
        texts = [text_of(i) for t in alternatives(expr)[0] for i in ids(t)]
        out.append((head, texts))
    return out


# ---- the check

SHOWS = re.compile('|'.join([  # (split, so this file doesn't show them)
    WHO, LICENSE, 'Permission is hereby' + ' granted',
    '<copy' + 'right>', 'Redistribution and use' + ' in source', 'Licensed under the' + ' Apache']))
FONT = re.compile(r'(?i)\.(ttf|otf|ttc|woff2?)$')
NOTICE = re.compile(r'(?i)^((readme|notice|licen[sc]e|copying)(\.(md|txt))?|.*ofl\.txt|.*-licen[sc]e\.txt)$')


def check():
    ts = tables()
    files = tracked()
    _, allow, _ = approved()
    said = []
    # (each repository's own REUSE.toml, LICENSE and LICENSES/: here's and each package's)
    tops = [''] + [f'{d.relative_to(ROOT)}/' for d in packages()]
    skip = {top + f for top in tops for f in ('REUSE.toml', 'LICENSE')}
    folders = {}
    for f in files:
        folders.setdefault(str(Path(f).parent), []).append(f)
    # a folder whose notice or readme names a license holds others' work
    marked = set()
    for d, fs in folders.items():
        if d == '.' or d + '/' in tops:
            continue
        for f in fs:
            name = Path(f).name
            if NOTICE.search(name) and (not name.lower().startswith('readme') or re.search(r'(?i)licen[sc]e', (ROOT / f).read_text(errors='replace'))):
                marked.add(d)
    for f in files:
        if f in skip or any(f.startswith(top + 'LICENSES/') for top in tops):
            continue
        name = Path(f).name
        shows = FONT.search(f) or (str(Path(f).parent) in marked and not name.lower().startswith('readme'))
        if not shows and not f.endswith('.md'):
            try:
                shows = SHOWS.search((ROOT / f).read_text())
            except (UnicodeDecodeError, OSError):
                shows = False
        if shows and table_of(f, ts) is None:
            said.append(f'unannotated {f}')
    used = {}
    for n, a in enumerate(ts):
        for p, r in zip(a['path'], a['rx']):
            if not any(r.match(f) for f in files):
                said.append(f'stale {p}')
        if n == 0:
            continue
        for t in alternatives(a[LICENSE])[0]:
            for i in ids(t):
                used.setdefault(i, []).extend(a['path'])
                # (a package's own repository carries the texts its tables name)
                if a['at'] and i != own_license() and not (ROOT / a['at'] / 'LICENSES' / f'{i}.txt').is_file():
                    said.append(f'no-text {i} {a["at"]}/LICENSES')
    for i, paths in sorted(used.items()):
        if i != own_license() and not (TEXTS / f'{i}.txt').is_file():
            said.append(f'no-text {i}')
        if i not in allow and not any(t.endswith(' WITH ' + i) for t in allow):
            said.append(f'unapproved {i} ' + ' '.join(paths))
    print('\n'.join(said))
    return 1 if said else 0


def main(argv):
    if argv[:1] == ['--check']:
        return check()
    manifest, dirs = Path(argv[0]), []
    rest = argv[1:]
    while rest:
        if rest[0] == '--files':
            dirs.append(rest[1])
            rest = rest[2:]
        else:
            sys.exit(f'notices: what\'s {rest[0]}?')
    order, allow, per_crate = approved()
    libs, members = shipped(manifest)
    if not dirs:
        dirs = [str(Path(m['manifest_path']).parent) for m in members]
    groups, order_seen = {}, []
    for head, texts in [library(p, order, allow, per_crate) for p in libs] + carried(dirs, tracked(), tables()):
        key = tuple(' '.join(t.split()) for t in texts)
        if key not in groups:
            groups[key] = (texts, [])
            order_seen.append(key)
        groups[key][1].append(head)
    names = ', '.join(m['name'] for m in members)
    out = [f'The notices of what {names} is built from and carries, each under its own',
           'license: its name, then the license text it comes with. Made by',
           f'tools/notices.py from Cargo.lock and REUSE.toml; {len(libs)} libraries.', '']
    for key in order_seen:
        texts, heads = groups[key]
        out += ['=' * 78, *heads, '-' * 78]
        for t in texts:
            out += [t.rstrip(), '']
    sys.stdout.write('\n'.join(out) + '\n')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
