"""The packages that come with Ectomatter written into the list of packages
(registry/packages.json): each one's release files made with
tools/release-package.py, and its entry (Ectonomic's, from
Ectonomic/ectomatter-<id>) put in the list in place of the one there; the
list's other entries kept as they are. Run after a package changes, with its
version raised for a release.

  list-packages.py [--out DIR]   the release files into DIR (target/releases)
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main(argv):
    out = argv[argv.index('--out') + 1] if '--out' in argv else str(ROOT / 'target/releases')
    path = ROOT / 'registry/packages.json'
    listing = json.loads(path.read_text()) if path.is_file() else {'format': 1, 'packages': []}
    ours = {}
    for d in sorted((ROOT / 'cores').glob('*/core.json')) + sorted((ROOT / 'plugins').glob('*/core.json')):
        folder = d.parent
        made = subprocess.run([sys.executable, '-I', str(ROOT / 'tools/release-package.py'), str(folder), '--out', out, '--repository', f'Ectonomic/ectomatter-{folder.name}'],
                              check=True, capture_output=True, text=True).stdout
        e = json.loads(made)
        ours[e['versions'][0]['manifest']['id']] = {'repository': e['repository'], 'publisher': 'Ectonomic', 'versions': e['versions']}
    kept = [e for e in listing['packages'] if (e.get('versions') or [{}])[0].get('manifest', {}).get('id') not in ours]
    listing['packages'] = list(ours.values()) + kept
    path.write_text(json.dumps(listing, indent=2, ensure_ascii=False) + '\n')
    print(f'{len(ours)} of Ectomatter\'s own, {len(kept)} others')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
