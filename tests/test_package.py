import hashlib
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('disco_package', ROOT / 'scripts/package.py')
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)


class PackageTests(unittest.TestCase):
    def test_bundle_installs_after_relocation_without_container_paths(self):
        with tempfile.TemporaryDirectory(prefix='disco-bundle-') as temporary:
            root = Path(temporary)
            binary = root / 'built executable'
            binary.write_text('#!/bin/sh\nprintf "bundle works\\n"\n')
            binary.chmod(0o755)
            out = root / 'artifacts with spaces'
            with patch.object(packager.subprocess, 'check_output', side_effect=[
                    'libc.so.6 => /lib/libc.so.6\n', 'GLIBC_2.17 GLIBC_2.36']):
                packager.package(binary, out, 'amd64')
            archive, = out.glob('*.tar.gz')
            checksum, = out.glob('*.sha256')
            self.assertEqual(checksum.read_text().split()[0], hashlib.sha256(archive.read_bytes()).hexdigest())
            relocated = root / 'relocated bundle'
            relocated.mkdir()
            with tarfile.open(archive) as tar:
                names = tar.getnames()
                self.assertIn('disco/assets/disco-dark.svg', names)
                self.assertIn('disco/licenses/egui_term-MIT.txt', names)
                self.assertIn('disco/licenses/egui_commonmark-MIT.txt', names)
                self.assertIn('disco/licenses/egui_commonmark-APACHE.txt', names)
                self.assertIn('disco/assets/file-icons/rust.svg', names)
                self.assertIn('disco/licenses/material-icon-theme-MIT.txt', names)
                self.assertFalse(any('/target/' in name or '/.git/' in name for name in names))
                tar.extractall(relocated, filter='data')
            bundle = relocated / 'disco'
            home = root / 'home'
            subprocess.run([sys.executable, str(bundle / 'install.py'), '--binary', str(bundle / 'disco')],
                           env=dict(os.environ, HOME=str(home), DISCO_CONFIG_DIR=str(home / 'config')),
                           check=True, capture_output=True)
            self.assertEqual(subprocess.check_output([str(home / '.local/bin/disco')], text=True).strip(), 'bundle works')
            self.assertNotIn('/source/', (home / '.local/share/applications/ua.disco.desktop').read_text())
            self.assertIn('Minimum referenced glibc: 2.36', (bundle / 'runtime-dependencies.txt').read_text())


if __name__ == '__main__':
    unittest.main()
