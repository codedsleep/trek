#!/usr/bin/env bash
set -euo pipefail
fixture_root="${XDG_CACHE_HOME:-$HOME/.cache}/trek-fixture"
if [[ ${1:-} == --clean ]]; then
    python3 - "$fixture_root" <<'PY'
import pathlib, shutil, sys
p=pathlib.Path(sys.argv[1])
if p.name != 'trek-fixture' or p.is_symlink():
    raise SystemExit('Refusing unexpected fixture path')
if p.exists():
    for entry in p.rglob('*'):
        if not entry.is_symlink():
            try: entry.chmod(0o700 if entry.is_dir() else 0o600)
            except OSError: pass
    shutil.rmtree(p)
PY
    exit 0
fi
python3 - "$fixture_root" <<'PY'
import pathlib, os, subprocess, sys
root=pathlib.Path(sys.argv[1])
for name in ['big','deep','empty','images','live','names','links','perm']: (root/name).mkdir(parents=True,exist_ok=True)
def touch(path, data=b'fixture\n'):
    if not path.exists() and not path.is_symlink(): path.write_bytes(data)
for i in range(50000): touch(root/'big'/f'file-{i:05d}.txt')
p=root/'deep'
for i in range(20):
    p=p/f'level-{i:02d}'; p.mkdir(exist_ok=True); touch(p/'item.txt')
for name in ['日本語.txt','éclair.txt','x'*240,'.hidden','backup~','two words','-leading','line\nbreak.txt']: touch(root/'names'/name)
for name,target in [('file','../names/two words'),('dir','../deep'),('broken','missing'),('loop','loop')]:
    p=root/'links'/name
    if not p.is_symlink(): p.symlink_to(target)
p=root/'perm'/'unreadable'; p.mkdir(exist_ok=True); p.chmod(0)
p=root/'perm'/'readonly'; touch(p); p.chmod(0o444)
for i in range(500):
    p=root/'images'/f'image-{i:03d}.png'
    if not p.exists(): subprocess.run(['magick','-size','160x120',f'xc:rgb({i%256},{(i*3)%256},{(i*7)%256})',str(p)],check=True)
print(root)
PY
