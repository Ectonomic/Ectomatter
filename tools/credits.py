"""Ectomatter's credits, for Settings' About, from the project's own history
and files, never typed by hand (as ArtCraft's About credits its makers):

  authors   each person who made a commit or is named a co-author in one, as
            they wrote their own name (git's %aN, through .mailmap), with
            their commits and the lines they added and took out, most
            commits first
  models    each AI model a commit names as its co-author, with its commits;
            a model's maker credits it under the maker's own name among the
            authors too (MAKERS)
  commits   how many commits the history holds
  libraries how many libraries ship with it (as tools/notices.py counts them)
  typefaces how many typefaces it carries from others (REUSE.toml's tables
            whose license isn't Ectomatter's own, by family)

  credits.py [MANIFEST]   prints them as JSON (MANIFEST: Cargo.toml)

Reads only this repository's history and files, and Cargo's metadata.
Runs the Cargo $CARGO names, else the one on the PATH.
"""
import importlib.util
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location('notices', ROOT / 'tools' / 'notices.py')
notices = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(notices)

# the address an AI model's maker signs its co-author lines with, and the name
# the maker gives the models among the authors
MAKERS = {'noreply@anthropic.com': 'Claude'}

# a commit's fields, and its end (git's own separators can't be in a name)
FIELD, END = '\x1f', '\x1e'
TRAILER = re.compile(r'^(.*?)\s*<([^>]*)>\s*$')


def history():
    """Each commit: its author, its co-authors (name, address), the lines it
    added and took out."""
    out = subprocess.run(
        ['git', 'log', '--use-mailmap', '--numstat', f'--format={END}%aN{FIELD}%(trailers:key=Co-Authored-By,valueonly,separator=%x1f)'],
        cwd=ROOT, check=True, capture_output=True, text=True).stdout
    for chunk in out.split(END)[1:]:
        head, _, stats = chunk.partition('\n')
        author, *co = head.split(FIELD)
        added = removed = 0
        for line in stats.splitlines():
            a, _, rest = line.partition('\t')
            r = rest.partition('\t')[0]
            # (a picture's numbers are dashes)
            if a.isdigit() and r.isdigit():
                added, removed = added + int(a), removed + int(r)
        coauthors = []
        for c in co:
            m = TRAILER.match(c.strip())
            if m:
                coauthors.append((m.group(1).strip(), m.group(2).strip().lower()))
        yield author.strip(), coauthors, added, removed


def credits(manifest):
    people, models, commits = {}, {}, 0
    for author, coauthors, added, removed in history():
        commits += 1
        names = {author}
        for name, address in coauthors:
            if address in MAKERS:
                models[name] = models.get(name, 0) + 1
                names.add(MAKERS[address])
            elif name:
                names.add(name)
        for n in names:
            p = people.setdefault(n, [0, 0, 0])
            p[0], p[1], p[2] = p[0] + 1, p[1] + added, p[2] + removed
    libraries = len(notices.shipped(manifest)[0])
    ts = notices.tables()
    families = set()
    for f in notices.tracked():
        n = notices.table_of(f, ts)
        if notices.FONT.search(f) and n is not None and ts[n][notices.LICENSE] != notices.own_license():
            families.add(re.split(r'[-.]', Path(f).name)[0])
    by = lambda kv: (-kv[1][0] if isinstance(kv[1], list) else -kv[1], kv[0])
    return {
        'authors': [{'name': n, 'commits': c, 'added': a, 'removed': r} for n, (c, a, r) in sorted(people.items(), key=by)],
        'models': [{'name': n, 'commits': c} for n, c in sorted(models.items(), key=by)],
        'commits': commits,
        'libraries': libraries,
        'typefaces': len(families),
    }


if __name__ == '__main__':
    manifest = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'Cargo.toml'
    json.dump(credits(manifest), sys.stdout, indent=1, ensure_ascii=False)
    sys.stdout.write('\n')
