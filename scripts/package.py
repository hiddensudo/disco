#!/usr/bin/env python3
"""Package only reviewed public runtime files; never ship Cargo caches or local config."""
import gzip
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib


def package(binary, output, arch):
    root = Path(__file__).resolve().parents[1]
    binary = Path(binary).resolve()
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
    if arch not in ('amd64', 'arm64'):
        raise ValueError('Expected a native Linux amd64 or arm64 build')
    name = f'disco-{version}-linux-{arch}'
    with tempfile.TemporaryDirectory(prefix='disco-package-') as temporary:
        bundle = Path(temporary) / 'disco'
        bundle.mkdir()
        shutil.copy2(binary, bundle / 'disco')
        (bundle / 'disco').chmod(0o755)
        for filename in ('install.py', 'disco.desktop', 'README.md', 'README.uk.md', 'CHANGELOG.md',
                         'categories.example.toml', 'layout.default.toml'):
            shutil.copy2(root / filename, bundle / filename)
        for directory in ('assets', 'docs', 'licenses'):
            shutil.copytree(root / directory, bundle / directory)
        shutil.copy2(root / 'vendor/egui_term/LICENSE', bundle / 'licenses/egui_term-MIT.txt')
        shutil.copy2(root / 'vendor/trash/LICENSE.txt', bundle / 'licenses/trash.txt')
        report = subprocess.check_output(['ldd', str(binary)], text=True)
        symbols = subprocess.check_output(['objdump', '-T', str(binary)], text=True)
        import re
        versions = set(re.findall(r'GLIBC_(\d+(?:\.\d+)+)', symbols))
        minimum = max(versions, key=lambda s: tuple(map(int, s.split('.'))))
        report = f'{name}\nMinimum referenced glibc: {minimum}\n\n{report}'
        report += '\nGraphics are dynamically loaded: Wayland or X11, OpenGL/EGL, libxkbcommon and desktop drivers.\n'
        (bundle / 'runtime-dependencies.txt').write_text(report)
        (output / f'{name}-runtime.txt').write_text(report)
        archive = output / f'{name}.tar.gz'
        temporary_archive = archive.with_suffix('.gz.next')
        with temporary_archive.open('wb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w') as tar:
                def normalize(info):
                    info.uid = info.gid = 0
                    info.uname = info.gname = ''
                    info.mtime = 0
                    return info
                tar.add(bundle, arcname='disco', filter=normalize)
        os.replace(temporary_archive, archive)
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        (output / f'{name}.tar.gz.sha256').write_text(f'{digest}  {archive.name}\n')
        print(archive)
        print(report)


if __name__ == '__main__':
    package(*sys.argv[1:])
