"""A core's or plugin's release files, the ones its GitHub release carries
(docs/making-a-package.md): its manifest (core.json), the package (<id>.ectocore,
a zip of its folder, the module's source and what building it left out) and
the package's checksum (<id>.ectocore.sha256). The same folder always makes
the same package, byte for byte, so the checksum the list of packages pins
is the one a release gives.

  release-package.py --list DIR
      the files of the package in DIR, one a line, as a release carries them
      and tools/install.sh installs them (LICENSE and NOTICES aside)
  release-package.py DIR [--out OUT] [--repository OWNER/NAME]
      the release files of the package in DIR (built: its module beside its
      manifest), into OUT/<id>/<version> (target/releases); prints the entry
      for the list of packages (registry/packages.json): its repository (the
      manifest's, else --repository), and this version with its manifest and
      checksum

The package carries its license (DIR's LICENSE, else this repository's when
the package is under the same license) and, for a module built with Cargo,
the notices of the libraries it's built from (tools/notices.py).
"""
import hashlib
import json
import os
import re
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# (git's own variables left out: a commit hook sets them for this repository,
# and they'd point a package's git there too)
GIT_ENV = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
# (what a package's folder holds that isn't the package: its module's source
# and build, the lab's copies of what it changed, and what its own repository
# keeps beside it: its libraries, tools, test cases, README, the pictures
# its README shows and license tables)
LEFT_OUT = re.compile(r'(^|/)(module|target)(/|$)|\.before-|(^|/)\.'
                      r'|^(crates|tools|parity|pictures|LICENSES)(/|$)|^(Cargo\.toml|Cargo\.lock|README\.md|REUSE\.toml)$')


def jsonc(text):
    """JSON with // and /* */ comments, as manifests are written."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == '\\' else 1
            out.append(text[i:j + 1])
            i = j + 1
        elif text.startswith('//', i):
            i = text.find('\n', i)
            i = n if i < 0 else i
        elif text.startswith('/*', i):
            i = text.find('*/', i) + 2
        else:
            out.append(c)
            i += 1
    return json.loads(''.join(out))


def tomllib_license():
    """The license this repository's own packages are under (its workspace's), if it has one."""
    try:
        import tomllib
        return tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['license']
    except (OSError, KeyError, ImportError):
        return None


def package(folder):
    """The package's files. A package that is a repository of its own (as each
    of Ectonomic's is) ships only what it has committed, and its built module:
    nothing left in its folder, kept there or ignored, goes out with it."""
    if (folder / '.git').exists():
        out = subprocess.run(['git', '-C', str(folder), 'ls-files', '-z'], check=True, capture_output=True, env=GIT_ENV).stdout
        found = [folder / f for f in out.decode().split('\0') if f]
        module = folder / jsonc((folder / 'core.json').read_text())['module']
        found += [module] if module.is_file() else []
    else:
        found = folder.rglob('*')
    return sorted({p for p in found if p.is_file() and not LEFT_OUT.search(str(p.relative_to(folder)))})


# (what a drawing must not carry out into a release: a picture inside it, such
# as the photo it was traced from, hidden or not, and anything its maker marked
# to delete before shipping)
UNSHIPPED = re.compile(rb'<image\b|before shipping', re.I)


def unshipped(files):
    """The package's drawings that still carry what a release mustn't."""
    return [name for name, data in files.items() if name.lower().endswith('.svg') and UNSHIPPED.search(data)]


def main(argv):
    if argv[:1] == ['--list'] and len(argv) == 2:
        folder = Path(argv[1]).resolve()
        print('\n'.join(str(f.relative_to(folder)) for f in package(folder) if f.name != 'LICENSE' or f.parent != folder))
        return 0
    if not argv or argv[0].startswith('-'):
        sys.exit(__doc__)
    folder = Path(argv[0]).resolve()
    out = Path(argv[argv.index('--out') + 1]) if '--out' in argv else ROOT / 'target/releases'
    text = (folder / 'core.json').read_text()
    m = jsonc(text)
    pid, version = m['id'], m['version']
    if not (folder / m['module']).is_file():
        sys.exit(f'release-package: {folder.name} isn\'t built: no {m["module"]}')
    at = out / pid / version
    at.mkdir(parents=True, exist_ok=True)
    (at / 'core.json').write_text(text)
    files = {str(f.relative_to(folder)): f.read_bytes() for f in package(folder)}
    held = unshipped(files)
    # (a module built without tools/build-module.sh keeps the builder's folders
    # in its error messages: the home folder, the folder it was built in)
    builders = [str(Path.home()), str(folder.parent.parent)]
    kept = [b for b in builders if b.encode() in files[m['module']]]
    if kept:
        sys.exit(f'release-package: {folder.name}\'s module holds the folder {kept[0]}; build it with tools/build-module.sh')
    if held:
        sys.exit(f'release-package: {", ".join(held)} in {folder.name} still holds a picture or a layer marked to delete before shipping')
    own = tomllib_license()
    if 'LICENSE' not in files and own and m.get('license') == own:
        files['LICENSE'] = (ROOT / 'LICENSE').read_bytes()
    if (folder / 'module/Cargo.toml').is_file() and (ROOT / 'tools/notices.py').is_file():
        files['NOTICES'] = subprocess.run([sys.executable, '-I', str(ROOT / 'tools/notices.py'), str(folder / 'module/Cargo.toml'), '--files', str(folder)],
                                          check=True, capture_output=True).stdout
    zf = at / f'{pid}.ectocore'
    with zipfile.ZipFile(zf, 'w', zipfile.ZIP_DEFLATED) as z:
        for name in sorted(files):
            # (each file dated the same, so the package is the same however old its files are)
            info = zipfile.ZipInfo(f'{pid}/{name}', date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            z.writestr(info, files[name])
    sha = hashlib.sha256(zf.read_bytes()).hexdigest()
    (at / f'{pid}.ectocore.sha256').write_text(f'{sha}  {pid}.ectocore\n')
    repo = m.get('repository') or (argv[argv.index('--repository') + 1] if '--repository' in argv else '')
    entry = {'repository': repo, 'versions': [{'manifest': m, 'sha256': sha}]}
    print(json.dumps(entry, indent=2, ensure_ascii=False))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
