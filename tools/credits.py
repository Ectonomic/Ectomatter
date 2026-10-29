"""Ectomatter's credits, for Settings' About, from the project's own history
never typed by hand (as ArtCraft's About credits its makers):

  authors   each person who made a commit or is named a co-author in one, as
            they wrote their own name (git's %aN, through .mailmap), with
            their commits and the lines they added and took out, most
            commits first
  models    each AI model a commit names as its co-author, with its commits;
            a model's maker credits it under the maker's own name among the
            authors too (MAKERS)
  commits   how many commits the history holds

  credits.py   prints them as JSON

Reads only this repository's history.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

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


def credits():
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
    by = lambda kv: (-kv[1][0] if isinstance(kv[1], list) else -kv[1], kv[0])
    return {
        'authors': [{'name': n, 'commits': c, 'added': a, 'removed': r} for n, (c, a, r) in sorted(people.items(), key=by)],
        'models': [{'name': n, 'commits': c} for n, c in sorted(models.items(), key=by)],
        'commits': commits,
    }


if __name__ == '__main__':
    json.dump(credits(), sys.stdout, indent=1, ensure_ascii=False)
    sys.stdout.write('\n')
