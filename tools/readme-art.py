"""The games' art the README's Library and game pictures show
(docs/pictures/games.json: each game's Steam number, its name, and its
cover, banner and logo, each by its address on Steam's servers and the
checksum it must have), fetched into target/readme-art/<number>/, where the
showcase desk takes it from (apps/ectomatter/tests/renders.rs,
Desk::showcase). The art is the games' publishers', shown in those pictures
under fair use (LICENSES/LicenseRef-Fair-Use.txt); it isn't kept in this
repository.

  readme-art.py            fetches what isn't here yet, and checks it all
  readme-art.py --pin      writes each file's checksum into games.json (when
                           games.json changes; look at the art first)

Needs the network (Steam's servers) the first time.
"""
import hashlib
import json
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LIST = ROOT / 'docs/pictures/games.json'
OUT = ROOT / 'target/readme-art'
# (each picture's file name, as Steam's library cache names it)
NAMES = {'cover': 'library_600x900_2x.jpg', 'hero': 'library_hero.jpg', 'logo': 'logo.png'}


def fetch(url):
    with urllib.request.urlopen(urllib.request.Request(url, headers={'User-Agent': 'ectomatter-readme-art'}), timeout=60) as r:
        return r.read()


def main(argv):
    pin = argv[:1] == ['--pin']
    games = json.loads(LIST.read_text())
    said = []
    for g in games['games']:
        for kind, name in NAMES.items():
            art = g['art'][kind]
            at = OUT / str(g['id']) / name
            if not at.is_file() or pin:
                at.parent.mkdir(parents=True, exist_ok=True)
                at.write_bytes(fetch(art['url']))
            sha = hashlib.sha256(at.read_bytes()).hexdigest()
            if pin:
                art['sha256'] = sha
            elif sha != art.get('sha256'):
                said.append(f'{g["name"]}: its {kind} at {art["url"]} is not the one pinned (Steam changed it? look, then --pin)')
    if pin:
        LIST.write_text(json.dumps(games, indent=2, ensure_ascii=False) + '\n')
    print('\n'.join(said) if said else f'readme-art: {len(games["games"])} games\' art in {OUT.relative_to(ROOT)}')
    return 1 if said else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
