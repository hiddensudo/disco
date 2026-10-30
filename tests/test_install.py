import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

INSTALL = Path(__file__).resolve().parents[1] / 'install.py'


class InstallerTests(unittest.TestCase):
    def test_install_is_repeatable_and_handles_spaces(self):
        with tempfile.TemporaryDirectory(prefix='disco-install-') as temporary:
            root = Path(temporary)
            home = root / 'home with spaces'
            binary = root / 'build with spaces/disco'
            binary.parent.mkdir()
            binary.write_text('#!/bin/sh\nexit 0\n')
            binary.chmod(0o755)
            for _ in range(2):
                subprocess.run([sys.executable, str(INSTALL), '--binary', str(binary)],
                               env=dict(os.environ, HOME=str(home), DISCO_CONFIG_DIR=str(home / '.config/disco')), check=True,
                               stdout=subprocess.PIPE, text=True)
            command = home / '.local/bin/disco'
            self.assertEqual(command.resolve(), binary)
            text = (home / '.local/share/applications/ua.disco.desktop').read_text()
            self.assertIn(f'Exec="{command}"', text)
            self.assertIn(f'TryExec={command}', text)
            self.assertIn(f'Icon={home / ".config/disco/app-icon-dark.svg"}', text)
            self.assertTrue((home / '.config/disco/app-icon-light.svg').is_file())
            registration = (home / '.config/disco/desktop-integration.toml').read_text()
            self.assertIn('ua.disco.desktop', registration)

    def test_install_selects_light_icon_from_saved_custom_theme(self):
        with tempfile.TemporaryDirectory(prefix='disco-install-light-') as temporary:
            home = Path(temporary)
            config = home / 'separate config'
            config.mkdir()
            (config / 'theme.toml').write_text('background = "#f2f4f7"\n')
            binary = home / 'disco'
            binary.write_text('#!/bin/sh\nexit 0\n')
            binary.chmod(0o755)
            subprocess.run([sys.executable, str(INSTALL), '--binary', str(binary)],
                           env=dict(os.environ, HOME=str(home), DISCO_CONFIG_DIR=str(config)),
                           check=True, capture_output=True)
            desktop = (home / '.local/share/applications/ua.disco.desktop').read_text()
            self.assertIn(f'Icon={config / "app-icon-light.svg"}', desktop)

    def test_development_install_keeps_one_disco_name_and_its_config(self):
        with tempfile.TemporaryDirectory(prefix='disco-install-dev-') as temporary:
            home = Path(temporary)
            config = home / 'development config'
            binary = home / 'build/disco'
            binary.parent.mkdir()
            binary.write_text('#!/bin/sh\nprintf "%s\\n" "$DISCO_CONFIG_DIR"\n')
            binary.chmod(0o755)
            entries = home / '.local/share/applications'
            entries.mkdir(parents=True)
            (entries / 'ua.disco.unstable.desktop').write_text('Name=disco Unstable\n')
            command_dir = home / '.local/bin'
            command_dir.mkdir(parents=True)
            (command_dir / 'disco-unstable').write_text('old launcher')
            subprocess.run([sys.executable, str(INSTALL), '--binary', str(binary), '--config-dir', str(config)],
                           env=dict(os.environ, HOME=str(home)), check=True, capture_output=True)
            output = subprocess.check_output([str(command_dir / 'disco')], text=True).strip()
            self.assertEqual(output, str(config))
            self.assertIn('Name=disco\n', (entries / 'ua.disco.desktop').read_text())
            self.assertFalse((entries / 'ua.disco.unstable.desktop').exists())
            self.assertFalse((command_dir / 'disco-unstable').exists())

    def test_missing_build_does_not_create_an_entry(self):
        with tempfile.TemporaryDirectory(prefix='disco-install-missing-') as temporary:
            home = Path(temporary)
            result = subprocess.run([sys.executable, str(INSTALL), '--binary', str(home / 'missing')],
                                    env=dict(os.environ, HOME=str(home)),
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((home / '.local/bin/disco').exists())
            self.assertFalse((home / '.local/share/applications/ua.disco.desktop').exists())

    def test_install_uses_active_catalog_theme_instead_of_legacy_backup(self):
        with tempfile.TemporaryDirectory(prefix='disco-install-catalog-') as temporary:
            home = Path(temporary)
            config = home / 'config'
            (config / 'themes').mkdir(parents=True)
            (config / 'theme.toml').write_text('background = "#101417"\n')
            (config / 'themes/paper.toml').write_text('background = "#f5f6f7"\n')
            (config / 'themes.toml').write_text('version=1\nactive="paper"\n[themes.paper]\nname="Paper"\nfile="themes/paper.toml"\n')
            binary = home / 'disco'
            binary.write_text('#!/bin/sh\nexit 0\n')
            binary.chmod(0o755)
            subprocess.run([sys.executable, str(INSTALL), '--binary', str(binary)],
                           env=dict(os.environ, HOME=str(home), DISCO_CONFIG_DIR=str(config)),
                           check=True, capture_output=True)
            desktop = (home / '.local/share/applications/ua.disco.desktop').read_text()
            self.assertIn(f'Icon={config / "app-icon-light.svg"}', desktop)


if __name__ == '__main__':
    unittest.main()
